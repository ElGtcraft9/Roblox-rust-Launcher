//! Pestaña Crear: rustudio dentro de la ventana del launcher.
//!
//! Al entrar se muestra durante 2 s un martillo golpeando un yunque mientras
//! el editor arranca (la primera vez). Después el editor ocupa toda la
//! ventana y se dibuja con el mismo contexto de egui y el mismo dispositivo
//! wgpu que el launcher. Desde su menú «RustLauncher» se vuelve a las
//! pestañas o se juega.
//!
//! rustudio cambia el estilo global de egui y sus fuentes: al entrar y al
//! salir se intercambian los estilos de uno y otro, y las fuentes del
//! launcher viven en familias propias que rustudio no toca.

use super::*;
use eframe::egui::{Shape, Stroke};
use rustudio::launcher::Request;

/// Lo que dura la animación del martillo.
const ANIMACION: f32 = 2.0;

#[derive(Default)]
pub(super) struct Taller {
    /// El editor, creado la primera vez que se entra; se conserva al salir
    /// para no perder el lugar abierto.
    editor: Option<Box<rustudio::app::RustudioApp>>,
    /// Si la pestaña Crear ocupa ahora la ventana.
    pub(super) activo: bool,
    /// Se pidió entrar (al elegir la pestaña); se entra en el siguiente fotograma.
    pub(super) pedido: bool,
    entrada: Option<Instant>,
    estilo_launcher: Option<Arc<egui::Style>>,
    estilo_estudio: Option<Arc<egui::Style>>,
    zoom_launcher: f32,
    error: Option<String>,
    /// El dispositivo de la ventana (wgpu) y si es Wayland.
    pub(super) render: Option<eframe::egui_wgpu::RenderState>,
    pub(super) wayland: Option<bool>,
    /// Depuración: fotograma de la secuencia que se está grabando.
    secuencia: u32,
    secuencia_pedida: bool,
}

impl App {
    pub(super) fn entrar_taller(&mut self, ctx: &egui::Context) {
        let t = &mut self.taller;
        if t.activo {
            return;
        }
        t.activo = true;
        t.entrada = Some(Instant::now());
        t.error = None;
        t.estilo_launcher = Some(ctx.global_style());
        t.zoom_launcher = ctx.zoom_factor();
        // El editor está pensado para su propio tamaño de texto.
        ctx.set_zoom_factor(1.0);
        if let Some(e) = &t.estilo_estudio {
            ctx.set_global_style(e.clone());
        }
        self.teclado = None;
    }

