//! Medición de tiempos (solo con la variable RUST_LAUNCHER_PERFIL=<segundos>).
//!
//! Imprime cuándo ocurre cada fase desde que arranca el proceso y, al final,
//! cuánto cuesta dibujar cada fotograma. Sin la variable no hace nada.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static INICIO: OnceLock<Instant> = OnceLock::new();
static ACTIVO: OnceLock<bool> = OnceLock::new();
static FOTOGRAMAS: AtomicU64 = AtomicU64::new(0);
static TOTAL_US: AtomicU64 = AtomicU64::new(0);
static MAX_US: AtomicU64 = AtomicU64::new(0);

pub fn iniciar() {
    INICIO.get_or_init(Instant::now);
    ACTIVO.get_or_init(|| std::env::var_os("RUST_LAUNCHER_PERFIL").is_some());
}

pub fn activo() -> bool {
    *ACTIVO.get().unwrap_or(&false)
}

pub fn ms() -> f64 {
    INICIO.get().map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0)
}

pub fn marca(que: &str) {
    if activo() {
        eprintln!("[perfil] {:>8.1} ms  {que}", ms());
    }
}

pub fn fotograma(us: u64) {
    if activo() {
        FOTOGRAMAS.fetch_add(1, Ordering::Relaxed);
        TOTAL_US.fetch_add(us, Ordering::Relaxed);
        MAX_US.fetch_max(us, Ordering::Relaxed);
    }
}

/// Segundos que debe durar la medición antes de salir.
pub fn duracion() -> Option<f64> {
    std::env::var("RUST_LAUNCHER_PERFIL").ok().and_then(|s| s.parse().ok())
}

pub fn resumen() {
    let n = FOTOGRAMAS.load(Ordering::Relaxed).max(1);
    eprintln!(
        "[perfil] fotogramas: {}  media {:.2} ms  máximo {:.2} ms",
        n,
        TOTAL_US.load(Ordering::Relaxed) as f64 / n as f64 / 1000.0,
        MAX_US.load(Ordering::Relaxed) as f64 / 1000.0
    );
}
