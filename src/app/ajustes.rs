//! Pantalla de ajustes.

use super::*;

impl App {
    // ---------- Ajustes ----------

    pub(super) fn filas_ajustes(&self) -> Vec<(&'static str, Vec<Ajuste>)> {
        let cliente = vec![
            Ajuste::Api,
            Ajuste::Msaa,
            Ajuste::Texturas,
            Ajuste::Hierba,
            Ajuste::Geometria,
            Ajuste::Cielo,
            Ajuste::Dpi,
        ];
        let mut cliente = cliente;
        cliente.push(Ajuste::LimiteFps);
        if plataforma::EFECTOS_SOBER {
            cliente.extend([Ajuste::Optimizacion, Ajuste::Escalado, Ajuste::Smaa, Ajuste::Mako]);
        }
        let mut launcher = vec![Ajuste::Modo, Ajuste::Idioma, Ajuste::EscaladorEditor, Ajuste::GenerarFotogramas, Ajuste::AlJugar, Ajuste::CerrarAlSalir, Ajuste::Precargar];
        if plataforma::ACTUALIZA_CLIENTE {
            launcher.push(Ajuste::Fondo);
        }
        launcher.extend([Ajuste::Enlaces, Ajuste::BorrarImagenes, Ajuste::ActualizarAuto, Ajuste::BuscarActualizacion]);
        let mut cuenta = Vec::new();
        cuenta.push(if self.sesion.is_some() { Ajuste::CerrarSesion } else { Ajuste::IniciarSesion });
        cuenta.extend([
            Ajuste::Web("Información de la cuenta", "Correo, teléfono, idioma y fecha de nacimiento", "https://www.roblox.com/my/account#!/info"),
            Ajuste::Web("Seguridad", "Contraseña, verificación en dos pasos y sesiones abiertas", "https://www.roblox.com/my/account#!/security"),
            Ajuste::Web("Privacidad", "Quién puede escribirte, unirse a tus partidas o ver tu inventario", "https://www.roblox.com/my/account#!/privacy"),
            Ajuste::Web("Controles parentales", "Límites de gasto y de contenido", "https://www.roblox.com/my/account#!/parental-controls"),
        ]);
        let titulo_cliente = if plataforma::APIS_GRAFICAS { "Cliente de Roblox" } else { "Cliente de Roblox (Sober)" };
        let bromas = vec![Ajuste::FingirFps];
        vec![(titulo_cliente, cliente), ("Launcher", launcher), ("Cuenta", cuenta), ("Bromas", bromas)]
    }

