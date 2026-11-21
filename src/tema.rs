//! Tokens visuales (ver DESIGN.md): consola de salón.
//!
//! No hay color de acento fijo: la interfaz se tiñe con el color del juego
//! enfocado. El blanco es el color del foco y de la acción principal.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect,
    Stroke, StrokeKind, TextStyle, Vec2, epaint::Mesh,
};
use std::sync::Arc;

pub const TEXTO: Color32 = Color32::from_rgb(0xF4, 0xF6, 0xF8);
pub const TEXTO_2: Color32 = Color32::from_rgba_premultiplied(0xA1, 0xA2, 0xA4, 0xA8);
pub const FOCO: Color32 = Color32::WHITE;
pub const TINTA: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
pub const ERROR: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x80);
pub const FONDO_NEUTRO: Color32 = Color32::from_rgb(0x1A, 0x1D, 0x24);

pub fn cristal() -> Color32 {
    Color32::from_white_alpha(36)
}
pub fn cristal_fuerte() -> Color32 {
    Color32::from_white_alpha(56)
}

/// Radio único para fichas y botones.
pub const RADIO: u8 = 6;
/// Margen lateral de la pantalla.
pub const MARGEN: f32 = 64.0;

pub fn cuerpo(t: f32) -> FontId {
    FontId::proportional(t)
}
pub fn medio(t: f32) -> FontId {
    FontId::new(t, FontFamily::Name("medio".into()))
}
pub fn semi(t: f32) -> FontId {
    FontId::new(t, FontFamily::Name("semi".into()))
}
pub fn estrecha(t: f32) -> FontId {
    FontId::new(t, FontFamily::Name("estrecha".into()))
}
pub fn estrecha_fuerte(t: f32) -> FontId {
    FontId::new(t, FontFamily::Name("estrecha-fuerte".into()))
}

/// Escala 1,25 sobre 16.
pub mod escala {
    pub const DISPLAY: f32 = 61.0;
    pub const TITULO: f32 = 39.0;
    pub const SECCION: f32 = 25.0;
    pub const PESTANA: f32 = 20.0;
    pub const CUERPO: f32 = 16.0;
    pub const ETIQUETA: f32 = 14.0;
}

pub fn aplicar(ctx: &egui::Context) {
    let mut f = FontDefinitions::default();
    let respaldo = f.families[&FontFamily::Proportional].clone();
    for (nombre, datos) in [
        ("barlow", &include_bytes!("../assets/barlow-400.ttf")[..]),
        ("barlow-500", &include_bytes!("../assets/barlow-500.ttf")[..]),
        ("barlow-600", &include_bytes!("../assets/barlow-600.ttf")[..]),
        ("barlow-c-600", &include_bytes!("../assets/barlow-condensed-600.ttf")[..]),
        ("barlow-c-700", &include_bytes!("../assets/barlow-condensed-700.ttf")[..]),
    ] {
        f.font_data.insert(nombre.into(), Arc::new(FontData::from_static(datos)));
    }
    f.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "barlow".into());
    for (familia, fuente) in [
        ("medio", "barlow-500"),
        ("semi", "barlow-600"),
        ("estrecha", "barlow-c-600"),
        ("estrecha-fuerte", "barlow-c-700"),
    ] {
        let mut lista = vec![fuente.to_string()];
        lista.extend(respaldo.iter().cloned());
        f.families.insert(FontFamily::Name(familia.into()), lista);
    }
    ctx.set_fonts(f);
    ctx.set_theme(egui::Theme::Dark);

    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, estrecha_fuerte(escala::TITULO)),
            (TextStyle::Body, cuerpo(escala::CUERPO)),
            (TextStyle::Button, semi(escala::CUERPO)),
            (TextStyle::Small, medio(escala::ETIQUETA)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.item_spacing = Vec2::new(16.0, 8.0);
        s.spacing.button_padding = Vec2::new(16.0, 8.0);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.dormant_handle_opacity = 0.0;
        s.spacing.scroll.dormant_background_opacity = 0.0;
        s.animation_time = 0.16;

        let v = &mut s.visuals;
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = TINTA;
        v.extreme_bg_color = cristal();
        v.override_text_color = None;
        v.hyperlink_color = TEXTO;
        v.selection.bg_fill = Color32::from_white_alpha(70);
        v.selection.stroke = Stroke::new(2.0, FOCO);
        v.text_cursor.stroke = Stroke::new(2.0, FOCO);
        v.popup_shadow = egui::epaint::Shadow::NONE;
        v.window_shadow = egui::epaint::Shadow::NONE;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.bg_fill = cristal();
            w.weak_bg_fill = cristal();
            w.fg_stroke = Stroke::new(1.5, TEXTO);
            w.corner_radius = CornerRadius::same(RADIO);
            w.bg_stroke = Stroke::NONE;
            w.expansion = 0.0;
        }
        v.widgets.hovered.weak_bg_fill = cristal_fuerte();
        v.widgets.hovered.bg_fill = cristal_fuerte();
    });
}

