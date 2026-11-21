//! Pantalla de perfil (o «Guest» sin sesión).

use super::*;

impl App {
    // ---------- Perfil ----------

    pub(super) fn pantalla_perfil(&mut self, ui: &mut Ui, pantalla: Rect) {
        let p = ui.painter().clone();
        let x0 = pantalla.left() + MARGEN;
        let lado = (pantalla.height() - 240.0).clamp(240.0, 470.0);
        let centro = Pos2::new(x0 + lado / 2.0, pantalla.top() + 92.0 + lado / 2.0);
        suelo(&p, centro, lado);
        let sesion = self.sesion.clone();
        match &sesion {
            Some(s) => {
                if let Some(t) = self.img(TipoImg::Avatar, s.usuario.id) {
                    cubrir(&p, &t, Rect::from_center_size(centro, Vec2::splat(lado)), 1.0);
                }
            }
            None => dibujar_invitado(&p, Pos2::new(centro.x, centro.y + lado * 0.42), lado * 0.78),
        }

        let xa = x0 + lado + 56.0;
        let ancho = (pantalla.right() - MARGEN - xa).max(240.0);
        let mut y = pantalla.top() + 104.0;
        let (nombre, usuario) = match &sesion {
            Some(s) => (s.usuario.nombre_visible.clone(), format!("@{}", s.usuario.nombre)),
            None => ("Guest".to_string(), "Jugando como invitado".to_string()),
        };
        let g = galley(ui, &nombre, estrecha_fuerte(escala::DISPLAY), TEXTO, ancho, 1);
        let h = g.size().y;
        p.galley(Pos2::new(xa, y), g, TEXTO);
        y += h;
        p.text(Pos2::new(xa, y), Align2::LEFT_TOP, usuario, medio(escala::CUERPO), TEXTO_2);
        y += 40.0;

        if sesion.is_none() {
            let g = galley(
                ui,
                "Inicia sesión para ver tu avatar, tu descripción y a tus amigos.",
                cuerpo(escala::CUERPO),
                TEXTO,
                ancho,
                3,
            );
            let h = g.size().y;
            p.galley(Pos2::new(xa, y), g, TEXTO);
            if boton(ui, Pos2::new(xa, y + h + 24.0), "Iniciar sesión", EstiloBoton::Principal, "perfil_login").0.clicked() {
                self.abrir_login();
            }
            return;
        }

        // Descripción.
        let (texto, color) = match &self.descripcion {
            Carga::Listo(d) if d.is_empty() => ("Sin descripción.".to_string(), TEXTO_2),
            Carga::Listo(d) => (d.clone(), TEXTO.gamma_multiply(0.88)),
            Carga::Error(e) => (e.clone(), ERROR),
            _ => (String::new(), TEXTO_2),
        };
        let fuente = cuerpo(escala::CUERPO);
        let texto: String = texto.lines().map(|l| limpiar(ui, l, &fuente)).collect::<Vec<_>>().join("\n");
        let g = galley(ui, &texto, fuente, color, ancho.min(680.0), 4);
        let h = g.size().y;
        p.galley(Pos2::new(xa, y), g, color);
        y += h + 16.0;
        if boton(ui, Pos2::new(xa, y), "Cambiar avatar", EstiloBoton::Secundario, "perfil_avatar").0.clicked() {
            self.cambiar_pestana(Pestana::Avatar);
        }
        y += 52.0 + 32.0;

        // Amigos.
        let titulo = match &self.amigos {
            Carga::Listo(l) => format!("Amigos ({})", l.len()),
            _ => "Amigos".to_string(),
        };
        p.text(Pos2::new(xa, y), Align2::LEFT_TOP, titulo, estrecha(escala::SECCION), TEXTO);
        y += 44.0;
        let amigos = match &self.amigos {
            Carga::Listo(l) if l.is_empty() => {
                p.text(Pos2::new(xa, y), Align2::LEFT_TOP, "Aún no tienes amigos en Roblox.", cuerpo(escala::CUERPO), TEXTO_2);
                return;
            }
            Carga::Listo(l) => l.clone(),
            Carga::Error(e) => {
                let g = galley(ui, &format!("No se pudo cargar la lista. {e}"), cuerpo(escala::CUERPO), ERROR, ancho, 3);
                p.galley(Pos2::new(xa, y), g, ERROR);
                return;
            }
            _ => {
                p.text(Pos2::new(xa, y), Align2::LEFT_TOP, "Cargando amigos…", cuerpo(escala::CUERPO), TEXTO_2);
                return;
            }
        };

        let paso = Vec2::new(124.0, 156.0);
        let columnas = ((ancho + 12.0) / paso.x).floor().max(1.0) as usize;
        let zona = Rect::from_min_max(Pos2::new(xa - 8.0, y - 8.0), Pos2::new(pantalla.right(), pantalla.bottom() - 60.0));
        let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(zona));
        let mut abrir = None;
        egui::ScrollArea::vertical().id_salt("amigos").show(&mut hijo, |ui| {
            let filas = amigos.len().div_ceil(columnas);
            let (area, _) = ui.allocate_exact_size(Vec2::new(zona.width() - 12.0, filas as f32 * paso.y + 8.0), Sense::hover());
            for (i, a) in amigos.iter().enumerate() {
                let celda = Rect::from_min_size(
                    area.min + Vec2::new(8.0 + (i % columnas) as f32 * paso.x, 8.0 + (i / columnas) as f32 * paso.y),
                    Vec2::new(112.0, 148.0),
                );
                if !ui.is_rect_visible(celda) {
                    continue;
                }
                let c = Pos2::new(celda.center().x, celda.top() + 44.0);
                let resp = ui.interact(celda, Id::unique(("amigo", a.id)), Sense::click());
                let pp = ui.painter();
                pp.circle_filled(c, 42.0, if resp.hovered() { cristal_fuerte() } else { cristal() });
                if let Some(t) = self.imgs.get((TipoImg::Cabeza, a.id)) {
                    egui::Image::from_texture(egui::load::SizedTexture::from_handle(&t))
                        .corner_radius(40)
                        .paint_at(ui, Rect::from_center_size(c, Vec2::splat(80.0)));
                }
                let conectado = a.presencia != Presencia::Desconectado;
                if conectado {
                    ui.painter().circle_stroke(c, 45.0, Stroke::new(3.0, EN_LINEA));
                }
                let nombre = if a.nombre_visible.is_empty() { &a.nombre } else { &a.nombre_visible };
                let g = galley(ui, nombre, semi(15.0), TEXTO, celda.width(), 1);
                ui.painter().galley(Pos2::new(celda.center().x - g.size().x / 2.0, celda.top() + 96.0), g, TEXTO);
                let estado = match &a.presencia {
                    Presencia::Jugando(Some(j)) => j.clone(),
                    Presencia::Jugando(None) => "Jugando".to_string(),
                    Presencia::EnLinea => "En línea".to_string(),
                    Presencia::EnStudio => "En Studio".to_string(),
                    Presencia::Desconectado => "Desconectado".to_string(),
                };
                let g = galley(ui, &estado, medio(13.0), if conectado { EN_LINEA } else { TEXTO_2 }, celda.width(), 1);
                ui.painter().galley(Pos2::new(celda.center().x - g.size().x / 2.0, celda.top() + 118.0), g, TEXTO_2);
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    abrir = Some(a.id);
                }
            }
        });
        if let Some(id) = abrir {
            ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://www.roblox.com/users/{id}/profile")));
        }
    }
}
