//! Trabajo en segundo plano, sesión y procesado de mensajes.

use super::*;

impl App {
    // ---------- Tareas en segundo plano ----------

    pub(super) fn tarea<F>(&self, f: F)
    where
        F: Future<Output = Msg> + Send + 'static,
    {
        let tx = self.tx.clone();
        self.rt.spawn(async move { tx.enviar(f.await) });
    }

    pub(super) fn cargar_inicio(&mut self) {
        let http = self.http.clone();
        let cookie = self.sesion.as_ref().map(|s| s.cookie().to_string());
        self.tarea(async move {
            Msg::Inicio(api::secciones_inicio(&http, cookie.as_deref()).await.map_err(texto_error))
        });
    }

    pub(super) fn comprobar_plataforma(&self) {
        let tx = self.tx.clone();
        self.rt.spawn_blocking(move || {
            tx.enviar(Msg::Plataforma {
                aviso: plataforma::estado_cliente(),
                enlaces: plataforma::protocolos_registrados(),
            })
        });
    }

    /// Mantenimiento mientras el launcher está abierto (se pausa al jugar):
    /// - cada 30 s, DNS: algunos dominios de Roblox caducan a los ~37 s en la
    ///   caché del sistema, así que se renuevan para que Jugar los encuentre;
    /// - cada 10 min, actualización del cliente (Windows) y precarga de sus
    ///   archivos, por si el sistema liberó esa memoria.
    pub(super) fn mantenimiento_en_fondo(&self) {
        let tx = self.tx.clone();
        let jugando = self.jugando.clone();
        let actualizar = plataforma::ACTUALIZA_CLIENTE && self.cfg.actualizar_en_segundo_plano;
        let precargar = self.cfg.precargar;
        self.rt.spawn(async move {
            let diez_min = Duration::from_secs(10 * 60);
            let mut ultima: Option<Instant> = None;
            loop {
                if !jugando.load(Ordering::Relaxed) {
                    if ultima.is_none_or(|t| t.elapsed() >= diez_min) {
                        ultima = Some(Instant::now());
                        if actualizar {
                            let p = tx.clone();
                            let r = plataforma::actualizar(Arc::new(move |x| p.enviar(Msg::ProgresoFondo(x)))).await;
                            tx.enviar(Msg::FinFondo(r.map(|_| ()).map_err(texto_error)));
                        }
                        if precargar {
                            let rutas = plataforma::rutas_precarga();
                            let t = Instant::now();
                            let r = tokio::task::spawn_blocking(move || acelerar::precargar(&rutas, 768 << 20)).await;
                            if let Ok((n, bytes)) = r {
                                crate::perfil::marca(&format!(
                                    "precarga: {n} archivos, {} MB pedidos en {} ms",
                                    bytes >> 20,
                                    t.elapsed().as_millis()
                                ));
                            }
                        }
                    }
                    acelerar::calentar_dns().await;
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    }

    pub(super) fn calentar_dns(&mut self) {
        if self.dns_calentado.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            return;
        }
        self.dns_calentado = Some(Instant::now());
        self.rt.spawn(acelerar::calentar_dns());
    }

    pub(super) fn buscar(&mut self) {
        let q = self.consulta.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.busqueda_n += 1;
        let n = self.busqueda_n;
        self.buscada = q.clone();
        self.busqueda = Carga::Cargando;
        self.foco_busqueda = None;
        let http = self.http.clone();
        self.tarea(async move { Msg::Busqueda(n, api::buscar(&http, &q).await.map_err(texto_error)) });
    }

    pub(super) fn abrir_info(&mut self, universe_id: u64) {
        self.info = Some(universe_id);
        self.calentar_dns();
        if matches!(self.detalles.get(&universe_id), None | Some(Carga::Error(_))) {
            self.detalles.insert(universe_id, Carga::Cargando);
            let http = self.http.clone();
            self.tarea(async move {
                Msg::Detalle(universe_id, api::detalle(&http, universe_id).await.map_err(texto_error))
            });
        }
    }

    pub(super) fn jugar(&mut self, juego: Juego) {
        let uri = format!("roblox://experiences/start?placeId={}", juego.place_id);
        let titulo = juego.nombre.clone();
        self.fondo_a(juego.universe_id);
        self.cfg.anotar_reciente(juego);
        self.lanzar(titulo, uri);
    }

    pub(super) fn lanzar(&mut self, titulo: String, uri: String) {
        if self.lanzamiento.as_ref().is_some_and(|l| l.resultado.is_none()) {
            return;
        }
        self.lanzamiento = Some(Lanzamiento {
            titulo,
            progreso: Progreso::Comprobando,
            resultado: None,
            fin: None,
            fraccion: 0.0,
            hito: None,
            abierto: None,
        });
        // Sober ausente: se sabe desde el arranque, sin lanzar procesos ahora.
        if let Some(aviso) = self.aviso_cliente.clone() {
            if let Some(l) = &mut self.lanzamiento {
                l.resultado = Some(Err(aviso));
            }
            return;
        }
        self.fingir_activo = self.cfg.cliente.fingir_fps && !uri.is_empty();
        let ajustes = self.cfg.cliente.clone();
        let cerrar_al_salir = self.cfg.cerrar_al_salir;
        let tx = self.tx.clone();
        let jugando = self.jugando.clone();
        // El DNS se calienta en paralelo: el lanzamiento no lo espera.
        self.rt.spawn(acelerar::calentar_dns());
        self.rt.spawn(async move {
            let p = tx.clone();
            let desde = SystemTime::now();
            let r = plataforma::lanzar(&uri, &ajustes, Arc::new(move |x| p.enviar(Msg::ProgresoLanzar(x)))).await;
            match r {
                Ok(mut hijo) => {
                    jugando.store(true, Ordering::Relaxed);
                    tx.enviar(Msg::FinLanzar(Ok(())));
                    // El registro del cliente mueve la barra de carga y detecta
                    // la salida del juego (para cerrarlo y volver al launcher).
                    let pid = hijo.id();
                    let (dir, filtro) = plataforma::registros();
                    let activo = jugando.clone();
                    let txv = tx.clone();
                    tokio::spawn(async move {
                        let salio = vigia::vigilar(dir, filtro, desde, activo, move |e| {
                            if let Evento::Progreso(f, t) = e {
                                txv.enviar(Msg::Hito(f, t));
                            }
                        })
                        .await;
                        if salio && cerrar_al_salir {
                            let _ = tokio::task::spawn_blocking(move || plataforma::cerrar_cliente(pid)).await;
                        }
                    });
                    let _ = tokio::task::spawn_blocking(move || hijo.wait()).await;
                    jugando.store(false, Ordering::Relaxed);
                    tx.enviar(Msg::JuegoCerrado);
                }
                Err(e) => tx.enviar(Msg::FinLanzar(Err(texto_error(e)))),
            }
        });
    }

    // ---------- Sesión ----------

    pub(super) fn abrir_sesion_guardada(&mut self) {
        #[cfg(debug_assertions)]
        if std::env::var_os("RUST_LAUNCHER_DATOS").is_some() {
            // Pruebas: no tocar el almacén de credenciales real.
            self.sesion_comprobada = true;
            return;
        }
        if std::env::var_os("RUST_LAUNCHER_MEDIR").is_some() || crate::perfil::activo() {
            return;
        }
        let http = self.http.clone();
        self.tarea(async move {
            let Ok(Some(cookie)) = tokio::task::spawn_blocking(sesion::leer_cookie).await else {
                return Msg::Sesion(Err(String::new()));
            };
            let r = Sesion::abrir(http, cookie).await;
            if r.is_err() {
                let _ = tokio::task::spawn_blocking(sesion::borrar_cookie).await;
            }
            Msg::Sesion(r.map(Arc::new).map_err(texto_error))
        });
    }

    /// Crea un código de inicio de sesión y espera a que se apruebe.
    pub(super) fn iniciar_login(&mut self) {
        self.login_cancelar.store(true, Ordering::Relaxed);
        let cancelar = Arc::new(AtomicBool::new(false));
        self.login_cancelar = cancelar.clone();
        self.login = Login::Creando;
        self.qr = None;
        let http = self.http.clone();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let csrf = Mutex::new(String::new());
            let codigo = match sesion::crear_codigo(&http, &csrf).await {
                Ok(c) => c,
                Err(e) => return tx.enviar(Msg::Codigo(Err(texto_error(e)))),
            };
            tx.enviar(Msg::Codigo(Ok(codigo.clone())));
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                if cancelar.load(Ordering::Relaxed) {
                    sesion::cancelar_codigo(&http, &codigo, &csrf).await;
                    return;
                }
                let estado = sesion::estado_codigo(&http, &codigo, &csrf).await;
                let fin = matches!(estado, Ok(EstadoCodigo::Validado | EstadoCodigo::Cancelado) | Err(_));
                let validado = matches!(estado, Ok(EstadoCodigo::Validado));
                tx.enviar(Msg::EstadoCodigo(estado.map_err(texto_error)));
                if validado {
                    let r = async {
                        let cookie = sesion::completar(&http, &codigo, &csrf).await?;
                        let s = Sesion::abrir(http.clone(), cookie.clone()).await?;
                        tokio::task::spawn_blocking(move || sesion::guardar_cookie(&cookie)).await??;
                        anyhow::Ok(s)
                    }
                    .await;
                    tx.enviar(Msg::Sesion(r.map(Arc::new).map_err(texto_error)));
                }
                if fin {
                    return;
                }
            }
        });
    }

