//! Ajustes del cliente mediante Fast Flags.
//!
//! Solo se usan flags de la lista oficial de Roblox ("Allowlist for local client
//! configuration via Fast Flags"). El resto el cliente las ignora.

use crate::config::{AjustesCliente, ApiGrafica};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Valor {
    Bool(bool),
    Int(i64),
}

/// Todas las flags que gestiona el launcher (para poder quitarlas al desactivar).
pub const GESTIONADAS: &[&str] = &[
    "FFlagDebugGraphicsPreferD3D11",
    "FFlagDebugGraphicsPreferVulkan",
    "FFlagDebugGraphicsPreferOpenGL",
    "FIntDebugForceMSAASamples",
    "DFFlagTextureQualityOverrideEnabled",
    "DFIntTextureQualityOverride",
    "FIntFRMMinGrassDistance",
    "FIntFRMMaxGrassDistance",
    "DFIntCSGLevelOfDetailSwitchingDistance",
    "DFIntCSGLevelOfDetailSwitchingDistanceL12",
    "DFIntCSGLevelOfDetailSwitchingDistanceL23",
    "DFIntCSGLevelOfDetailSwitchingDistanceL34",
    "FFlagDebugSkyGray",
    "DFFlagDisableDPIScale",
];

pub fn flags(a: &AjustesCliente) -> Vec<(&'static str, Valor)> {
    let mut f = Vec::new();
    match a.api {
        ApiGrafica::Auto => {}
        ApiGrafica::D3D11 => f.push(("FFlagDebugGraphicsPreferD3D11", Valor::Bool(true))),
        ApiGrafica::Vulkan => f.push(("FFlagDebugGraphicsPreferVulkan", Valor::Bool(true))),
        ApiGrafica::OpenGL => f.push(("FFlagDebugGraphicsPreferOpenGL", Valor::Bool(true))),
    }
    if let Some(m) = a.msaa {
        f.push(("FIntDebugForceMSAASamples", Valor::Int(m.into())));
    }
    if let Some(t) = a.texturas {
        f.push(("DFFlagTextureQualityOverrideEnabled", Valor::Bool(true)));
        f.push(("DFIntTextureQualityOverride", Valor::Int(t.into())));
    }
    if a.sin_hierba {
        f.push(("FIntFRMMinGrassDistance", Valor::Int(0)));
        f.push(("FIntFRMMaxGrassDistance", Valor::Int(0)));
    }
    if a.geometria_reducida {
        for k in &GESTIONADAS[8..12] {
            f.push((k, Valor::Int(0)));
        }
    }
    if a.cielo_gris {
        f.push(("FFlagDebugSkyGray", Valor::Bool(true)));
    }
    if a.sin_escalado_dpi {
        f.push(("DFFlagDisableDPIScale", Valor::Bool(true)));
    }
    f
}

/// Formato de ClientAppSettings.json (Windows): valores como texto.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn json_windows(a: &AjustesCliente) -> String {
    let mapa: serde_json::Map<String, serde_json::Value> = flags(a)
        .into_iter()
        .map(|(k, v)| {
            let s = match v {
                Valor::Bool(b) => if b { "True" } else { "False" }.to_string(),
                Valor::Int(i) => i.to_string(),
            };
            (k.to_string(), serde_json::Value::String(s))
        })
        .collect();
    serde_json::to_string_pretty(&mapa).unwrap_or_else(|_| "{}".into())
}

/// Fusiona las flags en el objeto `fflags` de Sober sin tocar las que no son nuestras.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn fusionar_sober(fflags: &mut serde_json::Map<String, serde_json::Value>, a: &AjustesCliente) {
    for k in GESTIONADAS {
        fflags.remove(*k);
    }
    for (k, v) in flags(a) {
        let v = match v {
            Valor::Bool(b) => serde_json::Value::Bool(b),
            Valor::Int(i) => serde_json::Value::from(i),
        };
        fflags.insert(k.to_string(), v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_ajustes_no_hay_flags() {
        assert!(flags(&AjustesCliente::default()).is_empty());
    }

    #[test]
    fn todas_las_flags_estan_gestionadas() {
        let a = AjustesCliente {
            api: ApiGrafica::Vulkan,
            msaa: Some(4),
            texturas: Some(0),
            sin_hierba: true,
            geometria_reducida: true,
            cielo_gris: true,
            sin_escalado_dpi: true,
        };
        for (k, _) in flags(&a) {
            assert!(GESTIONADAS.contains(&k), "{k}");
        }
        assert!(json_windows(&a).contains("\"FIntDebugForceMSAASamples\": \"4\""));
    }

    #[test]
    fn fusion_respeta_flags_ajenas() {
        let mut m = serde_json::Map::new();
        m.insert("FFlagExample".into(), true.into());
        m.insert("FFlagDebugSkyGray".into(), true.into());
        fusionar_sober(&mut m, &AjustesCliente::default());
        assert!(m.contains_key("FFlagExample"));
        assert!(!m.contains_key("FFlagDebugSkyGray"));
    }
}
