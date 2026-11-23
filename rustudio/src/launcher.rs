//! rustudio dentro de RustLauncher (pestaña Crear).
//!
//! El launcher crea el editor con [`embedded`] sobre su propia ventana y su
//! propio dispositivo wgpu, lo dibuja en su `ui` y, después de cada
//! fotograma, recoge con [`take_request`] lo que se pidió desde el menú
//! «RustLauncher» del editor (volver al launcher o jugar).

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

static EMBEDDED: AtomicBool = AtomicBool::new(false);
static REQUEST: AtomicU8 = AtomicU8::new(0);
static PLACE: AtomicU64 = AtomicU64::new(0);

/// Lo que el usuario pidió desde el menú del launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Volver a las pestañas del launcher.
    Back,
    /// Volver al launcher para elegir un juego.
    Play,
    /// Jugar en Roblox a la experiencia abierta en el editor (place id).
    PlayPlace(u64),
}

/// The editor view's upscaler: 1 = FSR 1, 2 = FSR 2, 3 = FSR 3.1
/// (RustLauncher Settings > Escalado del editor).
pub fn set_escalado(modo: u8) {
    use rustudio_render::escalado::{Modo, set_modo};
    set_modo(match modo {
        1 => Modo::Fsr1,
        2 => Modo::Fsr2,
        _ => Modo::Fsr3,
    });
}

/// Frame generation in the editor view (experimental).
pub fn set_generar(on: bool) {
    rustudio_render::escalado::set_generar(on);
}

pub fn is_embedded() -> bool {
    EMBEDDED.load(Ordering::Relaxed)
}

pub(crate) fn request(r: Request) {
    let (code, place) = match r {
        Request::Back => (1, 0),
        Request::Play => (2, 0),
        Request::PlayPlace(p) => (3, p),
    };
    PLACE.store(place, Ordering::Relaxed);
    REQUEST.store(code, Ordering::Release);
}

/// La petición pendiente, si la hay (y la borra).
pub fn take_request() -> Option<Request> {
    match REQUEST.swap(0, Ordering::Acquire) {
        1 => Some(Request::Back),
        2 => Some(Request::Play),
        3 => Some(Request::PlayPlace(PLACE.load(Ordering::Relaxed))),
        _ => None,
    }
}

/// El editor sobre la ventana del launcher. `api_dump`: ruta del
/// api-dump.json que el launcher dejó en su carpeta de datos.
pub fn embedded(
    ctx: &egui::Context,
    render_state: Option<egui_wgpu::RenderState>,
    wayland: Option<bool>,
    api_dump: Option<std::path::PathBuf>,
) -> crate::app::RustudioApp {
    EMBEDDED.store(true, Ordering::Relaxed);
    // SAFETY (edición 2024): se llama una vez, desde el hilo de la
    // interfaz, antes de que el editor lea estas variables.
    unsafe {
        // Las actualizaciones las hace el launcher: el actualizador de
        // rustudio reemplazaría el ejecutable del launcher.
        std::env::set_var("RUSTUDIO_NO_UPDATE_CHECK", "1");
        if let Some(p) = api_dump {
            std::env::set_var("RUSTUDIO_API_DUMP", p);
        }
    }
    // Gráficas integradas (AMD Vega/RDNA de los Ryzen U, Intel UHD/Iris):
    // rustudio las trata como capaces y las pone a calidad máxima, ~12 ms de
    // GPU por fotograma solo en efectos. En Wayland eso pierde el refresco y
    // cae a 30 fps, y la GPU, medio parada, no sube de frecuencia. Medido en
    // un Ryzen 7 5700U (Laser Tag, vista 768x428): 44,8 fps (1 % peor: 18,7)
    // con calidad máxima, 59,9 fps (1 % peor: 50,8) con el modo de gráfica
    // débil, que empieza sin efectos caros y sube la calidad sola mientras
    // los tiempos lo permiten. RUSTUDIO_LOWEND=0 lo desactiva.
    if std::env::var_os("RUSTUDIO_LOWEND").is_none()
        && render_state
            .as_ref()
            .is_some_and(|rs| rs.adapter.get_info().device_type == egui_wgpu::wgpu::DeviceType::IntegratedGpu)
    {
        // SAFETY: como arriba.
        unsafe { std::env::set_var("RUSTUDIO_LOWEND", "1") };
    }
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        rustudio_core::diagnostics::init();
        rustudio_core::load_trace::init();
        crate::studio_hooks::install();
        rustudio_core::studio_content::spawn_background_install_if_missing();
    });
    let harness = crate::app::Harness::embedded();
    // Pruebas de rendimiento: un lugar abierto al entrar (ver Harness::embedded).
    let open = std::env::var_os("RUSTUDIO_EMBED_OPEN").map(std::path::PathBuf::from);
    let guardar = crate::layout_store::enabled(&harness);
    crate::studio_settings::init(guardar);
    rustudio_core::rustudio_settings::init(guardar);
    rustudio_core::profiler::load_saved_config();
    let mut app = crate::app::RustudioApp::new_in(ctx, render_state, wayland, open, harness, false);
    app.ed.plugins.set_repaint_context(ctx.clone());
    app
}

impl crate::app::Harness {
    /// El editor tal como lo usa una persona. Para medir el rendimiento
    /// dentro del launcher (mismas medidas que `--bench` y
    /// `--profile-capture` de rustudio): RUSTUDIO_EMBED_OPEN=lugar.rbxl,
    /// RUSTUDIO_EMBED_BENCH=salida.json, RUSTUDIO_EMBED_PROFILE=salida.rsprof,
    /// RUSTUDIO_EMBED_SECONDS=10.
    pub fn embedded() -> Self {
        use crate::run_harness::{HarnessInput, RunHarness};
        let var = |k: &str| std::env::var_os(k).map(std::path::PathBuf::from);
        let seconds: f32 =
            std::env::var("RUSTUDIO_EMBED_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(10.0);
        let bench = var("RUSTUDIO_EMBED_BENCH").map(|out| rustudio_core::bench::Bench::new(Some(out), "embedded", false, seconds));
        let profile =
            var("RUSTUDIO_EMBED_PROFILE").map(|out| rustudio_core::profiler::cli::CliCapture::new(out, seconds, false, false));
        Self {
            run: RunHarness {
                input: HarnessInput { actions: Vec::new(), pointer: None, shift: false },
                shot_after: None,
                log_out: None,
                started: rustudio_core::sim_clock::now(),
                screenshot: None,
                screenshot_requested: false,
                start_play: false,
                bench,
                profile,
                camera: None,
                shots: Vec::new(),
                shot_next: 0,
                shot_lines: Vec::new(),
                from_play: false,
                play_started: None,
                play_game_time: None,
                frame_ms: Vec::new(),
                last_frame: None,
                no_ui: false,
                no_render: false,
                end_after: None,
                fail_on_error: false,
                result: None,
                end_value: None,
                run_frames: Vec::new(),
                run_last: None,
                run_playing: false,
                finished: false,
                held: 0.0,
            },
            selftest: None,
            ui_shots: None,
            open_roblox: None,
            team_create: None,
            roblox_picker: None,
            roblox_settings: false,
            ui_due: Vec::new(),
            overlays: false,
            plugin_files: Vec::new(),
            installed_plugins: Vec::new(),
            plugin_dirs: Vec::new(),
            no_plugins: false,
            plugins_started: false,
        }
    }
}