    pub(super) fn salir_taller(&mut self, ctx: &egui::Context) {
        let t = &mut self.taller;
        if !t.activo {
            return;
        }
        t.activo = false;
        if t.editor.is_some() {
            t.estilo_estudio = Some(ctx.global_style());
        }
        if let Some(e) = t.estilo_launcher.take() {
            ctx.set_global_style(e);
        }
        ctx.set_zoom_factor(t.zoom_launcher.max(0.5));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("RustLauncher".into()));
        idioma::para_launcher();
        self.cambiar_pestana(Pestana::Inicio);
    }

    /// Dibuja la pestaña Crear (animación, editor o error).
    pub(super) fn mostrar_taller(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[allow(unused_mut)]
        let mut pasado = self.taller.entrada.map_or(ANIMACION, |e| e.elapsed().as_secs_f32());
        #[cfg(debug_assertions)]
        if let Some(dir) = std::env::var_os("RUST_LAUNCHER_SECUENCIA") {
            pasado = self.grabar_secuencia(&ctx, std::path::Path::new(&dir));
        }

        if pasado < ANIMACION || (self.taller.editor.is_none() && self.taller.error.is_none()) {
            let pantalla = ctx.content_rect();
            egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
                pintar_forja(ui.painter(), pantalla, pasado, self.tinte_actual());
            });
            // El editor arranca detrás de la animación, cuando ya se ve el
            // primer golpe (crearlo bloquea un fotograma).
            if self.taller.editor.is_none() && self.taller.error.is_none() && pasado > 0.25 {
                self.crear_editor(&ctx);
            }
            ctx.request_repaint();
            return;
        }

        if let Some(e) = self.taller.error.clone() {
            self.pantalla_error_taller(ui, &e);
            return;
        }

        rustudio::launcher::set_escalado(self.cfg.escalador.numero());
        rustudio::launcher::set_generar(self.cfg.generar_fotogramas);
        let Some(editor) = self.taller.editor.as_mut() else { return };
        let t_ui = Instant::now();
        idioma::para_estudio();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            eframe::App::logic(editor.as_mut(), &ctx, frame);
            eframe::App::ui(editor.as_mut(), ui, frame);
        }));
        idioma::para_launcher();
        let vista = editor.ed.game.viewport_rect;
        if perfil_crear() {
            contar_causas(&ctx);
        }
        medir_fotograma(t_ui, || {
            format!(
                "ventana {:.0}x{:.0} pt, {:.2} px/pt, vista 3D {:.0}x{:.0} pt; redibujados pedidos por: {}",
                ctx.content_rect().width(),
                ctx.content_rect().height(),
                ctx.pixels_per_point(),
                vista.width(),
                vista.height(),
                causas_frecuentes()
            )
        });
        if r.is_err() {
            // Un fallo del editor no cierra el launcher: se descarta y se
            // vuelve a crear al entrar de nuevo.
            self.taller.editor = None;
            self.taller.estilo_estudio = None;
            self.taller.error = Some(tr("El editor se cerró por un error.").to_string());
            return;
        }

        match rustudio::launcher::take_request() {
            Some(Request::Back | Request::Play) => self.salir_taller(&ctx),
            Some(Request::PlayPlace(place_id)) => {
                self.salir_taller(&ctx);
                self.lanzar(
                    tr("Abriendo la experiencia").into(),
                    format!("roblox://experiences/start?placeId={place_id}"),
                );
            }
            None => {}
        }
    }

    /// Depuración (RUST_LAUNCHER_SECUENCIA=carpeta): guarda la animación
    /// fotograma a fotograma a 30 por segundo, con un reloj que avanza solo
    /// cuando se ha guardado el fotograma anterior.
    #[cfg(debug_assertions)]
    fn grabar_secuencia(&mut self, ctx: &egui::Context, dir: &std::path::Path) -> f32 {
        let img = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = img {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save_with_format(dir.join(format!("forja-{:04}.png", self.taller.secuencia)), image::ImageFormat::Png);
            }
            self.taller.secuencia += 1;
            self.taller.secuencia_pedida = false;
            if self.taller.secuencia as f32 >= ANIMACION * 30.0 {
                std::process::exit(0);
            }
        }
        if !self.taller.secuencia_pedida {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.taller.secuencia_pedida = true;
        }
        ctx.request_repaint();
        // (el editor nunca llega a crearse durante la grabación)
        self.taller.error = Some(String::new());
        self.taller.secuencia as f32 / 30.0
    }

    fn crear_editor(&mut self, ctx: &egui::Context) {
        let api_dump = preparar_api_dump();
        let render = self.taller.render.clone();
        let wayland = self.taller.wayland;
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rustudio::launcher::embedded(ctx, render, wayland, api_dump)
        }));
        match r {
            Ok(editor) => {
                self.taller.editor = Some(Box::new(editor));
                // rustudio acaba de poner su estilo. (Sus fuentes llevan las
                // del launcher: tema::aplicar las fijó en egui.)
                self.taller.estilo_estudio = Some(ctx.global_style());
            }
            Err(_) => {
                // Lo que rustudio cambiara a medias, fuera.
                if let Some(e) = &self.taller.estilo_launcher {
                    ctx.set_global_style(e.clone());
                }
                self.taller.error = Some(tr("No se pudo abrir el editor").to_string());
            }
        }
    }

    fn pantalla_error_taller(&mut self, ui: &mut Ui, error: &str) {
        let ctx = ui.ctx().clone();
        let pantalla = ctx.content_rect();
        let mut volver = false;
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            let p = ui.painter();
            p.rect_filled(pantalla, 0.0, FONDO_NEUTRO);
            let c = pantalla.center();
            p.text(c - Vec2::new(0.0, 40.0), Align2::CENTER_BOTTOM, error, estrecha_fuerte(escala::SECCION), ERROR);
            let (r, _) = boton(ui, c + Vec2::new(-70.0, 0.0), "Volver", EstiloBoton::Principal, "taller-volver");
            volver = r.clicked();
        });
        if volver || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.taller.error = None;
            self.salir_taller(&ctx);
        }
    }

    /// Lo que rustudio necesita ver de la entrada antes que egui.
    pub(super) fn entrada_taller(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if perfil_crear() {
            let mut c = CAUSAS.lock().unwrap();
            let mapa = c.get_or_insert_with(Default::default);
            if raw.events.is_empty() {
                *mapa.entry("(fotograma sin eventos de entrada)".into()).or_default() += 1;
            }
            for e in &raw.events {
                let nombre = format!("{e:?}");
                let nombre = nombre.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("?").to_string();
                *mapa.entry(format!("evento {nombre}")).or_default() += 1;
            }
        }
        if self.taller.activo
            && let Some(e) = self.taller.editor.as_mut()
        {
            eframe::App::raw_input_hook(e.as_mut(), ctx, raw);
        }
    }

    pub(super) fn cerrar_taller(&mut self) {
        if let Some(e) = self.taller.editor.as_mut() {
            eframe::App::on_exit(e.as_mut());
        }
    }
}

