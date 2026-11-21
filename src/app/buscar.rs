//! Búsqueda de experiencias.

use super::*;

impl App {
    // ---------- Buscar ----------

    pub(super) fn pantalla_buscar(&mut self, ui: &mut Ui, pantalla: Rect, acciones: &[Accion]) {
        let x0 = pantalla.left() + MARGEN;
        let campo = Rect::from_min_size(
            Pos2::new(x0, pantalla.top() + 96.0),
            Vec2::new((pantalla.width() - MARGEN * 2.0).min(720.0), 60.0),
        );
        let consola_en_campo = self.foco_busqueda.is_none();
        let c = self.campo(ui, campo, Destino::Juegos, "Busca una experiencia", consola_en_campo);
        if c.enviado {
            self.buscar();
        }
        let r = c.resp;
        if self.cfg.modo == Modo::Consola && consola_en_campo && acciones.contains(&Accion::Aceptar) {
            self.abrir_teclado(Destino::Juegos);
            return;
        }

        let y = campo.bottom() + 40.0;
        let p = ui.painter().clone();
        let aviso = match &self.busqueda {
            Carga::Nada => Some(("Escribe el nombre de una experiencia y pulsa Enter.".to_string(), TEXTO_2)),
            Carga::Cargando => Some((format!("Buscando «{}»", self.buscada), TEXTO_2)),
            Carga::Error(e) => Some((format!("La búsqueda falló. {e}"), ERROR)),
            Carga::Listo(l) if l.is_empty() => Some((
                format!("No hay experiencias que coincidan con «{}». Prueba con otras palabras.", self.buscada),
                TEXTO_2,
            )),
            Carga::Listo(_) => None,
        };
        if let Some((t, c)) = aviso {
            p.text(Pos2::new(x0, y), Align2::LEFT_TOP, t, cuerpo(escala::CUERPO), c);
            return;
        }
        let Carga::Listo(lista) = &self.busqueda else { return };
        let lista = lista.clone();

        let lado = 150.0;
        let paso = Vec2::new(lado + 20.0, lado + 64.0);
        let columnas = ((pantalla.width() - MARGEN * 2.0 + 20.0) / paso.x).floor().max(1.0) as usize;
        let escribiendo = c.foco;
        let mut foco_i = self.foco_busqueda;
        for a in acciones {
            match (a, foco_i) {
                (Accion::Abajo, None) => {
                    foco_i = Some(0);
                    if let Some(r) = &r {
                        r.surrender_focus();
                    }
                }
                (Accion::Abajo, Some(i)) => foco_i = Some((i + columnas).min(lista.len() - 1)),
                (Accion::Arriba, Some(i)) if i < columnas => {
                    // Con la rueda (modo PC) se queda en la primera fila.
                    if self.cfg.modo == Modo::Consola {
                        foco_i = None;
                    }
                }
                (Accion::Arriba, Some(i)) => foco_i = Some(i - columnas),
                (Accion::Izquierda, Some(i)) => foco_i = Some(i.saturating_sub(1)),
                (Accion::Derecha, Some(i)) => foco_i = Some((i + 1).min(lista.len() - 1)),
                _ => {}
            }
        }
        if escribiendo && !acciones.contains(&Accion::Abajo) {
            foco_i = None;
        }

        let zona = Rect::from_min_max(Pos2::new(pantalla.left(), y - 8.0), Pos2::new(pantalla.right(), pantalla.bottom() - 60.0));
        let fila_foco = foco_i.map(|i| i / columnas).unwrap_or(0) as f32;
        let filas_visibles = (zona.height() / paso.y).floor().max(1.0);
        let objetivo = (fila_foco - filas_visibles + 1.0).max(0.0) * paso.y;
        let off = ui.ctx().animate_value_with_time(Id::unique("busqueda_v"), objetivo, 0.18);
        let pc = p.with_clip_rect(zona);
        let movido = raton_movido(ui, self.cfg.modo == Modo::Consola);
        let mut abrir = None;
        for (i, j) in lista.iter().enumerate() {
            let (f, c) = (i / columnas, i % columnas);
            let r = Rect::from_min_size(Pos2::new(x0 + c as f32 * paso.x, y + f as f32 * paso.y - off), Vec2::splat(lado));
            if r.bottom() + 50.0 < zona.top() || r.top() > zona.bottom() {
                continue;
            }
            let tex = self.img(TipoImg::Icono, j.universe_id);
            ficha_img(&pc, tex.as_ref(), r, Color32::WHITE);
            let enfocada = foco_i == Some(i);
            if enfocada {
                foco(&pc, r);
            }
            let g = galley(ui, &j.nombre, medio(escala::ETIQUETA), if enfocada { TEXTO } else { TEXTO_2 }, lado, 2);
            pc.galley(Pos2::new(r.left(), r.bottom() + 10.0), g, TEXTO);
            let resp = ui.interact(r.intersect(zona), Id::unique(("busq", i)), Sense::click());
            if !enfocada && resp.hovered() {
                resalte(&pc, r);
            }
            if resp.hovered() && movido {
                foco_i = Some(i);
            }
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                abrir = Some(j.universe_id);
            }
        }
        self.foco_busqueda = foco_i;
        if let Some(i) = foco_i
            && let Some(j) = lista.get(i)
        {
            self.fondo_a(j.universe_id);
            for a in acciones {
                match a {
                    Accion::Aceptar if !escribiendo => {
                        self.jugar(j.clone());
                        return;
                    }
                    Accion::Info => abrir = Some(j.universe_id),
                    Accion::Favorito => self.cfg.alternar_favorito(j.clone()),
                    _ => {}
                }
            }
        }
        if let Some(id) = abrir {
            self.abrir_info(id);
        }
    }
}
