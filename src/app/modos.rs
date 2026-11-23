//! Modos PC y consola, transiciones, precarga de pestañas y ayudas de sesión.

use super::*;

impl App {
    /// Escala la interfaz al tamaño de la ventana y pone o quita la pantalla
    /// completa según el modo.
    pub(super) fn aplicar_modo(&mut self, ctx: &egui::Context) {
        let z = ctx.zoom_factor();
        let r = ctx.content_rect();
        // Tamaño sin zoom: no cambia al aplicar el zoom, así no oscila.
        let (w, h) = (r.width() * z, r.height() * z);
        let objetivo = match self.cfg.modo {
            Modo::Consola => (h / 720.0).min(w / 1280.0).clamp(1.0, 1.8),
            Modo::Pc => (h / 760.0).min(w / 1280.0).clamp(0.8, 1.25),
        };
        if (objetivo - z).abs() > 0.02 {
            ctx.set_zoom_factor(objetivo);
        }
        if self.modo_aplicado != Some(self.cfg.modo) {
            let primera = self.modo_aplicado.is_none();
            self.modo_aplicado = Some(self.cfg.modo);
            let completa = self.cfg.modo == Modo::Consola;
            // En las capturas de prueba no se ocupa la pantalla.
            let captura = cfg!(debug_assertions) && std::env::var_os("RUST_LAUNCHER_CAPTURA").is_some();
            if !captura && (completa || !primera) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(completa));
            }
        }
    }

    pub(super) fn cambiar_modo(&mut self, modo: Modo) {
        self.cfg.modo = modo;
        self.cfg.guardar();
        if modo == Modo::Consola {
            self.animacion_consola = Some(Instant::now());
        }
    }

    /// Pide por adelantado lo que necesitan las demás pestañas, para que al
    /// entrar ya esté cargado. Se hace una vez, poco después de arrancar, y
    /// otra vez al iniciar sesión.
    pub(super) fn precargar_pestanas(&mut self, ctx: &egui::Context) {
        if self.precargado || self.cerrar_tras_lanzar {
            return;
        }
        let espera = Duration::from_millis(2500);
        if self.arranque.elapsed() < espera {
            ctx.request_repaint_after(espera - self.arranque.elapsed());
            return;
        }
        self.precargado = true;
        if matches!(self.mercado, Carga::Nada) {
            self.cargar_mercado();
        }
        if self.sesion.is_some() {
            self.cargar_robux();
            self.cargar_conversaciones();
            self.cargar_perfil();
        }
    }

    /// Busca versiones nuevas del launcher unos segundos después de abrirlo y
    /// después cada 6 horas. En compilaciones de desarrollo no lo hace (salvo
    /// con RUST_LAUNCHER_ACTUALIZAR), para no reemplazar el ejecutable de pruebas.
    pub(super) fn actualizar_solo(&mut self, ctx: &egui::Context) {
        let permitido = !cfg!(debug_assertions) || std::env::var_os("RUST_LAUNCHER_ACTUALIZAR").is_some();
        if !permitido || !self.cfg.actualizar_launcher || self.cerrar_tras_lanzar || crate::perfil::activo() {
            return;
        }
        let espera = match self.ultima_busqueda {
            None => Duration::from_secs(4).saturating_sub(self.arranque.elapsed()),
            Some(t) => Duration::from_secs(6 * 3600).saturating_sub(t.elapsed()),
        };
        if espera.is_zero() {
            self.buscar_actualizacion();
        } else {
            ctx.request_repaint_after(espera);
        }
    }

    pub(super) fn buscar_actualizacion(&mut self) {
        if matches!(
            self.actualizacion,
            EstadoActualizacion::Buscando | EstadoActualizacion::Descargando(_) | EstadoActualizacion::Lista(_)
        ) {
            return;
        }
        self.ultima_busqueda = Some(Instant::now());
        self.actualizacion = EstadoActualizacion::Buscando;
        let (http, tx) = (self.http.clone(), self.tx.clone());
        self.rt.spawn(async move {
            let estado = match crate::actualizar::buscar(&http).await {
                Ok(None) => EstadoActualizacion::AlDia,
                Ok(Some(v)) => {
                    tx.enviar(Msg::Actualizacion(EstadoActualizacion::Descargando(v.numero.clone())));
                    match crate::actualizar::instalar(&http, &v).await {
                        Ok(()) => EstadoActualizacion::Lista(v.numero),
                        Err(e) => EstadoActualizacion::Error(texto_error(e)),
                    }
                }
                Err(e) => EstadoActualizacion::Error(texto_error(e)),
            };
            tx.enviar(Msg::Actualizacion(estado));
        });
    }

    /// Descarga e instala MAKO en segundo plano.
    pub(super) fn instalar_mako(&mut self) {
        if matches!(self.mako, EstadoMako::Descargando(_)) {
            return;
        }
        self.mako = EstadoMako::Descargando(0);
        let (http, tx) = (self.http.clone(), self.tx.clone());
        self.rt.spawn(async move {
            let ultimo = std::sync::atomic::AtomicU8::new(0);
            let r = crate::plataforma::instalar_mako(&http, |f| {
                let pct = (f * 100.0) as u8;
                if ultimo.swap(pct, std::sync::atomic::Ordering::Relaxed) != pct {
                    tx.enviar(Msg::Mako(EstadoMako::Descargando(pct)));
                }
            })
            .await;
            tx.enviar(Msg::Mako(match r {
                Ok(()) => EstadoMako::Nada,
                Err(e) => EstadoMako::Error(texto_error(e)),
            }));
        });
    }

    /// Progreso (0 a 1, con desaceleración) de la transición entre vistas.
    /// Detecta solo los cambios de pestaña y la apertura de «Más información».
    pub(super) fn transicion(&mut self) -> f32 {
        let vista = (self.pestana, self.info.is_some());
        if vista != self.vista_anterior {
            let indice = |p: Pestana| PESTANAS.iter().position(|(x, _)| *x == p).unwrap_or(0) as f32;
            self.dir_vista = if vista.0 != self.vista_anterior.0 {
                (indice(vista.0) - indice(self.vista_anterior.0)).signum()
            } else if vista.1 {
                1.0
            } else {
                -1.0
            };
            self.vista_anterior = vista;
            self.cambio_vista = Instant::now();
        }
        let t = (self.cambio_vista.elapsed().as_secs_f32() / 0.26).min(1.0);
        1.0 - (1.0 - t).powi(3)
    }

    pub(super) fn hay_capa(&self) -> bool {
        self.animacion_consola.is_some()
            || self.login_abierto
            || !self.dialogos.is_empty()
            || self.teclado.is_some()
            || self.articulo.is_some()
    }

    pub(super) fn preguntar_login(&mut self) {
        if self.preguntado_login || self.sesion.is_some() || self.cerrar_tras_lanzar {
            return;
        }
        self.preguntado_login = true;
        self.dialogos.push(Dialogo::Sesion);
    }

    pub(super) fn abrir_login(&mut self) {
        self.login_abierto = true;
        self.copiado = None;
        if !matches!(self.login, Login::Esperando { .. } | Login::Creando) {
            self.iniciar_login();
        }
    }

    pub(super) fn cerrar_login(&mut self) {
        self.login_abierto = false;
        self.login_cancelar.store(true, Ordering::Relaxed);
        self.login = Login::Nada;
        self.qr = None;
    }

    pub(super) fn cargar_perfil(&mut self) {
        let Some(s) = self.sesion.clone() else { return };
        if matches!(self.amigos, Carga::Nada | Carga::Error(_)) {
            self.amigos = Carga::Cargando;
            let s2 = s.clone();
            self.tarea(async move { Msg::Amigos(s2.amigos().await.map_err(texto_error)) });
        }
        if matches!(self.descripcion, Carga::Nada | Carga::Error(_)) {
            self.descripcion = Carga::Cargando;
            self.tarea(async move { Msg::Descripcion(s.descripcion().await.map_err(texto_error)) });
        }
    }

    /// Pantalla para quien no ha iniciado sesión: explica y ofrece hacerlo.
    pub(super) fn invitar_login(&mut self, ui: &mut Ui, pantalla: Rect, titulo: &str) {
        let p = ui.painter().clone();
        let x0 = pantalla.left() + MARGEN;
        let ancho = (pantalla.width() - MARGEN * 2.0).min(620.0);
        let mut y = pantalla.top() + 120.0;
        let g = galley(ui, titulo, estrecha_fuerte(escala::TITULO), TEXTO, ancho, 2);
        let h = g.size().y;
        p.galley(Pos2::new(x0, y), g, TEXTO);
        y += h + 12.0;
        let g = galley(
            ui,
            "Se inicia con un código desde tu móvil o el navegador. El launcher nunca ve tu contraseña.",
            cuerpo(escala::CUERPO),
            TEXTO_2,
            ancho,
            3,
        );
        let h = g.size().y;
        p.galley(Pos2::new(x0, y), g, TEXTO_2);
        y += h + 28.0;
        if boton(ui, Pos2::new(x0, y), "Iniciar sesión", EstiloBoton::Principal, "invitar").0.clicked() {
            self.abrir_login();
        }
    }
}