/// RUST_LAUNCHER_PERFIL_CREAR: cada 120 fotogramas, por la salida de
/// errores, el intervalo medio entre fotogramas y lo que tarda el editor
/// dentro de él (el resto es eframe: teselar, dibujar y presentar).
fn medir_fotograma(inicio: Instant, detalle: impl FnOnce() -> String) {
    use std::sync::Mutex;
    static ESTADO: Mutex<Option<(Instant, u32, f64, f64)>> = Mutex::new(None);
    if !perfil_crear() {
        return;
    }
    let ahora = Instant::now();
    let ui = ahora.duration_since(inicio).as_secs_f64() * 1000.0;
    let mut e = ESTADO.lock().unwrap();
    match e.as_mut() {
        None => *e = Some((ahora, 0, 0.0, 0.0)),
        Some((anterior, n, suma_int, suma_ui)) => {
            *suma_int += ahora.duration_since(*anterior).as_secs_f64() * 1000.0;
            *suma_ui += ui;
            *n += 1;
            *anterior = ahora;
            if *n == 120 {
                eprintln!(
                    "[crear] intervalo {:.2} ms ({:.1} fps), editor {:.2} ms, resto {:.2} ms; {}",
                    *suma_int / 120.0,
                    120_000.0 / *suma_int,
                    *suma_ui / 120.0,
                    (*suma_int - *suma_ui) / 120.0,
                    detalle()
                );
                *n = 0;
                *suma_int = 0.0;
                *suma_ui = 0.0;
            }
        }
    }
}

/// RUST_LAUNCHER_PERFIL_CREAR (leído una vez).
fn perfil_crear() -> bool {
    static ACTIVO: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ACTIVO.get_or_init(|| std::env::var_os("RUST_LAUNCHER_PERFIL_CREAR").is_some())
}

static CAUSAS: std::sync::Mutex<Option<std::collections::HashMap<String, u32>>> = std::sync::Mutex::new(None);

/// Quién pidió redibujar en el fotograma anterior (archivo:línea).
fn contar_causas(ctx: &egui::Context) {
    let mut c = CAUSAS.lock().unwrap();
    let mapa = c.get_or_insert_with(Default::default);
    for causa in ctx.repaint_causes() {
        *mapa.entry(format!("{}:{}", causa.file.rsplit("/src/").next().unwrap_or(causa.file), causa.line)).or_default() += 1;
    }
}

fn causas_frecuentes() -> String {
    let Some(mapa) = CAUSAS.lock().unwrap().take() else { return String::new() };
    let mut v: Vec<_> = mapa.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.iter().take(8).map(|(k, n)| format!("{k}×{n}")).collect::<Vec<_>>().join(", ")
}

/// rustudio necesita `api-dump.json` (la descripción de las clases de
/// Roblox). Va comprimido dentro del launcher y se deja en su carpeta de
/// datos la primera vez.
fn preparar_api_dump() -> Option<std::path::PathBuf> {
    static DUMP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/api-dump.json.gz"));
    let ruta = config::base().join("rustudio").join("api-dump.json");
    let marca = ruta.with_extension("tam");
    let tam = DUMP.len().to_string();
    if ruta.is_file() && std::fs::read_to_string(&marca).ok().as_deref() == Some(tam.as_str()) {
        return Some(ruta);
    }
    let _ = std::fs::create_dir_all(ruta.parent()?);
    let mut json = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(DUMP), &mut json).ok()?;
    let tmp = ruta.with_extension("tmp");
    std::fs::write(&tmp, json).ok()?;
    std::fs::rename(&tmp, &ruta).ok()?;
    let _ = std::fs::write(&marca, tam);
    Some(ruta)
}