    pub(super) fn describir(&self, a: Ajuste) -> (String, String, String) {
        let c = &self.cfg.cliente;
        let si_no = |b: bool| if b { "Sí" } else { "No" }.to_string();
        match a {
            Ajuste::Api => (
                "Motor gráfico".into(),
                "Prueba otro si tienes tirones o cierres inesperados.".into(),
                match c.api {
                    ApiGrafica::Auto => "Automático",
                    ApiGrafica::D3D11 => "DirectX 11",
                    ApiGrafica::Vulkan => "Vulkan",
                    ApiGrafica::OpenGL => "OpenGL",
                }
                .into(),
            ),
            Ajuste::Msaa => (
                "Suavizado de bordes".into(),
                "Más muestras se ven mejor pero cuestan rendimiento.".into(),
                match c.msaa {
                    None => "Automático".into(),
                    Some(1) => "Desactivado".into(),
                    Some(n) => format!("{n}x"),
                },
            ),
            Ajuste::Texturas => (
                "Calidad de texturas".into(),
                "Fija la calidad en lugar de dejar que Roblox la ajuste.".into(),
                match c.texturas {
                    None => "Automática",
                    Some(0) => "Baja",
                    Some(1) => "Media",
                    Some(2) => "Alta",
                    _ => "Máxima",
                }
                .into(),
            ),
            Ajuste::Hierba => ("Quitar la hierba".into(), "Mejora el rendimiento en mapas con mucho terreno.".into(), si_no(c.sin_hierba)),
            Ajuste::Geometria => (
                "Reducir el detalle lejano".into(),
                "Los objetos lejanos usan modelos más simples.".into(),
                si_no(c.geometria_reducida),
            ),
            Ajuste::Cielo => ("Cielo gris".into(), "Sustituye el cielo por un color liso.".into(), si_no(c.cielo_gris)),
            Ajuste::Dpi => (
                "Ignorar el escalado de pantalla".into(),
                "Útil en monitores con escalado al 125 % o más.".into(),
                si_no(c.sin_escalado_dpi),
            ),
            Ajuste::Escalado => (
                "Escalado FSR (estilo Lossless Scaling)".into(),
                "Roblox se dibuja a menos resolución y se amplía a la de tu pantalla con FSR. Si lo desactivas podrías perder rendimiento.".into(),
                match c.escalado {
                    crate::config::Escalado::Apagado => "Desactivado",
                    crate::config::Escalado::Calidad => "Calidad (77 %)",
                    crate::config::Escalado::Equilibrado => "Equilibrado (67 %)",
                    crate::config::Escalado::Rendimiento => "Rendimiento (50 %)",
                }
                .into(),
            ),
            Ajuste::Optimizacion => (
                "Optimización gráfica de Sober".into(),
                "Ajuste propio de Sober: «Rendimiento» baja el detalle lejano y las texturas para ganar FPS.".into(),
                match c.optimizacion {
                    crate::config::OptimizacionSober::Calidad => "Calidad",
                    crate::config::OptimizacionSober::Equilibrado => "Equilibrado",
                    crate::config::OptimizacionSober::Rendimiento => "Rendimiento",
                }
                .into(),
            ),
            Ajuste::Smaa => (
                "Suavizado SMAA + nitidez".into(),
                if plataforma::vkbasalt_instalado() {
                    "Quita los dientes de sierra antes de ampliar la imagen y la afina (vkBasalt). Usa Vulkan."
                } else {
                    "Necesita la extensión vkBasalt de Flatpak (org.freedesktop.Platform.VulkanLayer.vkBasalt)."
                }
                .into(),
                si_no(c.smaa),
            ),
            Ajuste::LimiteFps => (
                "Límite de FPS".into(),
                "El límite del propio Roblox: FPS más estables y el equipo más fresco. Con MAKO, son los FPS objetivo.".into(),
                match c.limite_fps {
                    None => "Sin límite".to_string(),
                    Some(n) => trf("{} FPS", &[&n]),
                },
            ),
            Ajuste::Mako => {
                let desc = match &self.mako {
                    EstadoMako::Error(e) => trf("No se pudo instalar MAKO: {}", &[e]),
                    _ => "Genera fotogramas intermedios para ir más fluido. Experimental: añade retraso y puede dejar estelas. Usa Vulkan; la primera vez descarga unos 150 MB.".to_string(),
                };
                let valor = match &self.mako {
                    EstadoMako::Descargando(p) if *p >= 100 => "Instalando…".to_string(),
                    EstadoMako::Descargando(p) => trf("Descargando {} %", &[p]),
                    _ if c.mako => "Sí".to_string(),
                    _ => "No".to_string(),
                };
                ("MAKO: generación de fotogramas (BETA)".into(), desc, valor)
            }
            Ajuste::FingirFps => (
                "Fingir FPS (troll)".into(),
                "Al unirte a un juego muestra «6767 FPS» encima. Es solo un texto: no cambia los FPS reales ni el contador de Roblox.".into(),
                si_no(c.fingir_fps),
            ),
            Ajuste::Modo => (
                "Modo".into(),
                "Consola: pantalla completa, mando y texto grande. PC: ventana, ratón y rueda.".into(),
                match self.cfg.modo {
                    Modo::Pc => "PC",
                    Modo::Consola => "Consola",
                }
                .into(),
            ),
            Ajuste::EscaladorEditor => (
                "Escalado del editor".into(),
                "Cómo se amplía la vista de la pestaña Crear. FSR 3.1 y FSR 2 usan varios fotogramas; FSR 1, uno solo. Cambiar a uno peor podría hacerte perder rendimiento o calidad.".into(),
                match self.cfg.escalador {
                    crate::config::EscaladorEditor::Fsr1 => "FSR 1",
                    crate::config::EscaladorEditor::Fsr2 => "FSR 2",
                    crate::config::EscaladorEditor::Fsr3 => "FSR 3.1",
                }
                .into(),
            ),
            Ajuste::GenerarFotogramas => (
                "Generar fotogramas en el editor (experimental)".into(),
                "Intercala fotogramas calculados para ir más fluido. Puede notarse algo de retraso o deformación al mover la cámara.".into(),
                si_no(self.cfg.generar_fotogramas),
            ),
            Ajuste::Idioma => (
                "Idioma".into(),
                "Idioma del launcher y del editor de la pestaña Crear.".into(),
                idioma::actual().nombre().into(),
            ),
            Ajuste::CerrarAlSalir => (
                "Cerrar Roblox al salir de un juego".into(),
                "Al volver al menú de Roblox, se cierra y vuelves aquí.".into(),
                si_no(self.cfg.cerrar_al_salir),
            ),
            Ajuste::IniciarSesion => (
                "Iniciar sesión".into(),
                "Para ver tu perfil y tus amigos, y cambiar tu avatar.".into(),
                "Iniciar sesión".into(),
            ),
            Ajuste::AlJugar => (
                "Al abrir un juego".into(),
                "Minimizado, el launcher vuelve solo cuando cierras el juego.".into(),
                match self.cfg.al_jugar {
                    AlJugar::Minimizar => "Minimizar",
                    AlJugar::Cerrar => "Cerrar el launcher",
                    AlJugar::Nada => "No hacer nada",
                }
                .into(),
            ),
            Ajuste::Precargar => (
                "Precargar el cliente".into(),
                "Deja los archivos del juego en memoria para que arranque antes.".into(),
                si_no(self.cfg.precargar),
            ),
            Ajuste::Fondo => (
                "Actualizar Roblox en segundo plano".into(),
                "Así Jugar nunca espera a una descarga.".into(),
                si_no(self.cfg.actualizar_en_segundo_plano),
            ),
            Ajuste::Enlaces => (
                "Abrir los juegos de roblox.com aquí".into(),
                "Al pulsar Jugar en la web, se abre con este launcher.".into(),
                si_no(self.enlaces_web),
            ),
            Ajuste::BorrarImagenes => ("Imágenes guardadas".into(), "Miniaturas guardadas para abrir más rápido.".into(), "Borrar".into()),
            Ajuste::ActualizarAuto => (
                "Actualizar RustLauncher automáticamente".into(),
                "Solo se instalan versiones firmadas por el autor; la nueva se usa al reiniciar.".into(),
                si_no(self.cfg.actualizar_launcher),
            ),
            Ajuste::BuscarActualizacion => {
                let actual = crate::actualizar::VERSION_ACTUAL;
                let (desc, accion) = match &self.actualizacion {
                    EstadoActualizacion::Nada => (trf("Versión {}.", &[&actual]), "Buscar actualizaciones"),
                    EstadoActualizacion::Buscando => (trf("Versión {}. Buscando…", &[&actual]), "Buscando…"),
                    EstadoActualizacion::AlDia => (trf("Versión {}: es la última.", &[&actual]), "Buscar de nuevo"),
                    EstadoActualizacion::Descargando(v) => (trf("Descargando la versión {}…", &[v]), "Descargando…"),
                    EstadoActualizacion::Lista(v) => (trf("Versión {} instalada. Reinicia para usarla.", &[v]), "Reiniciar ahora"),
                    EstadoActualizacion::Error(e) => (trf("No se pudo actualizar: {}", &[e]), "Reintentar"),
                };
                ("Versión del launcher".into(), desc, accion.into())
            }
            Ajuste::CerrarSesion => (
                "Cerrar sesión".into(),
                self.sesion.as_ref().map(|s| trf("Sesión iniciada como @{}", &[&s.usuario.nombre])).unwrap_or_default(),
                "Cerrar sesión".into(),
            ),
            Ajuste::Web(t, d, _) => (t.into(), d.into(), "Abrir en el navegador".into()),
        }
    }

