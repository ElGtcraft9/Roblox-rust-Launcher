//! Campos de texto y teclado en pantalla.

use super::*;

impl App {
    pub(super) fn texto_de(&mut self, d: Destino) -> &mut String {
        match d {
            Destino::Juegos => &mut self.consulta,
            Destino::Mercado => &mut self.mercado_q,
            Destino::Usuarios => &mut self.usuarios_q,
            Destino::Chat => &mut self.borrador,
        }
    }

    pub(super) fn abrir_teclado(&mut self, d: Destino) {
        let texto = self.texto_de(d).clone();
        self.teclado = Some(Teclado::nuevo(d, texto));
    }

    pub(super) fn al_teclado(&mut self, r: teclado::Resultado) {
        let teclado::Resultado::Aceptado(d, texto) = r else { return };
        *self.texto_de(d) = texto;
        match d {
            Destino::Juegos => self.buscar(),
            Destino::Mercado => self.cargar_mercado(),
            Destino::Usuarios => self.buscar_usuarios(),
            Destino::Chat => self.enviar_borrador(),
        }
    }

    /// Campo de texto: en modo PC se escribe en él; en modo consola, al
    /// elegirlo se abre el teclado en pantalla.
    pub(super) fn campo(&mut self, ui: &mut Ui, r: Rect, d: Destino, pista_txt: &str, enfocado_consola: bool) -> CampoRes {
        ui.painter().rect_filled(r, RADIO, cristal());
        let tam_letra = if r.height() > 54.0 { 22.0 } else { 17.0 };
        if self.cfg.modo == Modo::Consola {
            let texto = self.texto_de(d).clone();
            let resp = ui.interact(r, Id::unique(("campo", format!("{d:?}"))), Sense::click());
            let (t, color) = if texto.is_empty() { (pista_txt.to_string(), TEXTO_2) } else { (texto, TEXTO) };
            let g = galley(ui, &t, medio(tam_letra), color, r.width() - 40.0, 1);
            ui.painter().galley(Pos2::new(r.left() + 20.0, r.center().y - g.size().y / 2.0), g, color);
            if enfocado_consola || resp.hovered() {
                ui.painter().rect_stroke(r.expand(3.0), RADIO + 2, Stroke::new(3.0, FOCO), StrokeKind::Inside);
            }
            if resp.on_hover_cursor(CursorIcon::Text).clicked() {
                self.abrir_teclado(d);
            }
            return CampoRes { enviado: false, foco: false, resp: None };
        }
        let pedir = self.pedir_foco_texto && d == Destino::Juegos;
        let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(Vec2::new(20.0, 0.0))));
        let texto = self.texto_de(d);
        let resp = hijo.add_sized(
            [r.width() - 40.0, r.height()],
            TextEdit::singleline(texto)
                .hint_text(egui::RichText::new(pista_txt).color(TEXTO_2))
                .font(medio(tam_letra))
                .text_color(TEXTO)
                .frame(Frame::NONE)
                .margin(Margin::ZERO)
                .align(Align2::LEFT_CENTER),
        );
        if pedir {
            resp.request_focus();
            self.pedir_foco_texto = false;
        }
        if resp.has_focus() {
            ui.painter().rect_stroke(r.expand(3.0), RADIO + 2, Stroke::new(3.0, FOCO), StrokeKind::Inside);
        }
        let enviado = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        CampoRes { enviado, foco: resp.has_focus(), resp: Some(resp) }
    }
}
