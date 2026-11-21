//! RustLauncher: launcher alternativo y rápido para Roblox.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod acelerar;
mod actualizar;
mod api;
mod app;
mod config;
mod flags;
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

fn main() -> eframe::Result<()> {
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

    let opciones = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustLauncher")
            .with_app_id("rust-launcher")
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([860.0, 540.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "RustLauncher",
        opciones,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, handle, red, uri, arranque)))),
    )
}