// ── Animación de entrada: una forja de noche ─────────────────────────────
//
// La única luz de la escena es la barra de metal al rojo sobre el yunque:
// el yunque y el martillo son siluetas recortadas por su brillo. Con cada
// golpe la barra se calienta (rojo apagado → amarillo blanco) e ilumina más:
// el calor hace de barra de progreso. Al final, fundido a negro hacia el
// editor.

/// Golpes y el instante (en su ciclo) en que el martillo toca la barra.
const PERIODO: f32 = 0.45;
const IMPACTO: f32 = 0.8;
/// Últimos segundos: fundido hacia el editor.
const FUNDIDO: f32 = 0.22;

const NOCHE: Color32 = Color32::from_rgb(0x13, 0x0F, 0x0D);
const HIERRO: Color32 = Color32::from_rgb(0x2B, 0x27, 0x25);
const HIERRO_OSCURO: Color32 = Color32::from_rgb(0x1C, 0x19, 0x18);
const MADERA: Color32 = Color32::from_rgb(0x4A, 0x36, 0x28);
const HIERRO_MARTILLO: Color32 = Color32::from_rgb(0x3B, 0x36, 0x33);

/// Color del metal según su calor (0 frío … 1 al blanco).
fn calor(h: f32) -> Color32 {
    let tramos = [
        (0.0, Color32::from_rgb(0x4A, 0x16, 0x0C)),
        (0.35, Color32::from_rgb(0xB8, 0x32, 0x16)),
        (0.65, Color32::from_rgb(0xFF, 0x7A, 0x2E)),
        (0.85, Color32::from_rgb(0xFF, 0xB3, 0x47)),
        (1.0, Color32::from_rgb(0xFF, 0xDD, 0x92)),
    ];
    let h = h.clamp(0.0, 1.0);
    for w in tramos.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if h <= b {
            return mezclar(ca, cb, (h - a) / (b - a));
        }
    }
    tramos[4].1
}

/// Luz aditiva (alfa 0 premultiplicado): suma color a lo que hay debajo.
fn luz(c: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (c.r() as f32 * k) as u8,
        (c.g() as f32 * k) as u8,
        (c.b() as f32 * k) as u8,
        0,
    )
}

/// Resplandor elíptico: `centro` con color `c` que se apaga hacia el borde.
fn resplandor(p: &egui::Painter, centro: Pos2, radio: Vec2, c: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(centro, c);
    const N: u32 = 48;
    // Un anillo intermedio da una caída más suave que la lineal.
    for (f, k) in [(0.45f32, 0.35f32), (1.0, 0.0)] {
        for i in 0..N {
            let a = i as f32 / N as f32 * std::f32::consts::TAU;
            let v = Vec2::new(a.cos() * radio.x, a.sin() * radio.y) * f;
            let col = Color32::from_rgba_premultiplied(
                (c.r() as f32 * k) as u8,
                (c.g() as f32 * k) as u8,
                (c.b() as f32 * k) as u8,
                (c.a() as f32 * k) as u8,
            );
            m.colored_vertex(centro + v, col);
        }
    }
    for i in 0..N {
        let j = (i + 1) % N;
        m.add_triangle(0, 1 + i, 1 + j);
        m.add_triangle(1 + i, 1 + N + i, 1 + N + j);
        m.add_triangle(1 + i, 1 + N + j, 1 + j);
    }
    p.add(Shape::mesh(m));
}

/// Polígono convexo relleno.
fn pieza(p: &egui::Painter, puntos: Vec<Pos2>, c: Color32) {
    p.add(Shape::convex_polygon(puntos, c, Stroke::NONE));
}