    pub(super) fn cargar_avatar(&mut self) {
        let Some(s) = self.sesion.clone() else { return };
        if !matches!(self.avatar, Carga::Listo(_)) {
            self.avatar = Carga::Cargando;
        }
        self.tarea(async move { Msg::Avatar(s.avatar().await.map_err(texto_error)) });
    }

    pub(super) fn cargar_categoria(&mut self, cat: Categoria) {
        let Some(s) = self.sesion.clone() else { return };
        if cat == Categoria::Trajes {
            if matches!(self.trajes, Carga::Nada | Carga::Error(_)) {
                self.trajes = Carga::Cargando;
                self.tarea(async move { Msg::Trajes(s.trajes().await.map_err(texto_error)) });
            }
        } else if matches!(self.inventario.get(&cat), None | Some(Carga::Error(_))) {
            self.inventario.insert(cat, Carga::Cargando);
            self.tarea(async move { Msg::Inventario(cat, s.inventario(cat).await.map_err(texto_error)) });
        }
    }

    pub(super) fn tras_cambio_avatar(&mut self) {
        if let Some(s) = &self.sesion {
            let id = s.usuario.id;
            self.imgs.olvidar((TipoImg::Avatar, id));
            self.imgs.olvidar((TipoImg::Cabeza, id));
        }
        self.cargar_avatar();
    }

