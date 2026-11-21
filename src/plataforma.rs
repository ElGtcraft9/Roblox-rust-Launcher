//! Interfaz común entre plataformas.

use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Progreso {
    Comprobando,
    Descargando { hechos: usize, total: usize },
    Iniciando,
    /// Había otra partida abierta: el cliente solo admite una a la vez.
    CerrandoAnterior,
}

pub type ProgresoFn = Arc<dyn Fn(Progreso) + Send + Sync>;

#[cfg(target_os = "linux")]
pub use crate::linux::*;
#[cfg(windows)]
pub use crate::windows::*;
