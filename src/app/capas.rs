//! Capas por encima de todo: lanzamiento, inicio de sesión, diálogos y animación.

use super::*;

impl App {
    // ---------- Lanzamiento ----------

    pub(super) fn capa_lanzamiento(&mut self, ui: &mut Ui, pantalla: Rect, acciones: &[Accion]) {
        let p = ui.painter().clone();
        p.rect_filled(pantalla, 0, self.tinte_actual().gamma_multiply(0.5));
        let Some(l) = &self.lanzamiento else { return };
        let x0 = pantalla.left() + MARGEN;
        let ancho = (pantalla.width() * 0.5).min(640.0);
        let y = pantalla.bottom() - 200.0;
        let cliente = plataforma::NOMBRE_CLIENTE;
        // La barra avanza por fases reales: preparar el cliente (hasta 20 %) y
        // después los hitos del registro de Roblox (hasta 100 %).
        let (estado, objetivo, error) = match (&l.resultado, l.progreso) {
            (Some(Err(e)), _) => (e.clone(), 0.0, true),
            (Some(Ok(())), _) => (
                l.hito.map(str::to_string).unwrap_or_else(|| trf("Abriendo {}", &[&cliente])),
                l.fraccion.max(0.22),
                false,
            ),
            (None, Progreso::Comprobando) => ("Comprobando la versión de Roblox".into(), 0.04, false),
            (None, Progreso::Descargando { hechos, total }) => (
                trf("Descargando la nueva versión de Roblox: {} de {} paquetes", &[&hechos, &total]),
                0.04 + 0.14 * hechos as f32 / total.max(1) as f32,
                false,
            ),
            (None, Progreso::Iniciando) => (trf("Iniciando {}", &[&cliente]), 0.2, false),
            (None, Progreso::CerrandoAnterior) => ("Cerrando la partida anterior".into(), 0.1, false),
        };
        p.text(Pos2::new(x0, y - 80.0), Align2::LEFT_BOTTOM, "Abriendo", medio(escala::CUERPO), TEXTO_2);
        let g = galley(ui, &l.titulo, estrecha_fuerte(escala::DISPLAY), TEXTO, ancho, 1);
        p.galley(Pos2::new(x0, y - 76.0), g, TEXTO);
        let g = galley(ui, &estado, medio(escala::CUERPO), if error { ERROR } else { TEXTO }, ancho, 3);
        let h = g.size().y;
        p.galley(Pos2::new(x0, y + 8.0), g, TEXTO);
        if error {
            let (r, _) = boton(ui, Pos2::new(x0, y + h + 28.0), "Cerrar", EstiloBoton::Secundario, "cerrar_error");
            if r.clicked() || acciones.contains(&Accion::Volver) || acciones.contains(&Accion::Aceptar) {
                self.lanzamiento = None;
                if self.cerrar_tras_lanzar {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            return;
        }
        let barra = Rect::from_min_size(Pos2::new(x0, y + h + 28.0), Vec2::new(ancho.min(420.0), 4.0));
        let f = ui.ctx().animate_value_with_time(Id::new("barra_carga"), objetivo, 0.45);
        p.rect_filled(barra, 2, cristal());
        p.rect_filled(Rect::from_min_size(barra.min, Vec2::new(barra.width() * f, 4.0)), 2, FOCO);
        p.text(
            Pos2::new(barra.right() + 14.0, barra.center().y),
            Align2::LEFT_CENTER,
            format!("{:.0} %", f * 100.0),
            medio(escala::ETIQUETA),
            TEXTO_2,
        );
    }

    // ---------- Capas ----------

    /// Inicio de sesión con código (como en las consolas).
    pub(super) fn capa_login(&mut self, ctx: &egui::Context, pantalla: Rect, acciones: &[Accion]) {
        let mut cerrar = acciones.contains(&Accion::Volver);
        let mut otro = false;
        let mut copiar: Option<String> = None;
        let fondo = self.tinte_actual();
        egui::Area::new(Id::new("capa_login")).order(Order::Foreground).fixed_pos(pantalla.min).show(ctx, |ui| {
            ui.allocate_rect(pantalla, Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(pantalla, 0, fondo);
            degradado_v(&p, pantalla, mezclar(fondo, Color32::WHITE, 0.06), fondo);

            let x0 = pantalla.left() + MARGEN;
            let ancho = (pantalla.width() * 0.42).min(520.0);
            let mut y = pantalla.top() + 96.0;
            let g = galley(ui, "Inicia sesión en Roblox", estrecha_fuerte(escala::TITULO), TEXTO, ancho + 200.0, 2);
            let h = g.size().y;
            p.galley(Pos2::new(x0, y), g, TEXTO);
            y += h + 28.0;

            // Los pasos son una secuencia real: por eso van numerados.
            let pasos = [
                "En tu móvil u otro equipo con la sesión iniciada, abre roblox.com/crossdevicelogin o escanea el código QR.",
                "Escribe el código que ves aquí. Puedes copiarlo con el botón.",
                "Confirma el inicio de sesión en ese dispositivo.",
            ];
            for (i, t) in pasos.iter().enumerate() {
                p.text(Pos2::new(x0, y), Align2::LEFT_TOP, format!("{}", i + 1), estrecha_fuerte(escala::SECCION), TEXTO_2);
                let g = galley(ui, t, cuerpo(escala::CUERPO), TEXTO, ancho, 3);
                let h = g.size().y;
                p.galley(Pos2::new(x0 + 32.0, y + 4.0), g, TEXTO);
                y += h.max(24.0) + 20.0;
            }
            y += 8.0;

            let (estado, color, reintentar) = match &self.login {
                Login::Nada | Login::Creando => ("Generando código…".to_string(), TEXTO_2, false),
                Login::Error(e) => (e.clone(), ERROR, true),
                Login::Esperando { estado, .. } => match estado {
                    EstadoCodigo::Creado => ("Esperando a que escribas el código…".to_string(), TEXTO_2, false),
                    EstadoCodigo::Vinculado(n) => (
                        trf(
                            "Código recibido de {}. Confírmalo en ese dispositivo.",
                            &[&n.clone().unwrap_or_else(|| tr("tu cuenta").into())],
                        ),
                        TEXTO,
                        false,
                    ),
                    EstadoCodigo::Validado => ("Iniciando sesión…".to_string(), TEXTO, false),
                    EstadoCodigo::Cancelado => ("El código caducó o se canceló.".to_string(), ERROR, true),
                },
            };
            let g = galley(ui, &estado, medio(escala::CUERPO), color, ancho, 3);
            let h = g.size().y;
            p.galley(Pos2::new(x0, y), g, color);
            y += h + 24.0;
            let mut x = x0;
            if reintentar {
                let (r, w) = boton(ui, Pos2::new(x, y), "Generar otro código", EstiloBoton::Secundario, "otro");
                otro = r.clicked();
                x += w + 12.0;
            }
            cerrar |= boton(ui, Pos2::new(x, y), "Volver", EstiloBoton::Secundario, "login_volver").0.clicked();

            let g = galley(
                ui,
                "El launcher nunca ve tu contraseña. La sesión se guarda en el almacén de credenciales del sistema y puedes cerrarla en Ajustes.",
                medio(escala::ETIQUETA),
                TEXTO_2,
                ancho,
                3,
            );
            p.galley(Pos2::new(x0, pantalla.bottom() - 72.0 - g.size().y), g, TEXTO_2);

            // Código grande, botón de copiar y QR a la derecha.
            if let Login::Esperando { codigo, .. } = &self.login {
                let xc = (x0 + ancho + 80.0).min(pantalla.right() - MARGEN - 400.0).max(x0 + ancho + 24.0);
                let yc = pantalla.top() + 104.0;
                let mut x = xc;
                for ch in codigo.codigo.chars() {
                    let r = Rect::from_min_size(Pos2::new(x, yc), Vec2::new(60.0, 80.0));
                    p.rect_filled(r, RADIO, cristal());
                    p.text(r.center(), Align2::CENTER_CENTER, ch, estrecha_fuerte(50.0), TEXTO);
                    x += 68.0;
                }
                let copiado = self.copiado.is_some_and(|t| t.elapsed() < Duration::from_secs(2));
                let texto = if copiado { "Copiado" } else { "Copiar código" };
                let estilo = if copiado { EstiloBoton::Secundario } else { EstiloBoton::Principal };
                if boton(ui, Pos2::new(xc, yc + 96.0), texto, estilo, "copiar").0.clicked() {
                    copiar = Some(codigo.codigo.clone());
                }
                if let Some((_, Some(qr))) = &self.qr {
                    let r = Rect::from_min_size(Pos2::new(xc + 10.0, yc + 96.0 + 52.0 + 34.0), Vec2::splat(180.0));
                    p.rect_filled(r.expand(10.0), RADIO, FOCO);
                    p.image(qr.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
            }
        });
        if let Some(c) = copiar {
            ctx.copy_text(c);
            self.copiado = Some(Instant::now());
            ctx.request_repaint_after(Duration::from_secs(2));
        }
        if otro {
            self.iniciar_login();
        } else if cerrar {
            self.cerrar_login();
        }
    }

    pub(super) fn capa_dialogo(&mut self, ctx: &egui::Context, pantalla: Rect, acciones: &[Accion]) {
        let Some(d) = self.dialogos.first().copied() else { return };
        let (titulo, texto, si, no) = match d {
            Dialogo::Consola => (
                "¿Cambiar a modo consola?",
                "Hemos detectado un mando. El modo consola usa la pantalla completa y texto grande, y se maneja entero con el mando.",
                "Sí",
                "No",
            ),
            Dialogo::Sesion => (
                "¿Deseas iniciar sesión para una mejor experiencia?",
                "Verás los juegos a los que has jugado, tu perfil y tus amigos, y podrás cambiar tu avatar.",
                "Sí",
                "No, gracias",
            ),
        };
        let mut foco_d = self.foco_dialogo.min(1);
        let mut elegido: Option<bool> = None;
        for a in acciones {
            match a {
                Accion::Izquierda => foco_d = 0,
                Accion::Derecha => foco_d = 1,
                Accion::Aceptar => elegido = Some(foco_d == 0),
                Accion::Volver => elegido = Some(false),
                _ => {}
            }
        }
        let fondo = mezclar(self.tinte_actual(), Color32::WHITE, 0.07);
        egui::Area::new(Id::new("dialogo")).order(Order::Foreground).fixed_pos(pantalla.min).show(ctx, |ui| {
            ui.allocate_rect(pantalla, Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(pantalla, 0, Color32::from_black_alpha(170));
            let ancho = (pantalla.width() - 64.0).min(620.0);
            let interior = ancho - 96.0;
            let g_t = galley(ui, titulo, estrecha_fuerte(escala::TITULO), TEXTO, interior, 3);
            let g_x = galley(ui, texto, cuerpo(escala::CUERPO), TEXTO_2, interior, 4);
            let icono = if d == Dialogo::Consola { 96.0 } else { 0.0 };
            let alto = 48.0 + icono + g_t.size().y + 14.0 + g_x.size().y + 32.0 + 52.0 + 48.0;
            let caja = Rect::from_center_size(pantalla.center(), Vec2::new(ancho, alto));
            p.rect_filled(caja, RADIO + 4, fondo);
            let x = caja.left() + 48.0;
            let mut y = caja.top() + 48.0;
            if d == Dialogo::Consola {
                dibujar_mando(&p, Pos2::new(x + 70.0, y + 40.0), 140.0, 10.0, 1.0);
                y += icono;
            }
            let h = g_t.size().y;
            p.galley(Pos2::new(x, y), g_t, TEXTO);
            y += h + 14.0;
            let h = g_x.size().y;
            p.galley(Pos2::new(x, y), g_x, TEXTO_2);
            y += h + 32.0;
            let estilo = |i: usize| if foco_d == i { EstiloBoton::Principal } else { EstiloBoton::Secundario };
            let (r_si, w) = boton(ui, Pos2::new(x, y), si, estilo(0), "dlg_si");
            let (r_no, _) = boton(ui, Pos2::new(x + w + 12.0, y), no, estilo(1), "dlg_no");
            if r_si.hovered() {
                foco_d = 0;
            }
            if r_no.hovered() {
                foco_d = 1;
            }
            if r_si.clicked() {
                elegido = Some(true);
            }
            if r_no.clicked() {
                elegido = Some(false);
            }
        });
        self.foco_dialogo = foco_d;
        if let Some(si) = elegido {
            self.dialogos.remove(0);
            self.foco_dialogo = 0;
            match (d, si) {
                (Dialogo::Consola, true) => self.cambiar_modo(Modo::Consola),
                (Dialogo::Sesion, true) => self.abrir_login(),
                _ => {}
            }
        }
    }

    /// «Modo consola activado»: el mando aparece, se encienden sus botones y
    /// se desvanece.
    pub(super) fn capa_animacion(&mut self, ctx: &egui::Context, pantalla: Rect) {
        const DURACION: f32 = 2.8;
        let Some(t0) = self.animacion_consola else { return };
        let t = t0.elapsed().as_secs_f32();
        if t > DURACION {
            self.animacion_consola = None;
            return;
        }
        ctx.request_repaint();
        let alfa = (t / 0.3).min(1.0).min(((DURACION - t) / 0.45).clamp(0.0, 1.0));
        let fondo = self.tinte_actual();
        egui::Area::new(Id::new("animacion")).order(Order::Tooltip).fixed_pos(pantalla.min).show(ctx, |ui| {
            ui.allocate_rect(pantalla, Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(pantalla, 0, fondo.gamma_multiply(alfa));
            let k = ease_out_back(((t - 0.05) / 0.55).clamp(0.0, 1.0));
            let ancho = (pantalla.width() * 0.32).clamp(240.0, 480.0) * (0.6 + 0.4 * k);
            let centro = pantalla.center() - Vec2::new(0.0, pantalla.height() * 0.1) + Vec2::new(0.0, (1.0 - k) * 30.0);
            dibujar_mando(&p, centro, ancho, t, alfa);
            let ta = ((t - 0.9) / 0.35).clamp(0.0, 1.0) * alfa;
            let y = centro.y + ancho * 0.36 + 28.0 - (1.0 - ta) * 12.0;
            p.text(
                Pos2::new(pantalla.center().x, y),
                Align2::CENTER_TOP,
                "Modo consola activado",
                estrecha_fuerte(escala::DISPLAY),
                TEXTO.gamma_multiply(ta),
            );
            p.text(
                Pos2::new(pantalla.center().x, y + 72.0),
                Align2::CENTER_TOP,
                "Muévete con el mando. Puedes volver al modo PC en Ajustes.",
                medio(escala::CUERPO),
                TEXTO_2.gamma_multiply(ta),
            );
        });
    }
}