    pub(super) fn poner_traje(&mut self, t: &Traje) {
        let Some(s) = self.sesion.clone() else { return };
        self.guardando = true;
        self.aviso_avatar = None;
        let id = t.id;
        self.tarea(async move { Msg::CambioAvatar(s.poner_traje(id).await.map_err(texto_error)) });
    }

    pub(super) fn alternar_objeto(&mut self, obj: &Objeto) {
        let (Some(s), Carga::Listo(actual)) = (self.sesion.clone(), &mut self.avatar) else { return };
        let antes = actual.clone();
        // Cambio inmediato en pantalla; Roblox confirma después.
        if actual.puestos.iter().any(|p| p.id == obj.id) {
            actual.puestos.retain(|p| p.id != obj.id);
        }
        self.guardando = true;
        self.aviso_avatar = None;
        let obj = obj.clone();
        self.tarea(async move { Msg::CambioAvatar(s.alternar(&antes, &obj).await.map_err(texto_error)) });
    }

    pub(super) fn cambiar_cuerpo(&mut self, r15: bool) {
        let Some(s) = self.sesion.clone() else { return };
        if let Carga::Listo(a) = &mut self.avatar {
            a.r15 = r15;
        }
        self.guardando = true;
        self.tarea(async move { Msg::CambioAvatar(s.tipo_cuerpo(r15).await.map_err(texto_error)) });
    }

