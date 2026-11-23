//! (RustLauncher) Which upscaler the view uses (Settings > Escalado del
//! editor): FSR 1 (spatial, finish.rs), FSR 2 (fsr2.rs) or FSR 3.1
//! (fsr3.rs), both temporal; and whether frames are generated (generar.rs).
//! For tests and benches: RUSTUDIO_ESCALADO=1|2|3 and RUSTUDIO_GENERAR=1.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modo {
    Fsr1,
    Fsr2,
    Fsr3,
}

/// 1, 2 or 3 (FSR 1, 2, 3.1).
static MODO: AtomicU8 = AtomicU8::new(3);
static GENERAR: AtomicBool = AtomicBool::new(false);

fn entorno() -> &'static (Option<u8>, bool) {
    static E: std::sync::OnceLock<(Option<u8>, bool)> = std::sync::OnceLock::new();
    E.get_or_init(|| {
        (
            std::env::var("RUSTUDIO_ESCALADO").ok().and_then(|v| v.parse().ok()),
            std::env::var_os("RUSTUDIO_GENERAR").is_some(),
        )
    })
}

pub fn set_modo(m: Modo) {
    MODO.store(
        match m {
            Modo::Fsr1 => 1,
            Modo::Fsr2 => 2,
            Modo::Fsr3 => 3,
        },
        Ordering::Relaxed,
    );
}

pub fn modo() -> Modo {
    match entorno().0.unwrap_or_else(|| MODO.load(Ordering::Relaxed)) {
        1 => Modo::Fsr1,
        2 => Modo::Fsr2,
        _ => Modo::Fsr3,
    }
}

/// Whether the temporal upscalers (FSR 2 and 3.1) are in use: the view then
/// draws with a jitter and hands its camera motion over.
pub fn temporal() -> bool {
    modo() != Modo::Fsr1
}

/// Frame generation (experimental, off by default).
pub fn set_generar(on: bool) {
    GENERAR.store(on, Ordering::Relaxed);
}

pub fn generar() -> bool {
    entorno().1 || GENERAR.load(Ordering::Relaxed)
}
