//! Teclado en pantalla para el modo consola.
//!
//! Se maneja con el mando (cruceta para moverse, A para escribir, B para
//! borrar, Y para espacio, X para mayúsculas) o con el ratón. Si hay un teclado
//! físico, también se puede escribir con él: Enter acepta y Esc cancela.

use crate::mando::Accion;
use crate::tema::*;
use eframe::egui::{self, Align2, Color32, CursorIcon, Event, Id, Key, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

/// Para qué campo se está escribiendo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destino {
    Juegos,
    Mercado,
    Usuarios,
    Chat,
}

pub enum Resultado {
    Aceptado(Destino, String),
    Cancelado,
}

const FILAS: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjklñ", "zxcvbnm.,-"];
const ESPECIALES: [&str; 4] = ["Mayús", "Espacio", "Borrar", "Aceptar"];
const MAX: usize = 200;

pub struct Teclado {
    pub destino: Destino,
    pub texto: String,
    titulo: &'static str,
    fila: usize,
    col: usize,
    mayus: bool,
}

impl Teclado {
    pub fn nuevo(destino: Destino, texto: String) -> Self {
        let titulo = match destino {
            Destino::Juegos => "Buscar experiencias",
            Destino::Mercado => "Buscar en el mercado",
            Destino::Usuarios => "Buscar personas",
            Destino::Chat => "Escribe un mensaje",
        };
        Self { destino, texto, titulo, fila: 1, col: 0, mayus: false }
    }

    fn largo_fila(fila: usize) -> usize {
        if fila < FILAS.len() { FILAS[fila].chars().count() } else { ESPECIALES.len() }
    }

    fn escribir(&mut self, c: char) {
        if self.texto.chars().count() < MAX {
            self.texto.push(if self.mayus { c.to_uppercase().next().unwrap_or(c) } else { c });
            self.mayus = false;
        }
    }

    /// Pulsa la tecla enfocada. Devuelve un resultado si se aceptó.
    fn pulsar(&mut self, fila: usize, col: usize) -> Option<Resultado> {
        if fila < FILAS.len() {
            if let Some(c) = FILAS[fila].chars().nth(col) {
                self.escribir(c);
            }
            return None;
        }
        match col {
            0 => self.mayus = !self.mayus,
            1 => self.escribir(' '),
            2 => {
                self.texto.pop();
            }
            _ => return Some(Resultado::Aceptado(self.destino, self.texto.trim().to_string())),
        }
        None
    }

    pub fn mostrar(&mut self, ctx: &egui::Context, pantalla: Rect, acciones: &[Accion], mando: bool, fondo: Color32) -> Option<Resultado> {
        let mut resultado = None;

        // Teclado físico: texto, borrar, Enter y Esc. Esas dos teclas también
        // llegan como acciones; aquí se distinguen del mando.
        let (enter, esc) = ctx.input(|i| {
            for e in &i.events {
                if let Event::Text(t) = e {
                    for c in t.chars() {
                        if !c.is_control() && self.texto.chars().count() < MAX {
                            self.texto.push(c);
                        }
                    }
                }
            }
            if i.key_pressed(Key::Backspace) {
                self.texto.pop();
            }
            (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape))
        });
        if enter {
            return Some(Resultado::Aceptado(self.destino, self.texto.trim().to_string()));
        }
        if esc {
            return Some(Resultado::Cancelado);
        }

        for a in acciones {
            match a {
                Accion::Arriba => {
                    if self.fila > 0 {
                        self.fila -= 1;
                        self.col = self.col.min(Self::largo_fila(self.fila) - 1);
                    }
                }
                Accion::Abajo => {
                    if self.fila < FILAS.len() {
                        self.fila += 1;
                        // La fila especial tiene menos teclas y más anchas.
                        self.col = if self.fila == FILAS.len() { (self.col * ESPECIALES.len()) / 10 } else { self.col };
                        self.col = self.col.min(Self::largo_fila(self.fila) - 1);
                    }
                }
                Accion::Izquierda => self.col = self.col.saturating_sub(1),
                Accion::Derecha => self.col = (self.col + 1).min(Self::largo_fila(self.fila) - 1),
                Accion::Aceptar => resultado = resultado.or(self.pulsar(self.fila, self.col)),
                // B borra; Y espacio; X mayúsculas (como en las consolas).
                Accion::Volver => {
                    if self.texto.pop().is_none() {
                        resultado = Some(Resultado::Cancelado);
                    }
                }
                Accion::Info => self.escribir(' '),
                Accion::Favorito => self.mayus = !self.mayus,
                _ => {}
            }
        }

