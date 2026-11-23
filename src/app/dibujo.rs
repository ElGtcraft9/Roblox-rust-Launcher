//! Utilidades de dibujo compartidas por las pantallas.

use super::*;

/// Quita los caracteres que la fuente no puede dibujar (emojis de los títulos)
/// y los corchetes que se quedan vacíos.
pub(super) fn limpiar(ui: &Ui, texto: &str, fuente: &egui::FontId) -> String {
    let mut s: String =
        ui.fonts_mut(|f| texto.chars().filter(|c| c.is_whitespace() || f.has_glyph(fuente, *c)).collect());
    for vacio in ["[]", "()", "【】", "[ ]", "( )"] {
        s = s.replace(vacio, "");
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn galley(ui: &Ui, texto: &str, fuente: egui::FontId, color: Color32, ancho: f32, filas: usize) -> Arc<egui::Galley> {
    let texto = limpiar(ui, crate::idioma::traducir(texto), &fuente);
    let mut job = LayoutJob::single_section(texto, TextFormat::simple(fuente, color));
    job.wrap = TextWrapping {
        max_width: ancho,
        max_rows: filas,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    ui.fonts_mut(|f| f.layout_job(job))
}

/// Pinta una textura llenando el rectángulo sin deformarla (recorte centrado).
pub(super) fn cubrir(p: &egui::Painter, tex: &TextureHandle, r: Rect, alfa: f32) {
    let [w, h] = tex.size();
    let prop_img = w as f32 / h.max(1) as f32;
    let prop = r.width() / r.height().max(1.0);
    let uv = if prop > prop_img {
        let k = prop_img / prop;
        Rect::from_min_max(Pos2::new(0.0, (1.0 - k) / 2.0), Pos2::new(1.0, (1.0 + k) / 2.0))
    } else {
        let k = prop / prop_img;
        Rect::from_min_max(Pos2::new((1.0 - k) / 2.0, 0.0), Pos2::new((1.0 + k) / 2.0, 1.0))
    };
    p.image(tex.id(), r, uv, Color32::from_white_alpha((alfa.clamp(0.0, 1.0) * 255.0) as u8));
}

/// Imagen con esquinas redondeadas, recortada por el painter dado.
pub(super) fn ficha_img(p: &egui::Painter, tex: Option<&TextureHandle>, r: Rect, tinte: Color32) {
    match tex {
        Some(t) => {
            let forma = egui::epaint::RectShape::filled(r, RADIO, tinte).with_texture(
                t.id(),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            );
            p.add(forma);
        }
        None => {
            p.rect_filled(r, RADIO, cristal());
        }
    }
}

/// Borde suave bajo el ratón (el foco de verdad es el borde blanco).
pub(super) fn resalte(p: &egui::Painter, r: Rect) {
    p.rect_stroke(r.expand(3.0), RADIO + 2, Stroke::new(2.0, Color32::from_white_alpha(110)), StrokeKind::Inside);
}

/// El ratón se ha movido en este fotograma, para enfocar al pasar por encima.
/// Solo en modo consola: en modo PC el foco se mueve con la rueda, las flechas
/// o un clic, y el ratón solo resalta.
pub(super) fn raton_movido(ui: &Ui, consola: bool) -> bool {
    if !consola {
        return false;
    }
    #[cfg(debug_assertions)]
    if std::env::var_os("RUST_LAUNCHER_CAPTURA").is_some() {
        return false;
    }
    ui.input(|i| i.pointer.delta() != Vec2::ZERO)
}

pub(super) enum EstiloBoton {
    Principal,
    Secundario,
}

/// Botón de consola: alto fijo, texto semi, blanco (principal) o cristal.
pub(super) fn boton(ui: &mut Ui, pos: Pos2, texto: &str, estilo: EstiloBoton, id: &str) -> (egui::Response, f32) {
    let g = ui.painter().layout_no_wrap(texto.to_string(), semi(18.0), TEXTO);
    let r = Rect::from_min_size(pos, Vec2::new(g.size().x + 56.0, 52.0));
    let resp = ui.interact(r, Id::new(("boton", id)), Sense::click());
    let (fondo, color) = match estilo {
        EstiloBoton::Principal if resp.hovered() => (Color32::from_gray(225), TINTA),
        EstiloBoton::Principal => (FOCO, TINTA),
        EstiloBoton::Secundario if resp.hovered() => (cristal_fuerte(), TEXTO),
        EstiloBoton::Secundario => (cristal(), TEXTO),
    };
    let r_vis = if resp.is_pointer_button_down_on() { r.shrink(1.5) } else { r };
    ui.painter().rect_filled(r_vis, RADIO, fondo);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, texto, semi(18.0), color);
    (resp.on_hover_cursor(CursorIcon::PointingHand), r.width())
}

pub(super) const EN_LINEA: Color32 = Color32::from_rgb(0x5C, 0xC9, 0x8E);

pub(super) fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

/// Suelo de escenario: elipses concéntricas que imitan un foco de luz.
pub(super) fn suelo(p: &egui::Painter, centro: Pos2, lado: f32) {
    for i in 0..12 {
        let k = 1.0 - i as f32 / 12.0;
        let r = Rect::from_center_size(Pos2::new(centro.x, centro.y + lado * 0.42), Vec2::new(lado * 0.95 * k, lado * 0.16 * k));
        p.rect_filled(r, (r.height() / 2.0).min(255.0) as u8, Color32::from_white_alpha(4));
    }
}

/// Mando dibujado con formas. `t` anima los botones (a partir de 0,55 s
/// se encienden A, B, X, Y); con `t` grande queda en reposo.
pub(super) fn dibujar_mando(p: &egui::Painter, c: Pos2, w: f32, t: f32, alfa: f32) {
    let a = |col: Color32| col.gamma_multiply(alfa);
    let cuerpo_col = Color32::from_rgb(0xEE, 0xF0, 0xF3);
    let sombra = Color32::from_rgb(0xB4, 0xBA, 0xC4);
    let oscuro = Color32::from_rgb(0x2A, 0x2E, 0x36);
    let radio = |x: f32| x.clamp(0.0, 255.0) as u8;
    for (dy, col) in [(w * 0.03, sombra), (0.0, cuerpo_col)] {
        for lado in [-1.0, 1.0] {
            p.circle_filled(c + Vec2::new(lado * 0.29 * w, 0.11 * w + dy), 0.17 * w, a(col));
        }
        let cuerpo = Rect::from_min_max(c + Vec2::new(-0.38 * w, -0.17 * w + dy), c + Vec2::new(0.38 * w, 0.12 * w + dy));
        p.rect_filled(cuerpo, radio(0.15 * w), a(col));
    }
    // Sticks (el izquierdo se mueve un poco durante la animación).
    let vaiven = if (0.7..1.7).contains(&t) { (t * 9.0).sin() * 0.012 * w } else { 0.0 };
    for centro in [c + Vec2::new(-0.21 * w + vaiven, -0.05 * w), c + Vec2::new(0.10 * w, 0.06 * w)] {
        p.circle_filled(centro, 0.075 * w, a(oscuro));
        p.circle_filled(centro, 0.05 * w, a(Color32::from_rgb(0x3C, 0x42, 0x4D)));
    }
    // Cruceta.
    let cruz = c + Vec2::new(-0.10 * w, 0.06 * w);
    p.rect_filled(Rect::from_center_size(cruz, Vec2::new(0.12 * w, 0.038 * w)), 2, a(oscuro));
    p.rect_filled(Rect::from_center_size(cruz, Vec2::new(0.038 * w, 0.12 * w)), 2, a(oscuro));
    // Botón central.
    p.circle_filled(c + Vec2::new(0.0, -0.10 * w), 0.032 * w, a(oscuro));
    // A, B, X, Y.
    let base = c + Vec2::new(0.21 * w, -0.05 * w);
    let d = 0.056 * w;
    let botones = [
        ("A", Vec2::new(0.0, d), Color32::from_rgb(0x3D, 0xBE, 0x5A)),
        ("B", Vec2::new(d, 0.0), Color32::from_rgb(0xE5, 0x48, 0x4D)),
        ("X", Vec2::new(-d, 0.0), Color32::from_rgb(0x2D, 0x7F, 0xF9)),
        ("Y", Vec2::new(0.0, -d), Color32::from_rgb(0xF5, 0xC5, 0x18)),
    ];
    for (i, (letra, off, col)) in botones.iter().enumerate() {
        let ti = 0.55 + i as f32 * 0.12;
        let k = ease_out_back(((t - ti) / 0.28).clamp(0.0, 1.0));
        let r = 0.027 * w * k.max(0.0);
        let pos = base + *off;
        let brillo = (1.0 - (t - ti) / 0.45).clamp(0.0, 1.0);
        if brillo > 0.0 && t > ti {
            p.circle_stroke(pos, r * 1.7, Stroke::new(2.0, a(col.gamma_multiply(0.7 * brillo))));
        }
        if r > 0.5 {
            p.circle_filled(pos, r, a(*col));
            if r > 6.0 {
                p.text(pos, Align2::CENTER_CENTER, *letra, semi(r * 1.2), a(Color32::WHITE));
            }
        }
    }
}

/// Monigote clásico para jugar sin sesión («Guest»). `pie` es el centro de
/// la base y `alto` la altura total.
pub(super) fn dibujar_invitado(p: &egui::Painter, pie: Pos2, alto: f32) {
    let u = alto / 10.0;
    let amarillo = Color32::from_rgb(0xF5, 0xCD, 0x30);
    let azul = Color32::from_rgb(0x0D, 0x69, 0xAC);
    let verde = Color32::from_rgb(0xA4, 0xBD, 0x47);
    let r = |x: f32| x.clamp(0.0, 255.0) as u8;
    p.rect_filled(Rect::from_center_size(pie, Vec2::new(5.0 * u, 0.7 * u)), r(0.35 * u), Color32::from_black_alpha(70));
    for x in [-1.5, 0.05] {
        p.rect_filled(Rect::from_min_size(pie + Vec2::new(x * u, -4.0 * u), Vec2::new(1.45 * u, 4.0 * u)), r(0.12 * u), verde);
    }
    p.rect_filled(Rect::from_min_size(pie + Vec2::new(-1.5 * u, -7.2 * u), Vec2::new(3.0 * u, 3.25 * u)), r(0.12 * u), azul);
    for x in [-2.75, 1.55] {
        p.rect_filled(Rect::from_min_size(pie + Vec2::new(x * u, -7.2 * u), Vec2::new(1.2 * u, 3.2 * u)), r(0.12 * u), amarillo);
    }
    let cabeza = Rect::from_center_size(pie + Vec2::new(0.0, -8.35 * u), Vec2::new(2.0 * u, 1.9 * u));
    p.rect_filled(Rect::from_center_size(cabeza.center_top(), Vec2::new(0.9 * u, 0.5 * u)), r(0.1 * u), amarillo);
    p.rect_filled(cabeza, r(0.45 * u), amarillo);
    let ojo = Color32::from_rgb(0x1E, 0x1E, 0x1E);
    for x in [-0.36, 0.36] {
        p.circle_filled(cabeza.center() + Vec2::new(x * u, -0.18 * u), 0.12 * u, ojo);
    }
    let sonrisa: Vec<Pos2> = (0..=10)
        .map(|i| {
            let a = std::f32::consts::PI * (0.15 + 0.7 * i as f32 / 10.0);
            cabeza.center() + Vec2::new(a.cos() * 0.5 * u, 0.12 * u + a.sin() * 0.32 * u)
        })
        .collect();
    p.add(egui::Shape::line(sonrisa, Stroke::new((0.11 * u).max(1.5), ojo)));
}

// ---------- Campos de texto, mercado y social ----------

/// Resultado de dibujar un campo de texto.
pub(super) struct CampoRes {
    /// Se pulsó Enter en el campo (modo PC).
    pub(super) enviado: bool,
    /// El campo tiene el foco del teclado físico (modo PC).
    pub(super) foco: bool,
    pub(super) resp: Option<egui::Response>,
}

/// 1234567 → «1.234.567».
pub(super) fn miles(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}