    pub(super) fn cerrar_sesion(&mut self) {
        if let Some(s) = self.sesion.take() {
            self.rt.spawn(async move { s.cerrar().await });
        }
        self.avatar = Carga::Nada;
        self.trajes = Carga::Nada;
        self.inventario.clear();
        self.login = Login::Nada;
    }

    // ---------- Mensajes ----------

    pub(super) fn procesar(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Inicio(Ok(s)) => {
                    config::guardar_inicio(&s);
                    self.inicio = s;
                    self.inicio_estado = Carga::Listo(());
                }
                Msg::Inicio(Err(e)) => {
                    if self.inicio.is_empty() {
                        self.inicio_estado = Carga::Error(e);
                    }
                }
                Msg::Busqueda(n, r) if n == self.busqueda_n => self.busqueda = Carga::de(r),
                Msg::Busqueda(..) => {}
                Msg::Detalle(id, r) => {
                    self.detalles.insert(id, Carga::de(r));
                }
                Msg::Imagen(k, img) => self.imgs.recibir(ctx, k, img),
                Msg::Qr(url, img) => {
                    if let (Some((actual, tex)), Some(img)) = (&mut self.qr, img)
                        && *actual == url
                    {
                        *tex = Some(ctx.load_texture("qr", img, egui::TextureOptions::NEAREST));
                    }
                }
                Msg::Plataforma { aviso, enlaces } => {
                    self.aviso_cliente = aviso;
                    self.enlaces_web = enlaces;
                }
                Msg::ProgresoFondo(p) => self.fondo_progreso = Some(p),
                Msg::FinFondo(r) => {
                    self.fondo_progreso = None;
                    self.fondo_error = r.err();
                }
                Msg::ProgresoLanzar(p) => {
                    if let Some(l) = &mut self.lanzamiento {
                        l.progreso = p;
                    }
                }
                Msg::FinLanzar(r) => {
                    if let Some(l) = &mut self.lanzamiento {
                        if r.is_ok() {
                            // La capa sigue hasta que el registro diga que el juego cargó.
                            l.abierto = Some(Instant::now());
                            ctx.request_repaint_after(Duration::from_millis(500));
                        }
                        l.resultado = Some(r);
                    }
                }
                Msg::Hito(f, t) => {
                    if let Some(l) = &mut self.lanzamiento
                        && f > l.fraccion
                    {
                        l.fraccion = f;
                        l.hito = Some(t);
                        if f >= 1.0 && l.fin.is_none() {
                            l.fin = Some(Instant::now() + Duration::from_millis(900));
                            ctx.request_repaint_after(Duration::from_millis(900));
                        }
                    }
                }
                Msg::Mercado(n, r) if n == self.mercado_n => {
                    if let Ok(l) = &r {
                        self.mercado_cache.insert((self.mercado_cat, self.mercado_q.clone()), l.clone());
                    }
                    self.mercado = Carga::de(r);
                }
                Msg::Mercado(..) => {}
                Msg::Robux(r) => self.robux = r.ok(),
                Msg::Usuarios(n, r) if n == self.usuarios_n => self.usuarios = Carga::de(r),
                Msg::Usuarios(..) => {}
                Msg::Solicitud(id, r) => {
                    self.solicitudes.insert(id, Carga::de(r));
                }
                Msg::Conversaciones(r) => {
                    // Un fallo al refrescar no borra la lista que ya se ve.
                    if r.is_ok() || !matches!(self.conversaciones, Carga::Listo(_)) {
                        self.conversaciones = Carga::de(r);
                    }
                }
                Msg::Mensajes(id, r) => {
                    if self.chat.as_ref().is_some_and(|(c, _)| *c == id)
                        && (r.is_ok() || !matches!(self.mensajes, Carga::Listo(_)))
                    {
                        self.mensajes = Carga::de(r);
                    }
                }
                Msg::Enviado(r) => {
                    self.enviando = false;
                    match r {
                        Ok(()) => {
                            self.borrador.clear();
                            self.cargar_mensajes();
                        }
                        Err(e) => self.mensajes = Carga::Error(e),
                    }
                }
                Msg::ChatAbierto(r) => match r {
                    Ok((id, nombre)) => {
                        self.chat = Some((id, nombre));
                        self.mensajes = Carga::Cargando;
                        self.cargar_mensajes();
                        self.cargar_conversaciones();
                    }
                    Err(e) => self.mensajes = Carga::Error(e),
                },
                Msg::JuegoCerrado => {
                    // Vuelve el launcher, estuviera minimizado o detrás del juego.
                    self.minimizado = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Msg::Mando(a, conectado) => {
                    if conectado && !self.mando && self.cfg.modo == Modo::Pc && !self.preguntado_mando {
                        self.preguntado_mando = true;
                        self.dialogos.insert(0, Dialogo::Consola);
                        self.foco_dialogo = 0;
                    }
                    self.mando = conectado;
                    if let Some(a) = a {
                        self.acciones.push(a);
                    }
                }
                Msg::Sesion(r) => {
                    self.sesion_comprobada = true;
                    match r {
                        Ok(s) => {
                            self.sesion = Some(s);
                            self.login = Login::Nada;
                            self.avatar = Carga::Nada;
                            self.trajes = Carga::Nada;
                            self.inventario.clear();
                            self.amigos = Carga::Nada;
                            self.descripcion = Carga::Nada;
                            self.login_abierto = false;
                            self.dialogos.retain(|d| *d != Dialogo::Sesion);
                            if self.pestana == Pestana::Avatar {
                                self.cargar_avatar();
                                self.cargar_categoria(self.categoria);
                            }
                            if self.pestana == Pestana::Perfil {
                                self.cargar_perfil();
                            }
                            // Con sesión, Roblox añade «Seguir jugando» con tu historial.
                            self.cargar_inicio();
                            if self.precargado {
                                self.cargar_robux();
                                self.cargar_conversaciones();
                                self.cargar_perfil();
                            }
                        }
                        Err(e) if !e.is_empty() && !matches!(self.login, Login::Nada) => self.login = Login::Error(e),
                        Err(_) => self.preguntar_login(),
                    }
                }
                Msg::Codigo(Ok(c)) => {
                    // Pruebas: copiar el código en cuanto llega.
                    #[cfg(debug_assertions)]
                    if std::env::var_os("RUST_LAUNCHER_COPIAR").is_some() {
                        ctx.copy_text(c.codigo.clone());
                    }
                    self.pedir_qr(sesion::url_qr(&c));
                    self.login = Login::Esperando { codigo: c, estado: EstadoCodigo::Creado };
                }
                Msg::Codigo(Err(e)) => self.login = Login::Error(e),
                Msg::EstadoCodigo(r) => match (r, &mut self.login) {
                    (Ok(nuevo), Login::Esperando { estado, .. }) => *estado = nuevo,
                    (Err(e), _) => self.login = Login::Error(e),
                    _ => {}
                },
                Msg::Avatar(r) => self.avatar = Carga::de(r),
                Msg::Trajes(r) => self.trajes = Carga::de(r),
                Msg::Inventario(c, r) => {
                    self.inventario.insert(c, Carga::de(r));
                }
                Msg::Actualizacion(e) => self.actualizacion = e,
                Msg::Mako(e) => {
                    if matches!(e, EstadoMako::Error(_)) {
                        // Sin MAKO instalado no tiene sentido dejarlo activado.
                        self.cfg.cliente.mako = false;
                        self.cfg.guardar();
                    }
                    self.mako = e;
                }
                Msg::Amigos(r) => self.amigos = Carga::de(r),
                Msg::Descripcion(r) => self.descripcion = Carga::de(r),
                Msg::CambioAvatar(r) => {
                    self.guardando = false;
                    if let Err(e) = r {
                        self.aviso_avatar = Some(e);
                    }
                    self.tras_cambio_avatar();
                }
            }
        }
    }

    pub(super) fn pedir_qr(&mut self, url: String) {
        self.qr = Some((url.clone(), None));
        let http = self.http.clone();
        self.tarea(async move {
            let img = async {
                let b = http.get(&url).send().await.ok()?.error_for_status().ok()?.bytes().await.ok()?;
                let i = image::load_from_memory(&b).ok()?.to_rgba8();
                Some(egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw()))
            }
            .await;
            Msg::Qr(url, img)
        });
    }
}