        egui::Area::new(Id::new("teclado")).order(Order::Foreground).fixed_pos(pantalla.min).show(ctx, |ui| {
            ui.allocate_rect(pantalla, Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(pantalla, 0, Color32::from_black_alpha(150));
            let lado = 62.0;
            let hueco = 8.0;
            let ancho_teclas = 10.0 * lado + 9.0 * hueco;
            let alto_panel = 96.0 + 5.0 * (lado + hueco) + 72.0;
            let panel = Rect::from_min_max(Pos2::new(pantalla.left(), pantalla.bottom() - alto_panel), pantalla.max);
            p.rect_filled(panel, 0, fondo);
            let x0 = panel.center().x - ancho_teclas / 2.0;

            // Campo con el texto escrito.
            let campo = Rect::from_min_size(Pos2::new(x0, panel.top() + 24.0), Vec2::new(ancho_teclas, 56.0));
            p.text(campo.left_top() - Vec2::new(0.0, 6.0), Align2::LEFT_BOTTOM, self.titulo, medio(escala::ETIQUETA), TEXTO_2);
            p.rect_filled(campo, RADIO, cristal());
            p.rect_stroke(campo.expand(2.0), RADIO + 2, Stroke::new(2.0, FOCO), StrokeKind::Inside);
            let g = p.layout_no_wrap(self.texto.clone(), medio(22.0), TEXTO);
            // Si el texto no cabe se muestra su final.
            let sobra = (g.size().x - (campo.width() - 40.0)).max(0.0);
            let pt = Pos2::new(campo.left() + 20.0 - sobra, campo.center().y - g.size().y / 2.0);
            let ancho_texto = g.size().x;
            p.with_clip_rect(campo.shrink(4.0)).galley(pt, g, TEXTO);
            let parpadeo = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
            if parpadeo {
                let xc = pt.x + ancho_texto + 2.0;
                p.line_segment([Pos2::new(xc, campo.top() + 14.0), Pos2::new(xc, campo.bottom() - 14.0)], Stroke::new(2.0, TEXTO));
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));

            let mut y = campo.bottom() + 20.0;
            // Se recorre por índice: la última fila (teclas especiales) no está en FILAS.
            #[allow(clippy::needless_range_loop)]
            for fila in 0..=FILAS.len() {
                let n = Self::largo_fila(fila);
                let ancho_tecla = if fila < FILAS.len() { lado } else { (ancho_teclas - 3.0 * hueco) / 4.0 };
                for col in 0..n {
                    let r = Rect::from_min_size(Pos2::new(x0 + col as f32 * (ancho_tecla + hueco), y), Vec2::new(ancho_tecla, lado));
                    let resp = ui.interact(r, Id::new(("tecla", fila, col)), Sense::click());
                    if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                        self.fila = fila;
                        self.col = col;
                    }
                    let enfocada = self.fila == fila && self.col == col;
                    let activa_mayus = fila == FILAS.len() && col == 0 && self.mayus;
                    let fondo_t = if enfocada {
                        FOCO
                    } else if activa_mayus {
                        cristal_fuerte()
                    } else {
                        cristal()
                    };
                    p.rect_filled(r, RADIO, fondo_t);
                    let etiqueta = if fila < FILAS.len() {
                        let c = FILAS[fila].chars().nth(col).unwrap_or(' ');
                        if self.mayus { c.to_uppercase().to_string() } else { c.to_string() }
                    } else {
                        ESPECIALES[col].to_string()
                    };
                    let fuente = if fila < FILAS.len() { semi(22.0) } else { semi(17.0) };
                    p.text(r.center(), Align2::CENTER_CENTER, etiqueta, fuente, if enfocada { TINTA } else { TEXTO });
                    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        self.fila = fila;
                        self.col = col;
                        resultado = resultado.take().or(self.pulsar(fila, col));
                    }
                }
                y += lado + hueco;
            }

            // Pistas de botones.
            let mut x = x0;
            let yp = panel.bottom() - 30.0;
            let pistas: &[(&str, &str, &str)] = &[
                ("A", "Enter", "Escribir"),
                ("B", "Borrar", "Borrar"),
                ("Y", "Espacio", "Espacio"),
                ("X", "Mayús", "Mayúsculas"),
            ];
            for (bm, bt, t) in pistas {
                let b = if mando { *bm } else { *bt };
                x += pista(&p, Pos2::new(x, yp), b, t, mando) + 28.0;
            }
        });
        resultado
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escribe_borra_y_acepta() {
        let mut t = Teclado::nuevo(Destino::Juegos, String::new());
        t.mayus = true;
        assert!(t.pulsar(1, 0).is_none()); // Q mayúscula
        assert!(t.pulsar(1, 1).is_none()); // w
        assert!(t.pulsar(4, 1).is_none()); // espacio
        assert!(t.pulsar(4, 2).is_none()); // borrar
        match t.pulsar(4, 3) {
            Some(Resultado::Aceptado(Destino::Juegos, s)) => assert_eq!(s, "Qw"),
            _ => panic!("debía aceptar"),
        }
    }
}
