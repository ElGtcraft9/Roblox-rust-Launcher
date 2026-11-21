//! Lógica común: versión, manifiesto, descarga paralela verificada y extracción.
//!
//! Solo descarga y coloca los archivos oficiales de Roblox. Nunca modifica el
//! binario del cliente.
//!
//! La instalación del Player solo se usa en Windows; en Linux se compila para
//! los tests.
#![cfg_attr(not(windows), allow(dead_code))]

use anyhow::{Context, Result, bail};
use futures_util::{StreamExt, TryStreamExt, stream};
use md5::{Digest, Md5};
use serde::Deserialize;
use std::collections::HashSet;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::AsyncWriteExt;

const VERSION_URL: &str = "https://clientsettings.roblox.com/v2/client-version/WindowsPlayer";
const CDN: &str = "https://setup.rbxcdn.com";
/// Descargas simultáneas.
const PARALELO: usize = 8;
/// Marca que indica que una versión está completa y lista para lanzar.
const MARCA_COMPLETA: &str = ".completo";

const APP_SETTINGS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Settings>
	<ContentFolder>content</ContentFolder>
	<BaseUrl>http://www.roblox.com</BaseUrl>
</Settings>
"#;

/// Paquetes del manifiesto que no hacen falta para jugar.
const IGNORADOS: &[&str] = &["WebView2RuntimeInstaller.zip"];

#[derive(Debug, Clone)]
pub struct Paquete {
    pub nombre: String,
    pub md5: String,
    pub tam_comprimido: u64,
}

#[derive(Deserialize)]
struct RespuestaVersion {
    #[serde(rename = "clientVersionUpload")]
    client_version_upload: String,
}

