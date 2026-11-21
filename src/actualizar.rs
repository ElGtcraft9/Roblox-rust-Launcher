//! Actualizaciones automáticas del launcher desde GitHub Releases.
//!
//! Cada release lleva el ejecutable de cada plataforma y su firma `minisign`
//! (`<ejecutable>.minisig`). El launcher solo se reemplaza si:
//! 1. la release es de una versión mayor que la actual;
//! 2. la firma es válida para la clave pública incluida aquí (la privada solo
//!    la tiene quien publica, fuera del repositorio);
//! 3. el comentario firmado nombra esa misma versión y plataforma, para que
//!    nadie pueda hacer pasar un ejecutable antiguo (firmado) por uno nuevo.

use anyhow::{Context, Result, bail};
use minisign_verify::{PublicKey, Signature};
use serde_json::Value;

pub const REPOSITORIO: &str = "iker11094-cmyk/Roblox-rust-Launcher";

/// Clave pública de firma (`herramientas/minisign.pub`).
const CLAVE_PUBLICA: &str = "RWT4XCk0ZhR7fDVZ0FmaixOKiHwyUAdqzRWcrm5+Pd9lGtyogm84s1UT";

pub const VERSION_ACTUAL: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct Version {
    pub numero: String,
    url: String,
    url_firma: String,
}

/// Nombre del ejecutable de esta plataforma en las releases.
pub fn nombre_activo() -> &'static str {
    if cfg!(windows) { "RustLauncher-windows.exe" } else { "RustLauncher-linux" }
}

fn plataforma() -> &'static str {
    if cfg!(windows) { "windows" } else { "linux" }
}

/// «1.2.3» → (1, 2, 3). Acepta una «v» delante.
fn partes(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

fn es_mas_nueva(candidata: &str, actual: &str) -> bool {
    matches!((partes(candidata), partes(actual)), (Some(c), Some(a)) if c > a)
}

/// Comentario que debe llevar la firma de cada ejecutable.
pub fn comentario_firmado(version: &str, plataforma: &str) -> String {
    format!("rust-launcher {version} {plataforma}")
}

/// Consulta la última release. Devuelve `Some` solo si es más nueva y trae
/// el ejecutable y la firma de esta plataforma.
pub async fn buscar(http: &reqwest::Client) -> Result<Option<Version>> {
    let v: Value = http
        .get(format!("https://api.github.com/repos/{REPOSITORIO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()
        .context("GitHub no respondió a la consulta de versiones")?
        .json()
        .await?;
    let etiqueta = v["tag_name"].as_str().context("release sin etiqueta")?;
    let numero = etiqueta.trim_start_matches('v').to_string();
    if !es_mas_nueva(&numero, VERSION_ACTUAL) {
        return Ok(None);
    }
    let url_de = |nombre: &str| {
        v["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"] == nombre)
            .and_then(|a| a["browser_download_url"].as_str())
            .map(str::to_string)
    };
    let activo = nombre_activo();
    match (url_de(activo), url_de(&format!("{activo}.minisig"))) {
        (Some(url), Some(url_firma)) => Ok(Some(Version { numero, url, url_firma })),
        _ => Ok(None),
    }
}

/// Comprueba que `binario` está firmado con nuestra clave para esa versión.
pub fn verificar(binario: &[u8], firma: &str, version: &str, plataforma: &str) -> Result<()> {
    let clave = PublicKey::from_base64(CLAVE_PUBLICA).context("clave pública inválida")?;
    let firma = Signature::decode(firma).context("firma con formato inválido")?;
    clave.verify(binario, &firma, false).context("la firma no corresponde a este archivo")?;
    let esperado = comentario_firmado(version, plataforma);
    if firma.trusted_comment().trim() != esperado {
        bail!("la firma es de otra versión: «{}» (se esperaba «{esperado}»)", firma.trusted_comment());
    }
    Ok(())
}

/// Descarga, verifica e instala la versión: el ejecutable en uso se reemplaza
/// en el disco y la versión nueva se usa al reiniciar.
pub async fn instalar(http: &reqwest::Client, v: &Version) -> Result<()> {
    let (binario, firma) = tokio::try_join!(
        async { Ok::<_, anyhow::Error>(http.get(&v.url).send().await?.error_for_status()?.bytes().await?) },
        async { Ok::<_, anyhow::Error>(http.get(&v.url_firma).send().await?.error_for_status()?.text().await?) },
    )
    .context("no se pudo descargar la actualización")?;
    verificar(&binario, &firma, &v.numero, plataforma())?;

    let actual = std::env::current_exe()?;
    let nuevo = actual.with_extension("actualizacion");
    tokio::fs::write(&nuevo, &binario).await?;
    let r = tokio::task::spawn_blocking({
        let nuevo = nuevo.clone();
        move || self_replace::self_replace(&nuevo)
    })
    .await?;
    let _ = tokio::fs::remove_file(&nuevo).await;
    r.context("no se pudo reemplazar el ejecutable (¿permisos de la carpeta?)")
}

/// Abre de nuevo el launcher (ya actualizado) y cierra este proceso.
pub fn reiniciar() -> ! {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compara_versiones() {
        assert!(es_mas_nueva("0.3.0", "0.2.9"));
        assert!(es_mas_nueva("v1.0.0", "0.9.9"));
        assert!(es_mas_nueva("0.2.10", "0.2.9"));
        assert!(!es_mas_nueva("0.2.0", "0.2.0"));
        assert!(!es_mas_nueva("0.1.9", "0.2.0"));
        assert!(!es_mas_nueva("basura", "0.2.0"));
    }

    #[test]
    fn rechaza_firmas_falsas() {
        let firma_mala = "untrusted comment: x\nRWQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\ntrusted comment: rust-launcher 9.9.9 linux\nAAAA\n";
        assert!(verificar(b"hola", firma_mala, "9.9.9", "linux").is_err());
    }

    /// Firma real con la clave privada (si está en este equipo): la buena
    /// pasa y una versión o un contenido distintos fallan.
    /// `cargo test -- --ignored firma_real`
    #[test]
    #[ignore]
    fn firma_real() {
        let dir = std::env::temp_dir().join("rust-launcher-firma");
        let _ = std::fs::create_dir_all(&dir);
        let archivo = dir.join("binario");
        std::fs::write(&archivo, b"contenido de prueba").unwrap();
        let clave = dirs::home_dir().unwrap().join(".config/rustlauncher-firma/minisign.key");
        let ok = std::process::Command::new("minisign")
            .args(["-S", "-s"])
            .arg(&clave)
            .arg("-m")
            .arg(&archivo)
            .args(["-t", &comentario_firmado("1.2.3", "linux")])
            .status()
            .unwrap();
        assert!(ok.success());
        let firma = std::fs::read_to_string(dir.join("binario.minisig")).unwrap();
        assert!(verificar(b"contenido de prueba", &firma, "1.2.3", "linux").is_ok());
        assert!(verificar(b"contenido de prueba", &firma, "1.2.4", "linux").is_err());
        assert!(verificar(b"contenido de prueba", &firma, "1.2.3", "windows").is_err());
        assert!(verificar(b"otro contenido", &firma, "1.2.3", "linux").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
