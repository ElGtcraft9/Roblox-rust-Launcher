//! Fondo con el arte del juego y barras superior e inferior.

use super::*;

impl App {
    // ---------- Fondo ----------

    pub(super) fn pintar_fondo(&mut self, ui: &mut Ui, pantalla: Rect) {
        let p = ui.painter().clone();
        let base = self.tinte_actual();
        p.rect_filled(pantalla, 0, base);

        let con_arte = self.lanzamiento.is_some() || self.info.is_some() || self.pestana == Pestana::Inicio;
        if !con_arte {
            degradado_v(&p, pantalla, mezclar(base, Color32::WHITE, 0.05), base);
            return;
        }
        let t = self.progreso_fundido();
        if t < 1.0
            && let Some(prev) = self.fondo_previo
            && let Some(tex) = self.img(TipoImg::Banner, prev)
        {
            cubrir(&p, &tex, pantalla, 1.0 - t);
        }
        if let Some(id) = self.fondo
            && let Some(tex) = self.img(TipoImg::Banner, id)
        {
            cubrir(&p, &tex, pantalla, t);
        }
        // Velos: izquierda para el texto, abajo para los estantes, arriba para las pestañas.
        let w = pantalla.width();
        degradado_h(
            &p,
            Rect::from_min_size(pantalla.min, Vec2::new(w * 0.72, pantalla.height())),
            base.gamma_multiply(0.94),
            base.gamma_multiply(0.0),
        );
        degradado_v(
            &p,
            Rect::from_min_size(pantalla.min, Vec2::new(w, 120.0)),
            base.gamma_multiply(0.7),
            base.gamma_multiply(0.0),
        );
        let abajo = Rect::from_min_max(Pos2::new(pantalla.left(), pantalla.top() + pantalla.height() * 0.42), pantalla.max);
        degradado_v(&p, abajo, base.gamma_multiply(0.0), base);
    }

    // ---------- Barras ----------

    pub(super) fn barra_superior(&mut self, ui: &mut Ui, pantalla: Rect) {
        let p = ui.painter().clone();
        let y = pantalla.top() + 40.0;
        let mut subrayado: Option<(f32, f32)> = None;
        let mut x = pantalla.left() + MARGEN;
        for (pest, nombre) in PESTANAS {
            let activa = self.pestana == pest && self.info.is_none();
            let g = p.layout_no_wrap(nombre.to_string(), semi(escala::PESTANA), TEXTO);
            let ancho = g.size().x;
            let r = Rect::from_min_size(Pos2::new(x, y - g.size().y / 2.0), g.size()).expand2(Vec2::new(8.0, 10.0));
            let resp = ui.interact(r, Id::unique(("pestana", nombre)), Sense::click());
            let color = if activa || resp.hovered() { TEXTO } else { TEXTO_2 };
            p.text(Pos2::new(x, y), Align2::LEFT_CENTER, nombre, semi(escala::PESTANA), color);
            if activa {
                subrayado = Some((x, ancho));
            }
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.cambiar_pestana(pest);
            }
            x += ancho + 36.0;
        }

        if let Some((xs, ws)) = subrayado {
            let ctx = ui.ctx();
            let xa = ctx.animate_value_with_time(Id::unique("subrayado_x"), xs, 0.22);
            let wa = ctx.animate_value_with_time(Id::unique("subrayado_w"), ws, 0.22);
            p.rect_filled(Rect::from_min_size(Pos2::new(xa, y + 16.0), Vec2::new(wa, 3.0)), 1, FOCO);
        }