    /// Cambia el valor (hacia delante o hacia atrás) o ejecuta la acción.
    pub(super) fn activar(&mut self, a: Ajuste, delante: bool, ctx: &egui::Context) {
        fn ciclo<T: PartialEq + Copy>(lista: &[T], actual: T, delante: bool) -> T {
            let i = lista.iter().position(|x| *x == actual).unwrap_or(0) as i32;
            let n = lista.len() as i32;
            lista[((i + if delante { 1 } else { -1 }).rem_euclid(n)) as usize]
        }
        let mut instalar_mako = false;
        let c = &mut self.cfg.cliente;
        match a {
            Ajuste::Api => {
                let lista: &[ApiGrafica] = if plataforma::APIS_GRAFICAS {
                    &[ApiGrafica::Auto, ApiGrafica::D3D11, ApiGrafica::Vulkan, ApiGrafica::OpenGL]
                } else {
                    &[ApiGrafica::Auto, ApiGrafica::OpenGL]
                };
                c.api = ciclo(lista, c.api, delante);
                if c.api == ApiGrafica::OpenGL {
                    // vkBasalt y MAKO son capas Vulkan.
                    c.smaa = false;
                    c.mako = false;
                }
            }
            Ajuste::Smaa => {
                c.smaa = !c.smaa;
                if c.smaa && c.api == ApiGrafica::OpenGL {
                    c.api = ApiGrafica::Auto;
                }
            }
            Ajuste::LimiteFps => c.limite_fps = ciclo(&crate::config::AjustesCliente::LIMITES_FPS, c.limite_fps, delante),
            Ajuste::Optimizacion => c.optimizacion = ciclo(&crate::config::OptimizacionSober::TODOS, c.optimizacion, delante),
            Ajuste::FingirFps => c.fingir_fps = !c.fingir_fps,
            Ajuste::Mako => {
                if !matches!(self.mako, EstadoMako::Descargando(_)) {
                    c.mako = !c.mako;
                    if c.mako {
                        if c.api == ApiGrafica::OpenGL {
                            c.api = ApiGrafica::Auto;
                        }
                        instalar_mako = !plataforma::mako_instalado();
                    }
                }
            }
            Ajuste::Msaa => c.msaa = ciclo(&[None, Some(1), Some(2), Some(4), Some(8)], c.msaa, delante),
            Ajuste::Texturas => c.texturas = ciclo(&[None, Some(0), Some(1), Some(2), Some(3)], c.texturas, delante),
            Ajuste::Hierba => c.sin_hierba = !c.sin_hierba,
            Ajuste::Geometria => c.geometria_reducida = !c.geometria_reducida,
            Ajuste::Cielo => c.cielo_gris = !c.cielo_gris,
            Ajuste::Dpi => c.sin_escalado_dpi = !c.sin_escalado_dpi,
            Ajuste::Escalado => c.escalado = ciclo(&crate::config::Escalado::TODOS, c.escalado, delante),
            Ajuste::AlJugar => {
                self.cfg.al_jugar = ciclo(&[AlJugar::Minimizar, AlJugar::Cerrar, AlJugar::Nada], self.cfg.al_jugar, delante)
            }
            Ajuste::Precargar => self.cfg.precargar = !self.cfg.precargar,
            Ajuste::CerrarAlSalir => self.cfg.cerrar_al_salir = !self.cfg.cerrar_al_salir,
            Ajuste::Modo => {
                let nuevo = if self.cfg.modo == Modo::Pc { Modo::Consola } else { Modo::Pc };
                self.cambiar_modo(nuevo);
            }
            Ajuste::EscaladorEditor => {
                self.cfg.escalador = ciclo(&crate::config::EscaladorEditor::TODOS, self.cfg.escalador, delante)
            }
            Ajuste::GenerarFotogramas => self.cfg.generar_fotogramas = !self.cfg.generar_fotogramas,
            Ajuste::Idioma => {
                let nuevo = ciclo(&Idioma::TODOS, idioma::actual(), delante);
                self.cfg.idioma = Some(nuevo);
                idioma::fijar(nuevo);
            }
            Ajuste::IniciarSesion => self.abrir_login(),
            Ajuste::Fondo => self.cfg.actualizar_en_segundo_plano = !self.cfg.actualizar_en_segundo_plano,
            Ajuste::Enlaces => {
                let nuevo = !self.enlaces_web;
                match plataforma::registrar(nuevo) {
                    Ok(()) => {
                        self.enlaces_web = nuevo;
                        self.cfg.abrir_enlaces_web = Some(nuevo);
                        self.error_ajustes = None;
                        self.comprobar_plataforma();
                    }
                    Err(e) => self.error_ajustes = Some(texto_error(e)),
                }
            }
            Ajuste::BorrarImagenes => self.imgs.borrar_cache(),
            Ajuste::ActualizarAuto => self.cfg.actualizar_launcher = !self.cfg.actualizar_launcher,
            Ajuste::BuscarActualizacion => {
                if matches!(self.actualizacion, EstadoActualizacion::Lista(_)) {
                    crate::actualizar::reiniciar();
                }
                if matches!(self.actualizacion, EstadoActualizacion::AlDia | EstadoActualizacion::Error(_)) {
                    self.actualizacion = EstadoActualizacion::Nada;
                }
                self.buscar_actualizacion();
            }
            Ajuste::CerrarSesion => self.cerrar_sesion(),
            Ajuste::Web(_, _, url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        }
        if instalar_mako {
            self.instalar_mako();
        }
        self.cfg.guardar();
    }

    pub(super) fn pantalla_ajustes(&mut self, ui: &mut Ui, pantalla: Rect, acciones: &[Accion]) {
        let secciones = self.filas_ajustes();
        let planas: Vec<Ajuste> = secciones.iter().flat_map(|(_, f)| f.iter().copied()).collect();
        self.fila_ajuste = self.fila_ajuste.min(planas.len() - 1);
        // Primera fila de cada sección, para saltar entre ellas.
        let mut inicios = Vec::new();
        let mut n = 0;
        for (_, filas) in &secciones {
            inicios.push(n);
            n += filas.len();
        }
        let seccion_actual = inicios.iter().rposition(|i| *i <= self.fila_ajuste).unwrap_or(0);
        for a in acciones {
            let actual = planas[self.fila_ajuste];
            match a {
                Accion::PestanaAnterior => self.fila_ajuste = inicios[seccion_actual.saturating_sub(1)],
                Accion::PestanaSiguiente => self.fila_ajuste = inicios[(seccion_actual + 1).min(inicios.len() - 1)],
                Accion::Arriba => self.fila_ajuste = self.fila_ajuste.saturating_sub(1),
                Accion::Abajo => self.fila_ajuste = (self.fila_ajuste + 1).min(planas.len() - 1),
                Accion::Izquierda => self.activar(actual, false, ui.ctx()),
                Accion::Derecha | Accion::Aceptar => self.activar(actual, true, ui.ctx()),
                _ => {}
            }
        }

        let x0 = pantalla.left() + MARGEN;
        let ancho = (pantalla.width() - MARGEN * 2.0).min(860.0);
        let alto_fila = 68.0;
        let alto_titulo = 56.0;
        let separacion = 24.0;
        let zona = Rect::from_min_max(Pos2::new(pantalla.left(), pantalla.top() + 128.0), Pos2::new(pantalla.right(), pantalla.bottom() - 60.0));

        // Posición de la fila enfocada para que el desplazamiento la siga.
        let mut y_foco = 0.0;
        let mut acumulado = 0.0;
        let mut i = 0;
        for (_, filas) in &secciones {
            acumulado += alto_titulo;
            for _ in filas {
                if i == self.fila_ajuste {
                    y_foco = acumulado;
                }
                acumulado += alto_fila;
                i += 1;
            }
            acumulado += separacion;
        }
        let objetivo = (y_foco - zona.height() * 0.45).max(0.0);
        let off = ui.ctx().animate_value_with_time(Id::new("ajustes_v"), objetivo, 0.18);
        let p = ui.painter().with_clip_rect(zona);
        let movido = raton_movido(ui, self.cfg.modo == Modo::Consola);
        // Barra de secciones: un clic salta a su primera fila.
        let mut salto = None;
        {
            let barra = ui.painter();
            let mut x = x0;
            let y_barra = pantalla.top() + 84.0;
            for (k, (titulo, _)) in secciones.iter().enumerate() {
                let g = barra.layout_no_wrap(crate::idioma::traducir(titulo).to_string(), medio(escala::ETIQUETA), TEXTO);
                let r = Rect::from_min_size(Pos2::new(x, y_barra), Vec2::new(g.size().x + 28.0, 34.0));
                let activa = k == seccion_actual;
                let resp = ui.interact(r, Id::new(("seccion_ajustes", k)), Sense::click());
                barra.rect_filled(r, RADIO, if activa { cristal() } else { Color32::TRANSPARENT });
                if activa {
                    barra.rect_stroke(r, RADIO, Stroke::new(1.5, FOCO), StrokeKind::Inside);
                }
                barra.text(r.center(), Align2::CENTER_CENTER, crate::idioma::traducir(titulo).to_string(), medio(escala::ETIQUETA), if activa { TEXTO } else { TEXTO_2 });
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    salto = Some(inicios[k]);
                }
                x += r.width() + 8.0;
            }
        }
        if let Some(f) = salto {
            self.fila_ajuste = f;
        }
        let mut y = zona.top() + 16.0 - off;
        let mut i = 0;
        let mut activar: Option<(Ajuste, bool)> = None;
        let mut nuevo_foco = None;
        for (titulo, filas) in &secciones {
            p.text(Pos2::new(x0, y + 8.0), Align2::LEFT_TOP, *titulo, estrecha(escala::SECCION), TEXTO);
            y += alto_titulo;
            for a in filas {
                let r = Rect::from_min_size(Pos2::new(x0 - 16.0, y), Vec2::new(ancho + 32.0, alto_fila - 6.0));
                let enfocada = i == self.fila_ajuste;
                let resp = ui.interact(r.intersect(zona), Id::new(("ajuste", i)), Sense::click());
                if resp.hovered() && movido {
                    nuevo_foco = Some(i);
                }
                if enfocada {
                    p.rect_filled(r, RADIO, cristal());
                    p.rect_stroke(r.expand(2.0), RADIO + 2, Stroke::new(2.0, FOCO), StrokeKind::Inside);
                }
                let (t, d, v) = self.describir(*a);
                // La descripción no pasa de la columna del selector: se parte en
                // dos líneas como mucho y lo que sobre acaba en «…».
                let ancho_desc = (r.right() - 270.0 - x0).max(120.0);
                p.text(Pos2::new(x0, r.center().y - 17.0), Align2::LEFT_CENTER, t, semi(18.0), TEXTO);
                let mut trabajo = egui::text::LayoutJob::simple(crate::idioma::traducir(&d).to_string(), medio(escala::ETIQUETA), TEXTO_2, ancho_desc);
                trabajo.wrap.max_rows = 2;
                trabajo.wrap.overflow_character = Some('…');
                let galeria = p.layout_job(trabajo);
                p.galley(Pos2::new(x0, r.center().y - 4.0), galeria, TEXTO_2);
                let selector =
                    !matches!(
                        a,
                        Ajuste::BorrarImagenes
                            | Ajuste::CerrarSesion
                            | Ajuste::IniciarSesion
                            | Ajuste::BuscarActualizacion
                            | Ajuste::Web(..)
                    );
                let xd = r.right() - 16.0;
                if selector {
                    // Selector de consola: ‹ valor ›.
                    let ancho_v = 170.0;
                    let centro_v = xd - 26.0 - ancho_v / 2.0;
                    p.text(Pos2::new(centro_v, r.center().y), Align2::CENTER_CENTER, v, semi(17.0), TEXTO);
                    let color_flecha = if enfocada { TEXTO } else { TEXTO_2 };
                    p.text(Pos2::new(xd - 52.0 - ancho_v, r.center().y), Align2::CENTER_CENTER, "‹", semi(26.0), color_flecha);
                    p.text(Pos2::new(xd, r.center().y), Align2::RIGHT_CENTER, "›", semi(26.0), color_flecha);
                    if resp.clicked() {
                        // La mitad izquierda del selector retrocede; el resto avanza.
                        let x = ui.input(|i| i.pointer.interact_pos()).map(|p| p.x).unwrap_or(xd);
                        activar = Some((*a, x > centro_v - ancho_v / 2.0 - 10.0 || x < r.left() + ancho * 0.5));
                        if x > xd - 52.0 - ancho_v - 14.0 && x < xd - 52.0 - ancho_v + 14.0 {
                            activar = Some((*a, false));
                        }
                    }
                } else {
                    p.text(Pos2::new(xd, r.center().y), Align2::RIGHT_CENTER, v, semi(16.0), if enfocada { TEXTO } else { TEXTO_2 });
                    if resp.clicked() {
                        activar = Some((*a, true));
                    }
                }
                resp.on_hover_cursor(CursorIcon::PointingHand);
                y += alto_fila;
                i += 1;
            }
            y += separacion;
        }
        if let Some(e) = &self.error_ajustes {
            p.text(Pos2::new(x0, y), Align2::LEFT_TOP, e, medio(escala::ETIQUETA), ERROR);
        }
        let version = format!("RustLauncher {}", env!("CARGO_PKG_VERSION"));
        p.text(Pos2::new(x0, y + 28.0), Align2::LEFT_TOP, version, medio(escala::ETIQUETA), TEXTO_2);

        if let Some(f) = nuevo_foco {
            self.fila_ajuste = f;
        }
        if let Some((a, delante)) = activar {
            self.fila_ajuste = planas.iter().position(|x| *x == a).unwrap_or(self.fila_ajuste);
            self.activar(a, delante, ui.ctx());
        }
    }
}