pub fn cliente_http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("RustLauncher/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// Devuelve el hash de la versión actual del Player, p. ej. `version-2c6fd5e09aaf4cfa`.
pub async fn version_actual(cliente: &reqwest::Client) -> Result<String> {
    let r: RespuestaVersion = cliente
        .get(VERSION_URL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("respuesta de versión inválida")?;
    if !r.client_version_upload.starts_with("version-") {
        bail!("versión inesperada: {}", r.client_version_upload);
    }
    Ok(r.client_version_upload)
}

pub async fn manifiesto(cliente: &reqwest::Client, version: &str) -> Result<Vec<Paquete>> {
    let texto = cliente
        .get(format!("{CDN}/{version}-rbxPkgManifest.txt"))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parsear_manifiesto(&texto)
}

/// Formato: `v0` y luego bloques de 4 líneas (nombre, md5, tamaño comprimido, tamaño real).
pub fn parsear_manifiesto(texto: &str) -> Result<Vec<Paquete>> {
    let lineas: Vec<&str> = texto.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    match lineas.first() {
        Some(&"v0") => {}
        otro => bail!("formato de manifiesto desconocido: {otro:?}"),
    }
    let cuerpo = &lineas[1..];
    if !cuerpo.len().is_multiple_of(4) {
        bail!("manifiesto truncado");
    }
    cuerpo
        .chunks(4)
        .map(|c| {
            Ok(Paquete {
                nombre: c[0].to_string(),
                md5: c[1].to_lowercase(),
                tam_comprimido: c[2].parse().context("tamaño inválido")?,
            })
        })
        .collect()
}

/// Carpeta (relativa a la versión) donde se extrae cada paquete.
pub fn carpeta_destino(nombre: &str) -> Option<&'static str> {
    Some(match nombre {
        "RobloxApp.zip" | "redist.zip" | "WebView2.zip" => "",
        "shaders.zip" => "shaders",
        "ssl.zip" => "ssl",
        "content-avatar.zip" => "content/avatar",
        "content-configs.zip" => "content/configs",
        "content-fonts.zip" => "content/fonts",
        "content-sky.zip" => "content/sky",
        "content-sounds.zip" => "content/sounds",
        "content-textures2.zip" => "content/textures",
        "content-models.zip" => "content/models",
        "content-platform-fonts.zip" => "PlatformContent/pc/fonts",
        "content-platform-dictionaries.zip" => "PlatformContent/pc/shared_compression_dictionaries",
        "content-terrain.zip" => "PlatformContent/pc/terrain",
        "content-textures3.zip" => "PlatformContent/pc/textures",
        "extracontent-luapackages.zip" => "ExtraContent/LuaPackages",
        "extracontent-translations.zip" => "ExtraContent/translations",
        "extracontent-models.zip" => "ExtraContent/models",
        "extracontent-textures.zip" => "ExtraContent/textures",
        "extracontent-places.zip" => "ExtraContent/places",
        _ => return None,
    })
}

/// Paquetes que hay que instalar: solo `.zip`, sin los ignorados.
pub fn paquetes_necesarios(todos: Vec<Paquete>) -> Vec<Paquete> {
    todos
        .into_iter()
        .filter(|p| p.nombre.ends_with(".zip") && !IGNORADOS.contains(&p.nombre.as_str()))
        .collect()
}

pub fn version_completa(dir: &Path) -> bool {
    dir.join(MARCA_COMPLETA).exists()
}

/// Asegura que `version` está instalada en `raiz_versiones/version`.
/// Reutiliza paquetes ya descargados (caché por md5) y descarga el resto en paralelo.
pub async fn instalar(
    cliente: &reqwest::Client,
    version: &str,
    raiz_versiones: &Path,
    cache: &Path,
    progreso: &(dyn Fn(usize, usize) + Sync),
) -> Result<PathBuf> {
    let destino = raiz_versiones.join(version);
    if version_completa(&destino) {
        return Ok(destino);
    }
    // Una sola instalación a la vez (p. ej. actualización en segundo plano y
    // clic en Jugar al mismo tiempo).
    static CANDADO: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guardia = CANDADO.lock().await;
    if version_completa(&destino) {
        return Ok(destino);
    }

    let paquetes = paquetes_necesarios(manifiesto(cliente, version).await?);

    let temporal = raiz_versiones.join(format!("{version}.tmp"));
    if temporal.exists() {
        tokio::fs::remove_dir_all(&temporal).await?;
    }
    tokio::fs::create_dir_all(&temporal).await?;
    tokio::fs::create_dir_all(cache).await?;

    let total = paquetes.len();
    let hechos = Arc::new(AtomicUsize::new(0));
    progreso(0, total);

    stream::iter(paquetes.iter().cloned())
        .map(|p| {
            let cliente = cliente.clone();
            let cache = cache.to_path_buf();
            let temporal = temporal.clone();
            let hechos = hechos.clone();
            async move {
                let (zip, _) = descargar_con_cache(&cliente, version, &p, &cache).await?;
                // Paquete desconocido: se extrae en la raíz.
                let sub = carpeta_destino(&p.nombre).unwrap_or("");
                let dir = temporal.join(sub);
                tokio::task::spawn_blocking(move || extraer(&zip, &dir)).await??;
                let n = hechos.fetch_add(1, Ordering::Relaxed) + 1;
                progreso(n, total);
                anyhow::Ok(())
            }
        })
        .buffer_unordered(PARALELO)
        .try_collect::<()>()
        .await?;

    tokio::fs::write(temporal.join("AppSettings.xml"), APP_SETTINGS).await?;
    tokio::fs::write(temporal.join(MARCA_COMPLETA), version).await?;
    if destino.exists() {
        tokio::fs::remove_dir_all(&destino).await?;
    }
    tokio::fs::rename(&temporal, &destino).await?;

    limpiar_cache(cache, &paquetes).await;
    Ok(destino)
}

/// Devuelve la ruta del zip verificado y si venía de la caché.
async fn descargar_con_cache(
    cliente: &reqwest::Client,
    version: &str,
    p: &Paquete,
    cache: &Path,
) -> Result<(PathBuf, bool)> {
    let ruta = cache.join(format!("{}.zip", p.md5));
    // Los archivos de la caché solo llegan ahí tras verificar su md5, así que
    // basta con comprobar el tamaño.
    if let Ok(meta) = tokio::fs::metadata(&ruta).await
        && meta.len() == p.tam_comprimido
    {
        return Ok((ruta, true));
    }

    let parcial = cache.join(format!("{}.part", p.md5));
    let url = format!("{CDN}/{version}-{}", p.nombre);
    let mut resp = cliente
        .get(&url)
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("descargando {}", p.nombre))?;

    let mut archivo = tokio::io::BufWriter::new(tokio::fs::File::create(&parcial).await?);
    let mut hasher = Md5::new();
    while let Some(trozo) = resp.chunk().await? {
        hasher.update(&trozo);
        archivo.write_all(&trozo).await?;
    }
    archivo.flush().await?;
    drop(archivo);

    let obtenido = hex(&hasher.finalize());
    if obtenido != p.md5 {
        let _ = tokio::fs::remove_file(&parcial).await;
        bail!("{}: md5 no coincide (esperado {}, obtenido {obtenido})", p.nombre, p.md5);
    }
    tokio::fs::rename(&parcial, &ruta).await?;
    Ok((ruta, false))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Convierte una ruta de zip (puede usar `\`) en una ruta relativa segura.
fn ruta_segura(nombre: &str) -> Option<PathBuf> {
    let mut ruta = PathBuf::new();
    for parte in nombre.split(['/', '\\']) {
        match parte {
            "" | "." => {}
            ".." => return None,
            p if p.contains(':') => return None,
            p => ruta.push(p),
        }
    }
    (!ruta.as_os_str().is_empty()).then_some(ruta)
}

fn extraer(zip: &Path, destino: &Path) -> Result<()> {
    let archivo = std::fs::File::open(zip)?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(archivo))
        .with_context(|| format!("zip inválido: {}", zip.display()))?;
    std::fs::create_dir_all(destino)?;
    for i in 0..zip.len() {
        let mut entrada = zip.by_index(i)?;
        let nombre = entrada.name()?.into_owned();
        let Some(rel) = ruta_segura(&nombre) else {
            continue;
        };
        let ruta = destino.join(rel);
        if nombre.ends_with('/') || nombre.ends_with('\\') {
            std::fs::create_dir_all(&ruta)?;
            continue;
        }
        if let Some(padre) = ruta.parent() {
            std::fs::create_dir_all(padre)?;
        }
        let mut salida = BufWriter::new(std::fs::File::create(&ruta)?);
        std::io::copy(&mut entrada, &mut salida)?;
        salida.flush()?;
    }
    Ok(())
}

/// Borra de la caché los paquetes que ya no forman parte de la versión actual.
async fn limpiar_cache(cache: &Path, actuales: &[Paquete]) {
    let validos: HashSet<String> = actuales.iter().map(|p| format!("{}.zip", p.md5)).collect();
    let Ok(mut dir) = tokio::fs::read_dir(cache).await else {
        return;
    };
    while let Ok(Some(e)) = dir.next_entry().await {
        let nombre = e.file_name().to_string_lossy().into_owned();
        if !validos.contains(&nombre) {
            let _ = tokio::fs::remove_file(e.path()).await;
        }
    }
}

/// Borra versiones antiguas. Ignora las que estén en uso.
pub fn limpiar_versiones(raiz: &Path, actual: &str) {
    let Ok(dir) = std::fs::read_dir(raiz) else {
        return;
    };
    for e in dir.flatten() {
        if e.file_name() != actual && e.path().is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Indica si el argumento es una URI de Roblox.
pub fn es_uri_roblox(arg: &str) -> bool {
    let a = arg.to_ascii_lowercase();
    a.starts_with("roblox-player:") || a.starts_with("roblox:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn parsea_manifiesto() {
        let m = "v0\nRobloxApp.zip\nABC\n10\n20\nssl.zip\ndef\n1\n2\n";
        let p = parsear_manifiesto(m).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].md5, "abc");
        assert_eq!(p[1].tam_comprimido, 1);
    }

    #[test]
    fn rechaza_rutas_peligrosas() {
        assert_eq!(ruta_segura(r"a\b.txt"), Some(PathBuf::from("a").join("b.txt")));
        assert!(ruta_segura("../x").is_none());
        assert!(ruta_segura(r"C:\x").is_none());
    }

    #[test]
    fn detecta_uri() {
        assert!(es_uri_roblox("roblox-player:1+launchmode:play"));
        assert!(es_uri_roblox("roblox://placeId=1"));
        assert!(!es_uri_roblox("--algo"));
    }

    /// Prueba real contra la CDN: instala la versión actual completa en un
    /// directorio temporal y la reinstala desde la caché.
    /// `cargo test -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn instalacion_real() {
        let c = cliente_http().unwrap();
        let v = version_actual(&c).await.unwrap();
        let paquetes = paquetes_necesarios(manifiesto(&c, &v).await.unwrap());
        for p in &paquetes {
            assert!(carpeta_destino(&p.nombre).is_some(), "sin mapa: {}", p.nombre);
        }
        let tmp = std::env::temp_dir().join("rust-launcher-test");
        let _ = std::fs::remove_dir_all(&tmp);
        let (versiones, cache) = (tmp.join("Versions"), tmp.join("Downloads"));

        let t = Instant::now();
        let dir = instalar(&c, &v, &versiones, &cache, &|_, _| {}).await.unwrap();
        println!("instalación completa: {:.1}s", t.elapsed().as_secs_f32());
        assert!(dir.join("RobloxPlayerBeta.exe").exists());
        assert!(dir.join("AppSettings.xml").exists());
        assert!(version_completa(&dir));

        // Reinstalación: todo debe salir de la caché.
        std::fs::remove_dir_all(&dir).unwrap();
        let t = Instant::now();
        instalar(&c, &v, &versiones, &cache, &|_, _| {}).await.unwrap();
        println!("desde caché: {:.1}s", t.elapsed().as_secs_f32());
        assert!(dir.join("RobloxPlayerBeta.exe").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
