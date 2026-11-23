//! RustLauncher: launcher alternativo y rápido para Roblox.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod acelerar;
mod actualizar;
mod api;
mod app;
mod config;
mod flags;
mod idioma;
mod imagenes;
mod mando;
mod perfil;
mod plataforma;
mod roblox;
mod sesion;
mod tema;
mod teclado;
mod vigia;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("Plataforma no soportada: solo Windows y Linux.");

use eframe::egui;

// Asignador de memoria: egui, Luau y la física del editor reservan y liberan
// memoria sin parar; mimalloc lo hace más deprisa que el de glibc/Windows.
#[global_allocator]
static ASIGNADOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> eframe::Result<()> {
    // Pruebas de rendimiento: rustudio solo, con su propio bucle de ventana y
    // sus opciones de línea de órdenes, compilado igual que dentro del launcher.
    // RUST_LAUNCHER_RUSTUDIO=1 rust-launcher lugar.rbxl --bench salida.json
    if std::env::var_os("RUST_LAUNCHER_RUSTUDIO").is_some() {
        // SAFETY: todavía no hay más hilos. Su actualizador reemplazaría este ejecutable.
        unsafe { std::env::set_var("RUSTUDIO_NO_UPDATE_CHECK", "1") };
        return rustudio::run();
    }
    let arranque = std::time::Instant::now();
    perfil::iniciar();
    perfil::marca("main");
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some("--desinstalar") => {
            let _ = plataforma::registrar(false);
            let _ = std::fs::remove_dir_all(config::base());
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("RustLauncher {}", actualizar::VERSION_ACTUAL);
            return Ok(());
        }
        Some("--ayuda" | "--help" | "-h") => {
            println!("RustLauncher: ejecútalo sin argumentos para abrir el launcher.");
            println!("  --desinstalar   Quita el registro de enlaces y los datos del launcher");
            return Ok(());
        }
        _ => {}
    }
    let uri = arg.filter(|a| roblox::es_uri_roblox(a));

    // Runtime de tokio en hilos propios: la interfaz nunca espera a la red.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("runtime de tokio");
    let handle = rt.handle().clone();
    perfil::marca("runtime tokio listo");

    // Mientras el sistema crea la ventana (≈60 ms) ya se trabaja en red: se
    // crea el cliente HTTP, se abren las conexiones con los servidores de
    // imágenes (el saludo TLS ya estará hecho al pedir la primera) y se pide
    // la portada.
    let red = handle.spawn(async {
        let http = roblox::cliente_http().expect("cliente HTTP");
        for url in ["https://thumbnails.roblox.com/", "https://tr.rbxcdn.com/"] {
            let c = http.clone();
            tokio::spawn(async move {
                let _ = c.head(url).send().await;
            });
        }
        let c = http.clone();
        let portada = tokio::spawn(async move { api::secciones_inicio(&c, None).await });
        (http, portada)
    });

    // Solo en depuración: otro tamaño de ventana para capturas (RUST_LAUNCHER_VENTANA=1920x1080).
    let tamano = if cfg!(debug_assertions) {
        std::env::var("RUST_LAUNCHER_VENTANA")
            .ok()
            .and_then(|v| v.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?])))
    } else {
        None
    };
    let opciones = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustLauncher")
            .with_app_id("rust-launcher")
            .with_inner_size(tamano.unwrap_or([1280.0, 760.0]))
            .with_min_inner_size([860.0, 540.0]),
        // wgpu (no OpenGL): el editor de la pestaña Crear dibuja su escena 3D
        // con el mismo dispositivo que la interfaz, con sus límites y
        // la elección de tarjeta gráfica de rustudio.
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: opciones_wgpu(),
        ..Default::default()
    };
    eframe::run_native(
        "RustLauncher",
        opciones,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, handle, red, uri, arranque)))),
    )
}

/// Configuración de wgpu de rustudio. Para pruebas de rendimiento:
/// RUST_LAUNCHER_PRESENTAR = mailbox | fifo | inmediato y
/// RUST_LAUNCHER_LATENCIA = fotogramas en vuelo.
fn opciones_wgpu() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::wgpu::PresentMode;
    let mut o = rustudio::offscreen::wgpu_options(false);
    match std::env::var("RUST_LAUNCHER_PRESENTAR").as_deref() {
        Ok("mailbox") => o.surface.present_mode = PresentMode::Mailbox,
        Ok("fifo") => o.surface.present_mode = PresentMode::AutoVsync,
        Ok("inmediato") => o.surface.present_mode = PresentMode::AutoNoVsync,
        _ => {}
    }
    if let Some(n) = std::env::var("RUST_LAUNCHER_LATENCIA").ok().and_then(|v| v.parse().ok()) {
        o.surface.desired_maximum_frame_latency = Some(n);
    }
    o
}
