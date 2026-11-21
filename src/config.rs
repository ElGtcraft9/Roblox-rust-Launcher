//! Configuración persistente del launcher y ajustes del cliente de Roblox.

use crate::api::{Juego, Seccion, Usuario};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn base() -> PathBuf {
    // Solo en compilaciones de depuración: carpeta de datos alternativa para pruebas.
    #[cfg(debug_assertions)]
    if let Some(d) = std::env::var_os("RUST_LAUNCHER_DATOS") {
        return PathBuf::from(d);
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("RustLauncher")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApiGrafica {
    #[default]
    Auto,
    D3D11,
    Vulkan,
    OpenGL,
}

/// Modo de la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Modo {
    /// Ventana, ratón y rueda.
    #[default]
    Pc,
    /// Pantalla completa, mando y texto grande.
    Consola,
}

/// Qué hace el launcher cuando se abre un juego.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AlJugar {
    /// Se minimiza y vuelve al cerrar el juego.
    Minimizar,
    Cerrar,
    /// El launcher se queda donde está y Roblox se abre encima.
    #[default]
    Nada,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AjustesCliente {
    pub api: ApiGrafica,
    /// Muestras de antialiasing; `None` = lo decide Roblox.
    pub msaa: Option<u8>,
    /// Calidad de texturas 0-3; `None` = lo decide Roblox.
    pub texturas: Option<u8>,
    pub sin_hierba: bool,
    pub geometria_reducida: bool,
    pub cielo_gris: bool,
    pub sin_escalado_dpi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub usuario: Option<Usuario>,
    pub favoritos: Vec<Juego>,
    pub recientes: Vec<Juego>,
    pub cliente: AjustesCliente,
    /// `None` hasta la primera ejecución, en la que se decide según la plataforma.
    pub abrir_enlaces_web: Option<bool>,
    pub actualizar_en_segundo_plano: bool,
    pub al_jugar: AlJugar,
    /// Precargar los archivos del cliente para que arranque antes.
    pub precargar: bool,
    pub modo: Modo,
    /// Versión del formato, para migrar valores por defecto que cambian.
    /// Si falta (configuraciones antiguas) vale 0.
    #[serde(default)]
    pub version: u32,
    /// Cerrar Roblox al salir de una experiencia y volver al launcher.
    pub cerrar_al_salir: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            usuario: None,
            favoritos: Vec::new(),
            recientes: Vec::new(),
            cliente: AjustesCliente::default(),
            abrir_enlaces_web: None,
            actualizar_en_segundo_plano: true,
            al_jugar: AlJugar::Nada,
            precargar: true,
            modo: Modo::Pc,
            version: VERSION_CONFIG,
            cerrar_al_salir: true,
        }
    }
}

pub const VERSION_CONFIG: u32 = 4;

fn ruta() -> PathBuf {
    base().join("config.json")
}

impl Config {
    pub fn cargar() -> Self {
        let mut c: Config = std::fs::read(ruta())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if c.version < 4 {
            // Antes el launcher se minimizaba al jugar; ahora Roblox se abre encima.
            c.al_jugar = AlJugar::Nada;
            c.version = VERSION_CONFIG;
            c.guardar();
        }
        c
    }

    pub fn guardar(&self) {
        let _ = std::fs::create_dir_all(base());
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            // Escritura atómica: nunca deja un archivo a medias.
            let tmp = ruta().with_extension("tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(tmp, ruta());
            }
        }
    }

    pub fn es_favorito(&self, universe_id: u64) -> bool {
        self.favoritos.iter().any(|j| j.universe_id == universe_id)
    }

    pub fn alternar_favorito(&mut self, juego: Juego) {
        if self.es_favorito(juego.universe_id) {
            self.favoritos.retain(|j| j.universe_id != juego.universe_id);
        } else {
            self.favoritos.insert(0, juego);
        }
        self.guardar();
    }

    pub fn anotar_reciente(&mut self, juego: Juego) {
        self.recientes.retain(|j| j.universe_id != juego.universe_id);
        self.recientes.insert(0, juego);
        self.recientes.truncate(12);
        self.guardar();
    }
}

/// Última portada descargada: se muestra al instante en el siguiente arranque
/// mientras se refresca en segundo plano.
pub fn cargar_inicio() -> Vec<Seccion> {
    std::fs::read(base().join("inicio.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn guardar_inicio(secciones: &[Seccion]) {
    let _ = std::fs::create_dir_all(base());
    if let Ok(json) = serde_json::to_vec(secciones) {
        let _ = std::fs::write(base().join("inicio.json"), json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migra_configuraciones_antiguas() {
        let vieja: Config = serde_json::from_str(r#"{"al_jugar":"Minimizar"}"#).unwrap();
        assert_eq!(vieja.version, 0);
        assert_eq!(vieja.al_jugar, AlJugar::Minimizar);
        assert_eq!(Config::default().version, VERSION_CONFIG);
    }
}
