//! Idioma de la interfaz: español o inglés.
//!
//! Los textos se escriben en español en el código y se pasan por [`tr`]
//! (o [`trf`] si llevan valores intercalados con `{}`). En inglés se buscan en
//! la tabla de `idioma/en.rs`; si falta alguno, se muestra el español.
//!
//! rustudio (pestaña Crear) está escrito en inglés: en español se traduce con
//! la tabla de `idioma/estudio_es.rs` a través del traductor de textos que
//! se añadió a su egui.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

mod en;
mod estudio_es;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Idioma {
    Es,
    En,
}

impl Idioma {
    pub const TODOS: [Idioma; 2] = [Idioma::Es, Idioma::En];

    pub fn nombre(self) -> &'static str {
        match self {
            Idioma::Es => "Español",
            Idioma::En => "English",
        }
    }
}

static ACTUAL: AtomicU8 = AtomicU8::new(0);

static INGLES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| en::TABLA.iter().copied().collect());
static ESTUDIO: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| estudio_es::TABLA.iter().copied().collect());

pub fn actual() -> Idioma {
    if ACTUAL.load(Ordering::Relaxed) == 1 { Idioma::En } else { Idioma::Es }
}

/// Idioma del sistema: español si la configuración regional empieza por
/// «es», inglés en cualquier otro caso.
pub fn del_sistema() -> Idioma {
    match sys_locale::get_locale() {
        Some(l) if l.to_ascii_lowercase().starts_with("es") => Idioma::Es,
        _ => Idioma::En,
    }
}

/// Fija el idioma del launcher y del editor de la pestaña Crear.
pub fn fijar(i: Idioma) {
    ACTUAL.store(if i == Idioma::En { 1 } else { 0 }, Ordering::Relaxed);
    para_launcher();
}

/// Los textos que se dibujen a partir de ahora son del launcher (español):
/// en inglés, egui los traduce al dibujarlos.
pub fn para_launcher() {
    eframe::egui::widget_text::set_text_translator(match actual() {
        Idioma::Es => None,
        Idioma::En => Some(launcher),
    });
}

/// Los textos que se dibujen a partir de ahora son de rustudio (inglés):
/// en español, egui los traduce al dibujarlos.
pub fn para_estudio() {
    eframe::egui::widget_text::set_text_translator(match actual() {
        Idioma::Es => Some(estudio),
        Idioma::En => None,
    });
}

fn launcher(texto: &str) -> Option<&'static str> {
    INGLES.get(texto).copied()
}

/// Traduce un texto de rustudio (inglés) al español.
fn estudio(texto: &str) -> Option<&'static str> {
    ESTUDIO.get(texto).copied()
}

/// Texto de la interfaz en el idioma actual.
pub fn tr(es: &'static str) -> &'static str {
    match actual() {
        Idioma::Es => es,
        Idioma::En => INGLES.get(es).copied().unwrap_or(es),
    }
}

/// Como [`tr`], para textos que no son `'static` (los que llegan ya
/// montados a las funciones de dibujo).
pub fn traducir(es: &str) -> &str {
    match actual() {
        Idioma::Es => es,
        Idioma::En => INGLES.get(es).copied().unwrap_or(es),
    }
}

/// Como [`tr`], sustituyendo cada `{}` por el siguiente valor.
pub fn trf(es: &'static str, valores: &[&dyn std::fmt::Display]) -> String {
    let plantilla = tr(es);
    let mut salida = String::with_capacity(plantilla.len() + 16);
    let mut valores = valores.iter();
    let mut resto = plantilla;
    while let Some(i) = resto.find("{}") {
        salida.push_str(&resto[..i]);
        if let Some(v) = valores.next() {
            salida.push_str(&v.to_string());
        }
        resto = &resto[i + 2..];
    }
    salida.push_str(resto);
    salida
}

/// Las pruebas que cambian o leen el idioma global no corren a la vez.
#[cfg(test)]
pub static PRUEBAS_IDIOMA: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduce_y_sustituye() {
        let _turno = PRUEBAS_IDIOMA.lock().unwrap_or_else(|e| e.into_inner());
        fijar(Idioma::Es);
        assert_eq!(trf("{} jugando ahora", &[&"5 mil"]), "5 mil jugando ahora");
        fijar(Idioma::En);
        assert_eq!(tr("Inicio"), "Home");
        assert_eq!(trf("{} jugando ahora", &[&"5 mil"]), "5 mil playing now");
        // Lo que no está en la tabla se queda en español.
        assert_eq!(tr("texto sin traducción xyz"), "texto sin traducción xyz");
        fijar(Idioma::Es);
    }

    #[test]
    fn las_plantillas_tienen_los_mismos_huecos() {
        for (es, en) in en::TABLA {
            assert_eq!(es.matches("{}").count(), en.matches("{}").count(), "«{es}» → «{en}»");
        }
    }

    #[test]
    fn sin_claves_repetidas() {
        let mut vistas = std::collections::HashSet::new();
        for (es, _) in en::TABLA {
            assert!(vistas.insert(es), "repetida: {es}");
        }
        let mut vistas = std::collections::HashSet::new();
        for (en, _) in estudio_es::TABLA {
            assert!(vistas.insert(en), "repetida en estudio: {en}");
        }
    }
}
