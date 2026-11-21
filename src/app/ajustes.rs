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
        let mut launcher = vec![Ajuste::Modo, Ajuste::AlJugar, Ajuste::CerrarAlSalir, Ajuste::Precargar];
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
        vec![(titulo_cliente, cliente), ("Launcher", launcher), ("Cuenta", cuenta)]
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
            Ajuste::Modo => (
                "Modo".into(),
                "Consola: pantalla completa, mando y texto grande. PC: ventana, ratón y rueda.".into(),
                match self.cfg.modo {
                    Modo::Pc => "PC",
                    Modo::Consola => "Consola",
                }
                .into(),
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
                    EstadoActualizacion::Nada => (format!("Versión {actual}."), "Buscar actualizaciones"),
                    EstadoActualizacion::Buscando => (format!("Versión {actual}. Buscando…"), "Buscando…"),
                    EstadoActualizacion::AlDia => (format!("Versión {actual}: es la última."), "Buscar de nuevo"),
                    EstadoActualizacion::Descargando(v) => (format!("Descargando la versión {v}…"), "Descargando…"),
                    EstadoActualizacion::Lista(v) => (format!("Versión {v} instalada. Reinicia para usarla."), "Reiniciar ahora"),
                    EstadoActualizacion::Error(e) => (format!("No se pudo actualizar: {e}"), "Reintentar"),
                };
                ("Versión del launcher".into(), desc, accion.into())
            }
            Ajuste::CerrarSesion => (
                "Cerrar sesión".into(),
                self.sesion.as_ref().map(|s| format!("Sesión iniciada como @{}", s.usuario.nombre)).unwrap_or_default(),
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
        let c = &mut self.cfg.cliente;
        match a {
            Ajuste::Api => {
                let lista: &[ApiGrafica] = if plataforma::APIS_GRAFICAS {
                    &[ApiGrafica::Auto, ApiGrafica::D3D11, ApiGrafica::Vulkan, ApiGrafica::OpenGL]
                } else {
                    &[ApiGrafica::Auto, ApiGrafica::OpenGL]
                };
                c.api = ciclo(lista, c.api, delante);
            }
            Ajuste::Msaa => c.msaa = ciclo(&[None, Some(1), Some(2), Some(4), Some(8)], c.msaa, delante),
            Ajuste::Texturas => c.texturas = ciclo(&[None, Some(0), Some(1), Some(2), Some(3)], c.texturas, delante),
            Ajuste::Hierba => c.sin_hierba = !c.sin_hierba,
            Ajuste::Geometria => c.geometria_reducida = !c.geometria_reducida,
            Ajuste::Cielo => c.cielo_gris = !c.cielo_gris,
            Ajuste::Dpi => c.sin_escalado_dpi = !c.sin_escalado_dpi,
            Ajuste::AlJugar => {
                self.cfg.al_jugar = ciclo(&[AlJugar::Minimizar, AlJugar::Cerrar, AlJugar::Nada], self.cfg.al_jugar, delante)
            }
            Ajuste::Precargar => self.cfg.precargar = !self.cfg.precargar,
            Ajuste::CerrarAlSalir => self.cfg.cerrar_al_salir = !self.cfg.cerrar_al_salir,
            Ajuste::Modo => {
                let nuevo = if self.cfg.modo == Modo::Pc { Modo::Consola } else { Modo::Pc };
                self.cambiar_modo(nuevo);
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
        self.cfg.guardar();
    }

    pub(super) fn pantalla_ajustes(&mut self, ui: &mut Ui, pantalla: Rect, acciones: &[Accion]) {
        let secciones = self.filas_ajustes();
        let planas: Vec<Ajuste> = secciones.iter().flat_map(|(_, f)| f.iter().copied()).collect();
        self.fila_ajuste = self.fila_ajuste.min(planas.len() - 1);
        for a in acciones {
            let actual = planas[self.fila_ajuste];
            match a {
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
        let zona = Rect::from_min_max(Pos2::new(pantalla.left(), pantalla.top() + 84.0), Pos2::new(pantalla.right(), pantalla.bottom() - 60.0));

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
        let off = ui.ctx().animate_value_with_time(Id::unique("ajustes_v"), objetivo, 0.18);
        let p = ui.painter().with_clip_rect(zona);
        let movido = raton_movido(ui, self.cfg.modo == Modo::Consola);
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
                let resp = ui.interact(r.intersect(zona), Id::unique(("ajuste", i)), Sense::click());
                if resp.hovered() && movido {
                    nuevo_foco = Some(i);
                }
                if enfocada {
                    p.rect_filled(r, RADIO, cristal());
                    p.rect_stroke(r.expand(2.0), RADIO + 2, Stroke::new(2.0, FOCO), StrokeKind::Inside);
                }
                let (t, d, v) = self.describir(*a);
                p.text(Pos2::new(x0, r.center().y - 11.0), Align2::LEFT_CENTER, t, semi(18.0), TEXTO);
                p.text(Pos2::new(x0, r.center().y + 12.0), Align2::LEFT_CENTER, d, medio(escala::ETIQUETA), TEXTO_2);
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