/// Fondo oscuro teñido con el color de un juego.
pub fn tinte(c: Color32) -> Color32 {
    let [r, g, b, _] = c.to_array();
    let max = r.max(g).max(b).max(1) as f32;
    let k = 46.0 / max;
    // Algo de gris para que los colores muy saturados no tiñan de más.
    let mezcla = |x: u8| ((x as f32 * k) * 0.75 + 12.0 * 0.25) as u8;
    Color32::from_rgb(mezcla(r), mezcla(g), mezcla(b))
}

pub fn mezclar(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, _] = a.to_array();
    let [br, bg, bb, _] = b.to_array();
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(ar, br), l(ag, bg), l(ab, bb))
}

pub fn degradado_v(p: &egui::Painter, r: Rect, arriba: Color32, abajo: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), arriba);
    m.colored_vertex(r.right_top(), arriba);
    m.colored_vertex(r.left_bottom(), abajo);
    m.colored_vertex(r.right_bottom(), abajo);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 2, 3);
    p.add(m);
}

pub fn degradado_h(p: &egui::Painter, r: Rect, izq: Color32, der: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), izq);
    m.colored_vertex(r.right_top(), der);
    m.colored_vertex(r.left_bottom(), izq);
    m.colored_vertex(r.right_bottom(), der);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 2, 3);
    p.add(m);
}

/// Borde de foco de consola: blanco, por fuera de la ficha.
pub fn foco(p: &egui::Painter, r: Rect) {
    p.rect_stroke(r.expand(4.0), RADIO + 3, Stroke::new(3.0, FOCO), StrokeKind::Inside);
}

/// Pista de botón: círculo con letra (mando) o tecla (teclado).
pub fn pista(p: &egui::Painter, pos: Pos2, boton: &str, texto: &str, mando: bool) -> f32 {
    let x0 = pos.x;
    let ancho_boton = if mando {
        let c = Pos2::new(pos.x + 11.0, pos.y);
        p.circle_filled(c, 11.0, Color32::from_white_alpha(230));
        p.text(c, Align2::CENTER_CENTER, boton, semi(13.0), TINTA);
        22.0
    } else {
        let g = p.layout_no_wrap(boton.to_string(), semi(13.0), TEXTO);
        let w = (g.size().x + 14.0).max(24.0);
        let r = Rect::from_min_size(Pos2::new(pos.x, pos.y - 11.0), Vec2::new(w, 22.0));
        p.rect_stroke(r, 4, Stroke::new(1.5, Color32::from_white_alpha(170)), StrokeKind::Inside);
        p.galley(r.center() - g.size() / 2.0, g, TEXTO);
        w
    };
    let g = p.layout_no_wrap(texto.to_string(), medio(escala::ETIQUETA), TEXTO);
    let w = g.size().x;
    p.galley(Pos2::new(x0 + ancho_boton + 8.0, pos.y - g.size().y / 2.0), g, TEXTO);
    ancho_boton + 8.0 + w
}

/// 12345 → «12,3 mil»; 1_200_000 → «1,2 M».
pub fn abreviar(n: u64) -> String {
    let f = |x: f64, suf: &str| {
        let s = format!("{x:.1}").replace('.', ",");
        let s = s.strip_suffix(",0").unwrap_or(&s).to_string();
        format!("{s} {suf}")
    };
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => f(n as f64 / 1e3, "mil"),
        1_000_000..1_000_000_000 => f(n as f64 / 1e6, "M"),
        _ => f(n as f64 / 1e9, "mil M"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abrevia() {
        assert_eq!(abreviar(999), "999");
        assert_eq!(abreviar(12_345), "12,3 mil");
        assert_eq!(abreviar(2_000_000), "2 M");
    }

    #[test]
    fn tinte_es_oscuro() {
        let t = tinte(Color32::from_rgb(250, 120, 30));
        let [r, g, b, _] = t.to_array();
        assert!(r.max(g).max(b) <= 50);
        assert!(r > b, "conserva el tono");
    }
}