        // Derecha: reloj y perfil.
        let mut xd = pantalla.right() - MARGEN;
        if let Some(s) = self.sesion.clone() {
            let c = Pos2::new(xd - 18.0, y);
            let r = Rect::from_center_size(c, Vec2::splat(36.0));
            p.circle_filled(c, 18.0, cristal());
            if let Some(tex) = self.img(TipoImg::Cabeza, s.usuario.id) {
                egui::Image::from_texture(egui::load::SizedTexture::from_handle(&tex))
                    .corner_radius(18)
                    .paint_at(ui, r);
            }
            let resp = ui.interact(r, Id::unique("perfil"), Sense::click());
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.cambiar_pestana(Pestana::Avatar);
            }
            xd -= 36.0 + 20.0;
        }
        let hora = chrono::Local::now().format("%H:%M").to_string();
        p.text(Pos2::new(xd, y), Align2::RIGHT_CENTER, hora, medio(escala::PESTANA), TEXTO);
    }

    pub(super) fn barra_inferior(&mut self, ui: &mut Ui, pantalla: Rect) {
        let p = ui.painter().clone();
        let y = pantalla.bottom() - 30.0;

        // Izquierda: estado del cliente, solo si hay algo que decir.
        let estado = if let Some(a) = &self.aviso_cliente {
            Some((a.clone(), ERROR))
        } else if let Some(Progreso::Descargando { hechos, total }) = self.fondo_progreso {
            Some((format!("Actualizando {}: {hechos} de {total}", plataforma::NOMBRE_CLIENTE), TEXTO_2))
        } else {
            self.fondo_error
                .as_ref()
                .map(|_| ("No se pudo comprobar si hay actualizaciones".to_string(), ERROR))
        };
        let x_aviso = pantalla.left() + MARGEN;
        if let EstadoActualizacion::Lista(v) = &self.actualizacion {
            let g = p.layout_no_wrap(format!("RustLauncher {v} instalado."), medio(escala::ETIQUETA), TEXTO);
            let w = g.size().x;
            p.galley(Pos2::new(x_aviso, y - g.size().y / 2.0), g, TEXTO);
            let (r, _) = boton(ui, Pos2::new(x_aviso + w + 12.0, y - 18.0), "Reiniciar ahora", EstiloBoton::Principal, "reiniciar");
            if r.clicked() {
                crate::actualizar::reiniciar();
            }
        } else if let EstadoActualizacion::Descargando(v) = &self.actualizacion {
            p.text(Pos2::new(x_aviso, y), Align2::LEFT_CENTER, format!("Descargando RustLauncher {v}…"), medio(escala::ETIQUETA), TEXTO_2);
        } else if let Some((t, c)) = estado {
            p.text(Pos2::new(x_aviso, y), Align2::LEFT_CENTER, t, medio(escala::ETIQUETA), c);
        }

        // Derecha: pistas de botones, solo en modo consola.
        if self.cfg.modo != Modo::Consola {
            return;
        }
        let m = self.mando;
        let mut pistas: Vec<(&str, &str, &str)> = if self.info.is_some() {
            vec![("A", "Enter", "Jugar"), ("X", "F", "Favorito"), ("B", "Esc", "Volver")]
        } else {
            match self.pestana {
                Pestana::Inicio | Pestana::Buscar => {
                    vec![("A", "Enter", "Jugar"), ("Y", "I", "Información"), ("X", "F", "Favorito")]
                }
                Pestana::Ajustes => vec![("A", "Enter", "Cambiar")],
                Pestana::Mercado => vec![("A", "Enter", "Abrir")],
                Pestana::Social => vec![("A", "Enter", "Elegir"), ("Y", "I", "Mensaje")],
                Pestana::Avatar | Pestana::Perfil => vec![],
            }
        };
        pistas.push(("LB RB", "Q E", "Pestañas"));
        let elegir = |(bm, bt, _): &(&str, &str, &str)| if m { bm.to_string() } else { bt.to_string() };
        let medidas: Vec<f32> = pistas
            .iter()
            .map(|t| {
                let b = elegir(t);
                let gt = p.layout_no_wrap(t.2.to_string(), medio(escala::ETIQUETA), TEXTO).size().x;
                let ancho_b = if m && b.len() == 1 {
                    22.0
                } else {
                    (p.layout_no_wrap(b, semi(13.0), TEXTO).size().x + 14.0).max(24.0)
                };
                ancho_b + 8.0 + gt
            })
            .collect();
        let total: f32 = medidas.iter().sum::<f32>() + 28.0 * medidas.len().saturating_sub(1) as f32;
        let mut x = pantalla.right() - MARGEN - total;
        for (i, t) in pistas.iter().enumerate() {
            let b = elegir(t);
            let circulo = m && b.len() == 1;
            pista(&p, Pos2::new(x, y), &b, t.2, circulo);
            x += medidas[i] + 28.0;
        }
    }
}