/// Un martillo que golpea una barra al rojo sobre un yunque.
fn pintar_forja(p: &egui::Painter, pantalla: Rect, t: f32, tinte: Color32) {
    let s = (pantalla.height() / 720.0).clamp(0.7, 1.7);
    let golpe_n = (t / PERIODO).floor();
    let f = (t % PERIODO) / PERIODO;
    let golpes_dados = golpe_n + if f >= IMPACTO { 1.0 } else { 0.0 };
    // Tiempo desde el último impacto (para destello, temblor y chispas).
    let desde = if f >= IMPACTO { (f - IMPACTO) * PERIODO } else if golpe_n > 0.0 { (f + 1.0 - IMPACTO) * PERIODO } else { 9.0 };
    let destello = (1.0 - desde / 0.18).clamp(0.0, 1.0);
    // Calor de la barra: sube con cada golpe y baja un poco entre golpes.
    let h = (0.18 + golpes_dados * 0.21 - (desde.min(0.5) * 0.12)).clamp(0.12, 1.0);

    // Temblor de cámara en el impacto (3 px que se apagan en 120 ms).
    let k_temblor = (1.0 - desde / 0.12).clamp(0.0, 1.0);
    let temblor = Vec2::new((t * 91.0).sin(), (t * 67.0).cos()) * 3.0 * s * k_temblor;

    // Fondo: noche de fragua con un poco del color del juego enfocado.
    p.rect_filled(pantalla, 0.0, mezclar(NOCHE, tinte, 0.12));
    let c = pantalla.center() + Vec2::new(0.0, 10.0 * s) + temblor;
    let o = |x: f32, y: f32| c + Vec2::new(x, y) * s;
    let brasa = calor(h);

    // Luz de la barra sobre la pared y el suelo.
    let intensidad = (0.35 + 0.65 * h + 0.35 * destello).min(1.15);
    resplandor(p, o(0.0, -10.0), Vec2::new(520.0, 360.0) * s, luz(brasa, 0.22 * intensidad));
    resplandor(p, o(0.0, 150.0), Vec2::new(420.0, 60.0) * s, luz(brasa, 0.30 * intensidad));
    // Suelo: línea de horizonte apenas iluminada.
    p.line_segment(
        [Pos2::new(pantalla.left(), c.y + 150.0 * s), Pos2::new(pantalla.right(), c.y + 150.0 * s)],
        Stroke::new(1.0, mezclar(NOCHE, brasa, 0.10 * intensidad)),
    );

    // Yunque (siluetas convexas): cuerno, cara, cintura, pie.
    let cara = -14.0;
    pieza(p, vec![o(-130.0, cara), o(-215.0, cara + 4.0), o(-130.0, cara + 30.0)], HIERRO);
    pieza(p, vec![o(-130.0, cara), o(140.0, cara), o(140.0, cara + 40.0), o(-130.0, cara + 40.0)], HIERRO);
    pieza(p, vec![o(-70.0, cara + 40.0), o(80.0, cara + 40.0), o(52.0, cara + 96.0), o(-42.0, cara + 96.0)], HIERRO_OSCURO);
    pieza(p, vec![o(-42.0, cara + 96.0), o(52.0, cara + 96.0), o(118.0, cara + 150.0), o(-108.0, cara + 150.0)], HIERRO);
    // Filo iluminado: la cara superior recibe la luz de la barra.
    p.line_segment([o(-212.0, cara + 4.0), o(140.0, cara)], Stroke::new(2.0 * s, mezclar(HIERRO, brasa, 0.55 * intensidad.min(1.0))));
    p.line_segment([o(-108.0, cara + 150.0), o(118.0, cara + 150.0)], Stroke::new(1.5 * s, mezclar(HIERRO, brasa, 0.25 * intensidad.min(1.0))));

    // La barra al rojo, con un núcleo más claro.
    let barra = Rect::from_min_max(o(-62.0, cara - 14.0), o(58.0, cara));
    p.rect_filled(barra, 3.0 * s, brasa);
    p.rect_filled(barra.shrink2(Vec2::new(10.0, 4.5) * s), 2.0 * s, calor((h + 0.15).min(1.0)));
    resplandor(p, barra.center(), Vec2::new(110.0, 46.0) * s, luz(brasa, 0.5 + 0.25 * destello));

    // Martillo: gira alrededor del extremo del mango (a la derecha).
    let bajada = if f < 0.55 {
        let k = f / 0.55;
        1.0 - k * k * (3.0 - 2.0 * k)
    } else if f < IMPACTO {
        let k = (f - 0.55) / (IMPACTO - 0.55);
        k * k * k
    } else {
        let k = (f - IMPACTO) / (1.0 - IMPACTO);
        1.0 - 0.08 * (k * std::f32::consts::PI).sin()
    };
    let pivote = o(250.0, cara - 52.0);
    let angulo = 68f32.to_radians() * (1.0 - bajada);
    let (sn, cs) = angulo.sin_cos();
    let gira = |x: f32, y: f32| pivote + Vec2::new(x * cs - y * sn, x * sn + y * cs) * s;
    let largo = 248.0;
    pieza(p, vec![gira(0.0, -5.0), gira(-largo, -5.0), gira(-largo, 5.0), gira(0.0, 5.0)], MADERA);
    // Cabeza: su cara de golpe (abajo) es la que toca la barra.
    let hx = -largo - 6.0;
    let (medio_ancho, arriba, abajo) = (24.0, -40.0, 38.0);
    pieza(
        p,
        vec![gira(hx + medio_ancho, arriba), gira(hx - medio_ancho, arriba), gira(hx - medio_ancho, abajo), gira(hx + medio_ancho, abajo)],
        HIERRO_MARTILLO,
    );
    // Contraluz en la cara de golpe, más fuerte cuanto más cerca de la barra.
    let cerca = bajada.powf(3.0);
    p.line_segment(
        [gira(hx - medio_ancho, abajo), gira(hx + medio_ancho, abajo)],
        Stroke::new(3.0 * s, mezclar(HIERRO, brasa, 0.2 + 0.8 * cerca)),
    );
    p.line_segment(
        [gira(hx - medio_ancho, arriba), gira(hx - medio_ancho, abajo)],
        Stroke::new(2.0 * s, mezclar(HIERRO, brasa, 0.35 + 0.45 * cerca)),
    );
    // El mango también recoge algo de luz por abajo.
    p.line_segment([gira(-largo + 4.0, 5.0), gira(-24.0, 5.0)], Stroke::new(1.5 * s, mezclar(MADERA, brasa, 0.35)));

    // Chispas: salen del punto de impacto, caen y se enfrían.
    if golpe_n > 0.0 || f >= IMPACTO {
        let n_golpe = golpes_dados;
        let punto = o(-4.0, cara - 14.0);
        let vida = desde / 0.42;
        if vida < 1.0 {
            for i in 0..26 {
                let semilla = ((n_golpe * 31.0 + i as f32 * 12.9898).sin() * 43758.547).fract().abs();
                let semilla2 = ((n_golpe * 17.0 + i as f32 * 78.233).sin() * 12345.678).fract().abs();
                let ang = std::f32::consts::PI * (1.08 + 0.84 * semilla);
                let vel = (180.0 + 260.0 * semilla2) * s;
                let tt = desde;
                let pos = punto + Vec2::angled(ang) * vel * tt + Vec2::new(0.0, 520.0 * s * tt * tt);
                let dir = (Vec2::angled(ang) * vel + Vec2::new(0.0, 1040.0 * s * tt)).normalized();
                let largo_chispa = (14.0 - 8.0 * vida) * s;
                let col = calor(1.0 - vida * 0.9);
                let alfa = (1.0 - vida).powf(1.5);
                p.line_segment([pos - dir * largo_chispa, pos], Stroke::new(2.6 * s, luz(col, alfa)));
                p.circle_filled(pos, 2.2 * s, luz(col, alfa * 0.9));
            }
        }
        // Destello blanco en el instante del golpe.
        resplandor(p, punto, Vec2::splat(60.0 * s), luz(Color32::from_rgb(0xFF, 0xE6, 0xB0), destello * 0.55));
    }

    // Título, alineado a la izquierda como el resto de páginas.
    let x = pantalla.left() + MARGEN;
    let y = pantalla.bottom() - MARGEN;
    p.text(Pos2::new(x, y - 28.0), Align2::LEFT_BOTTOM, "Crear", estrecha_fuerte(escala::TITULO), TEXTO);
    p.text(Pos2::new(x, y), Align2::LEFT_BOTTOM, "Abriendo el editor…", cuerpo(escala::CUERPO), TEXTO_2);

    // Fundido a negro hacia el editor.
    let fundido = ((t - (ANIMACION - FUNDIDO)) / FUNDIDO).clamp(0.0, 1.0);
    if fundido > 0.0 {
        p.rect_filled(pantalla, 0.0, Color32::BLACK.gamma_multiply(fundido));
    }
}
