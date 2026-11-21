//! Editor de avatar: trajes e inventario.

use super::*;

impl App {
    // ---------- Avatar ----------

    pub(super) fn pantalla_avatar(&mut self, ui: &mut Ui, pantalla: Rect) {
        let Some(s) = self.sesion.clone() else {
            return self.invitar_login(ui, pantalla, "Inicia sesión para cambiar tu avatar");
        };
        if matches!(self.avatar, Carga::Nada) {
            self.cargar_avatar();
            self.cargar_categoria(self.categoria);
        }
        let p = ui.painter().clone();
        let x0 = pantalla.left() + MARGEN;

        // --- Escenario ---
        let lado = (pantalla.height() - 300.0).clamp(280.0, 470.0);
        let centro = Pos2::new(x0 + lado / 2.0, pantalla.top() + 96.0 + lado / 2.0);
        // Suelo: elipses concéntricas que imitan un foco de luz.
        for i in 0..12 {
            let k = 1.0 - i as f32 / 12.0;
            let r = Rect::from_center_size(Pos2::new(centro.x, centro.y + lado * 0.42), Vec2::new(lado * 0.95 * k, lado * 0.16 * k));
            p.rect_filled(r, (r.height() / 2.0) as u8, Color32::from_white_alpha(4));
        }
        if let Some(t) = self.img(TipoImg::Avatar, s.usuario.id) {
            cubrir(&p, &t, Rect::from_center_size(centro, Vec2::splat(lado)), 1.0);
        }
        let mut y = centro.y + lado / 2.0 + 8.0;
        let g = galley(ui, &s.usuario.nombre_visible, estrecha_fuerte(escala::TITULO), TEXTO, lado, 1);
        let h = g.size().y;
        p.galley(Pos2::new(x0, y), g, TEXTO);
        y += h;
        p.text(Pos2::new(x0, y), Align2::LEFT_TOP, format!("@{}", s.usuario.nombre), medio(escala::CUERPO), TEXTO_2);
        y += 36.0;

        // Tipo de cuerpo: selector de dos posiciones.
        let listo = matches!(self.avatar, Carga::Listo(_));
        let r15 = matches!(&self.avatar, Carga::Listo(a) if a.r15);
        let mut x = x0;
        for (etiqueta, valor) in [("R15", true), ("R6", false)] {
            let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(64.0, 40.0));
            let activo = listo && r15 == valor;
            let resp = ui.interact(r, Id::unique(("cuerpo", etiqueta)), Sense::click());
            let fondo = if activo { FOCO } else if resp.hovered() { cristal_fuerte() } else { cristal() };
            p.rect_filled(r, RADIO, fondo);
            p.text(r.center(), Align2::CENTER_CENTER, etiqueta, semi(16.0), if activo { TINTA } else { TEXTO });
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && listo && !activo && !self.guardando {
                self.cambiar_cuerpo(valor);
            }
            x += 72.0;
        }
        let g = p.layout_no_wrap("Editor completo en Roblox".into(), medio(escala::ETIQUETA), TEXTO_2);
        let rr = Rect::from_min_size(Pos2::new(x + 8.0, y + 20.0 - g.size().y / 2.0), g.size());
        let resp = ui.interact(rr.expand(6.0), Id::unique("editor_web"), Sense::click());
        p.galley(rr.min, g, if resp.hovered() { TEXTO } else { TEXTO_2 });
        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://www.roblox.com/my/avatar"));
        }

        // --- Armario ---
        let xa = x0 + lado + 56.0;
        let ancho = pantalla.right() - MARGEN - xa;
        let mut xt = xa;
        let yt = pantalla.top() + 104.0;
        for cat in Categoria::TODAS {
            let activa = self.categoria == cat;
            let g = p.layout_no_wrap(cat.nombre().to_string(), semi(17.0), TEXTO);
            let tam = g.size();
            let r = Rect::from_min_size(Pos2::new(xt, yt), tam).expand2(Vec2::new(6.0, 8.0));
            let resp = ui.interact(r, Id::unique(("cat", cat.nombre())), Sense::click());
            p.galley(Pos2::new(xt, yt), g, if activa || resp.hovered() { TEXTO } else { TEXTO_2 });
            if activa {
                p.rect_filled(Rect::from_min_size(Pos2::new(xt, yt + tam.y + 6.0), Vec2::new(tam.x, 3.0)), 1, FOCO);
            }
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.categoria = cat;
                self.cargar_categoria(cat);
            }
            xt += tam.x + 28.0;
        }
        let mut yg = yt + 52.0;
        if let Some(a) = &self.aviso_avatar {
            p.text(Pos2::new(xa, yg), Align2::LEFT_TOP, a, medio(escala::ETIQUETA), ERROR);
            yg += 28.0;
        } else if self.guardando {
            p.text(Pos2::new(xa, yg), Align2::LEFT_TOP, "Guardando el avatar…", medio(escala::ETIQUETA), TEXTO_2);
            yg += 28.0;
        }

        // Fichas: trajes o prendas del inventario.
        let puestos: Vec<u64> = match &self.avatar {
            Carga::Listo(a) => a.puestos.iter().map(|p| p.id).collect(),
            _ => Vec::new(),
        };
        enum Elem {
            Traje(Traje),
            Obj(Objeto),
        }
        let (elems, estado): (Vec<Elem>, Option<String>) = if self.categoria == Categoria::Trajes {
            match &self.trajes {
                Carga::Listo(l) if l.is_empty() => (vec![], Some("No tienes trajes guardados. Créalos en el editor de Roblox.".into())),
                Carga::Listo(l) => (l.iter().cloned().map(Elem::Traje).collect(), None),
                Carga::Error(e) => (vec![], Some(e.clone())),
                _ => (vec![], Some("Cargando tus trajes".into())),
            }
        } else {
            match self.inventario.get(&self.categoria) {
                Some(Carga::Listo(l)) if l.is_empty() => (vec![], Some("No tienes objetos de esta categoría.".into())),
                Some(Carga::Listo(l)) => (l.iter().cloned().map(Elem::Obj).collect(), None),
                Some(Carga::Error(e)) => (vec![], Some(e.clone())),
                _ => (vec![], Some("Cargando tu inventario".into())),
            }
        };
        if let Some(t) = estado {
            let g = galley(ui, &t, cuerpo(escala::CUERPO), TEXTO_2, ancho, 3);
            p.galley(Pos2::new(xa, yg), g, TEXTO_2);
            return;
        }

        let lado_f = 112.0;
        let paso = Vec2::new(lado_f + 16.0, lado_f + 40.0);
        let columnas = ((ancho + 16.0) / paso.x).floor().max(1.0) as usize;
        let zona = Rect::from_min_max(Pos2::new(xa - 6.0, yg - 6.0), Pos2::new(pantalla.right(), pantalla.bottom() - 60.0));
        let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(zona));
        let mut elegido: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt(("armario", self.categoria.nombre())).show(&mut hijo, |ui| {
            let filas = elems.len().div_ceil(columnas);
            let (area, _) = ui.allocate_exact_size(Vec2::new(zona.width() - 12.0, filas as f32 * paso.y + 6.0), Sense::hover());
            for (i, e) in elems.iter().enumerate() {
                let r = Rect::from_min_size(
                    area.min + Vec2::new(6.0 + (i % columnas) as f32 * paso.x, 6.0 + (i / columnas) as f32 * paso.y),
                    Vec2::splat(lado_f),
                );
                if !ui.is_rect_visible(r) {
                    continue;
                }
                let (tex, nombre, puesto) = match e {
                    Elem::Traje(t) => (self.imgs.get((TipoImg::Traje, t.id)), t.nombre.clone(), false),
                    Elem::Obj(o) => (self.imgs.get((TipoImg::Prenda, o.id)), o.nombre.clone(), puestos.contains(&o.id)),
                };
                let resp = ui.interact(r, Id::unique(("armario_i", i)), Sense::click());
                let fondo = if resp.hovered() { cristal_fuerte() } else { cristal() };
                ui.painter().rect_filled(r, RADIO, fondo);
                if let Some(t) = &tex {
                    egui::Image::from_texture(egui::load::SizedTexture::from_handle(t)).paint_at(ui, r.shrink(8.0));
                }
                if puesto {
                    foco(ui.painter(), r);
                    let c = r.right_top() + Vec2::new(-14.0, 14.0);
                    ui.painter().circle_filled(c, 10.0, FOCO);
                    ui.painter().line_segment([c + Vec2::new(-4.5, 0.0), c + Vec2::new(-1.0, 3.5)], Stroke::new(2.2, TINTA));
                    ui.painter().line_segment([c + Vec2::new(-1.0, 3.5), c + Vec2::new(5.0, -3.5)], Stroke::new(2.2, TINTA));
                }
                let g = galley(ui, &nombre, medio(13.0), if puesto { TEXTO } else { TEXTO_2 }, lado_f, 1);
                ui.painter().galley(Pos2::new(r.left(), r.bottom() + 8.0), g, TEXTO);
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    elegido = Some(i);
                }
            }
        });
        if let Some(i) = elegido
            && !self.guardando
        {
            match &elems[i] {
                Elem::Traje(t) => self.poner_traje(t),
                Elem::Obj(o) => self.alternar_objeto(o),
            }
        }
    }
}
