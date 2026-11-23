//! Linux: Roblox no tiene cliente nativo y bloquea Wine, así que se lanza a
//! través de Sober (flatpak org.vinegarhq.Sober).

use crate::config::{AjustesCliente, ApiGrafica, base};
use crate::flags;
use crate::plataforma::{Progreso, ProgresoFn};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const SOBER: &str = "org.vinegarhq.Sober";
const DESKTOP: &str = "rust-launcher.desktop";
const MIMES: &[&str] = &["x-scheme-handler/roblox-player", "x-scheme-handler/roblox"];

pub const NOMBRE_CLIENTE: &str = "Sober";
/// Sober solo distingue entre su renderizador por defecto y OpenGL.
pub const APIS_GRAFICAS: bool = false;
pub const ACTUALIZA_CLIENTE: bool = false;
/// En Linux no se quita a Sober el control de los enlaces sin preguntar.
pub const ENLACES_POR_DEFECTO: bool = false;
/// Escalado con gamescope, SMAA con vkBasalt, límite de FPS y MAKO.
pub const EFECTOS_SOBER: bool = true;

/// Capas Vulkan que se instalan como extensiones de Flatpak.
const EXT_VKBASALT: &str = "org.freedesktop.Platform.VulkanLayer.vkBasalt";
const EXT_MAKO: &str = "org.freedesktop.Platform.VulkanLayer.makorender";

/// MAKO (github.com/eugeniosegala/MAKO, GPL-3.0): versión fija y su huella
/// SHA-256 (la que publica GitHub para el archivo), para no instalar nunca
/// algo distinto de lo revisado.
const MAKO_URL: &str =
    "https://github.com/eugeniosegala/MAKO/releases/download/render-v5.0.0/MAKO-Renderer-v5.0.0-flatpaks.tar.xz";
const MAKO_SHA256: &str = "683037a3643fae40b02e7d2ce2ba443c1b2c70191707aa284fe46a9adb009ff5";
const MAKO_RAMAS: [&str; 4] = ["23.08", "24.08", "25.08", "26.08"];

fn ruta_desktop() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("applications")
        .join(DESKTOP)
}

fn sober_instalado() -> bool {
    Command::new("flatpak")
        .args(["info", SOBER])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub fn estado_cliente() -> Option<String> {
    (!sober_instalado()).then(|| format!("Sober no está instalado. Instálalo con: flatpak install flathub {SOBER}"))
}

pub fn protocolos_registrados() -> bool {
    Command::new("xdg-mime")
        .args(["query", "default", MIMES[0]])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == DESKTOP)
}

pub fn registrar(activar: bool) -> Result<()> {
    if !activar {
        let _ = std::fs::remove_file(ruta_desktop());
        for mime in MIMES {
            let _ = Command::new("xdg-mime")
                .args(["default", &format!("{SOBER}.desktop"), mime])
                .status();
        }
        return Ok(());
    }
    let destino = base().join("rust-launcher");
    std::fs::create_dir_all(base())?;
    let actual = std::env::current_exe()?;
    if actual.canonicalize().ok() != destino.canonicalize().ok() {
        let tmp = destino.with_extension("nuevo");
        std::fs::copy(&actual, &tmp)?;
        std::fs::rename(&tmp, &destino)?;
    }
    let desktop = ruta_desktop();
    std::fs::create_dir_all(desktop.parent().context("ruta inválida")?)?;
    std::fs::write(
        &desktop,
        format!(
            "[Desktop Entry]\nType=Application\nName=RustLauncher\nExec=\"{}\" %u\n\
             MimeType={};\nNoDisplay=true\nTerminal=false\n",
            destino.display(),
            MIMES.join(";")
        ),
    )?;
    for mime in MIMES {
        Command::new("xdg-mime").args(["default", DESKTOP, mime]).status()?;
    }
    Ok(())
}

pub async fn actualizar(_progreso: ProgresoFn) -> Result<PathBuf> {
    // Sober se actualiza por su cuenta.
    Ok(PathBuf::new())
}

fn ruta_config_sober() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".var/app/org.vinegarhq.Sober/config/sober/config.json"))
}

/// Aplica los ajustes al config.json de Sober, conservando su cabecera de
/// comentarios y todo lo que no gestiona el launcher.
fn aplicar_ajustes_sober(ajustes: &AjustesCliente) -> Result<()> {
    let Some(ruta) = ruta_config_sober() else { return Ok(()) };
    let Ok(texto) = std::fs::read_to_string(&ruta) else {
        // Sober lo crea en su primer arranque.
        return Ok(());
    };
    let (cabecera, cuerpo): (Vec<&str>, Vec<&str>) =
        texto.lines().partition(|l| l.trim_start().starts_with("//"));
    let mut json: serde_json::Value =
        serde_json::from_str(&cuerpo.join("\n")).context("config.json de Sober no es válido")?;
    let original = json.clone();
    let obj = json.as_object_mut().context("config.json de Sober no es un objeto")?;
    obj.insert("use_opengl".into(), (ajustes.api == ApiGrafica::OpenGL).into());
    obj.insert("graphics_optimization_mode".into(), ajustes.optimizacion.clave().into());
    let fflags = obj
        .entry("fflags")
        .or_insert_with(|| serde_json::Value::Object(Default::default()));
    if let Some(m) = fflags.as_object_mut() {
        flags::fusionar_sober(m, ajustes);
    }
    if json != original {
        let mut nuevo = cabecera.join("\n");
        if !nuevo.is_empty() {
            nuevo.push('\n');
        }
        nuevo.push_str(&serde_json::to_string_pretty(&json)?);
        nuevo.push('\n');
        let tmp = ruta.with_extension("json.tmp");
        std::fs::write(&tmp, nuevo)?;
        std::fs::rename(tmp, ruta)?;
    }
    Ok(())
}

/// Archivos de Sober y de Roblox que conviene tener en la caché del sistema.
pub fn rutas_precarga() -> crate::acelerar::Precarga {
    let mut completas = vec![PathBuf::from("/var/lib/flatpak/app/org.vinegarhq.Sober/current/active/files")];
    let mut recientes = Vec::new();
    if let Some(h) = dirs::home_dir() {
        let sober = h.join(".var/app/org.vinegarhq.Sober");
        let datos = sober.join("data/sober");
        completas.extend([
            datos.join("packages"),
            datos.join("assets"),
            // Base de datos de recursos y cachés de sombreadores de la GPU.
            datos.join("appData/rbx-storage.db"),
            sober.join("cache/sober/shadercachevk.bin"),
            sober.join("cache/mesa_shader_cache"),
            sober.join("cache/radv_builtin_shaders"),
        ]);
        // Recursos de juegos (mapas, texturas): ocupa gigas, solo lo reciente.
        recientes.push(sober.join("cache/sober/rbx-storage"));
    }
    crate::acelerar::Precarga { completas, recientes }
}

/// Devuelve el proceso de `flatpak run`, que vive mientras Sober esté abierto.
/// Carpeta de registros del cliente y el texto que identifica su archivo.
pub fn registros() -> (PathBuf, &'static str) {
    let dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".var/app/org.vinegarhq.Sober/data/sober/sober_logs");
    (dir, "latest")
}

/// Si Sober está abierto (cuesta ~10 ms comprobarlo).
fn sober_abierto() -> bool {
    Command::new("flatpak")
        .args(["ps", "--columns=application"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == SOBER))
}

/// Proceso principal de Sober (el padre del proceso «Main» de Roblox).
fn pid_sober() -> Option<i32> {
    let leer = |pid: &str| -> Option<(String, i32)> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // Formato: pid (comm) estado ppid ...
        let (inicio, fin) = (stat.find('(')?, stat.rfind(')')?);
        let comm = stat[inicio + 1..fin].to_string();
        let ppid = stat[fin + 2..].split_whitespace().nth(1)?.parse().ok()?;
        Some((comm, ppid))
    };
    for e in std::fs::read_dir("/proc").ok()?.flatten() {
        let nombre = e.file_name();
        let pid = nombre.to_string_lossy();
        if let Some((comm, ppid)) = leer(&pid)
            && comm == "Main"
            && leer(&ppid.to_string()).is_some_and(|(c, _)| c == "sober")
        {
            return Some(ppid);
        }
    }
    None
}

/// Cierra Sober (y con él Roblox). Primero de forma ordenada (TERM, ~0,2 s),
/// para que guarde su estado; si en 2 s no se ha cerrado, se fuerza.
pub fn cerrar_cliente(_pid: u32) {
    if let Some(p) = pid_sober() {
        unsafe {
            libc::kill(p, libc::SIGTERM);
        }
        for _ in 0..40 {
            if !sober_abierto() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    let _ = Command::new("flatpak")
        .args(["kill", SOBER])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// La comprobación de que Sober está instalado se hace al arrancar el
/// launcher (`estado_cliente`), no aquí: así Jugar no espera a otro proceso.
pub async fn lanzar(uri: &str, ajustes: &AjustesCliente, progreso: ProgresoFn) -> Result<std::process::Child> {
    // Sober solo admite una instancia: si ya está abierto, la nueva falla con
    // «An instance of Sober is already running» y el juego no se abre. Se
    // cierra la anterior primero.
    if sober_abierto() {
        progreso(Progreso::CerrandoAnterior);
        cerrar_cliente(0);
        for _ in 0..60 {
            if !sober_abierto() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        // Margen para que la instancia anterior suelte sus recursos: sin él,
        // a veces la nueva se queda mostrando un aviso de Sober.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    progreso(Progreso::Iniciando);
    aplicar_ajustes_sober(ajustes)?;
    if let Some(h) = dirs::home_dir() {
        let xml = h.join(".var/app").join(SOBER).join("data/sober/appData/GlobalBasicSettings_13.xml");
        crate::roblox::fijar_limite_fps(&xml, ajustes.limite_fps)?;
    }
    let mut sober: Vec<String> = ["flatpak", "run"].map(String::from).to_vec();
    sober.extend(variables_efectos(ajustes)?.into_iter().map(|(k, v)| format!("--env={k}={v}")));
    sober.extend(["--command=sober".to_string(), SOBER.to_string()]);
    if !uri.is_empty() {
        sober.extend(["--".to_string(), uri.to_string()]);
    }
    // «Fingir FPS» necesita gamescope (su overlay mangoapp); solo al unirse a un juego.
    let fingir = ajustes.fingir_fps && !uri.is_empty() && gamescope_disponible();
    let factor = ajustes.escalado.factor().or(fingir.then_some(1.0));
    let mut cmd = match factor.filter(|_| gamescope_disponible()) {
        // Estilo Lossless Scaling: Sober dentro de gamescope, que lo dibuja
        // a `factor` de la resolución de la pantalla y lo amplía con FSR 1.
        Some(factor) => {
            let (w, h) = resolucion_pantalla().unwrap_or((1920, 1080));
            let par = |n: u32| ((n as f32 * factor).round() as u32) & !1;
            let mut c = Command::new("gamescope");
            c.args([
                "-W", &w.to_string(), "-H", &h.to_string(),
                "-w", &par(w).to_string(), "-h", &par(h).to_string(),
                "-F", "fsr", "--sharpness", "3",
                "-f", "--force-windows-fullscreen", "--force-grab-cursor",
            ]);
            if fingir {
                // El texto lo dibuja el overlay de gamescope, fuera del juego.
                c.arg("--mangoapp");
                c.env("MANGOHUD_CONFIG", "custom_text_center=6767 FPS,legacy_layout=0,fps=0,frametime=0,frame_timing=0,cpu_stats=0,gpu_stats=0,font_size=40,position=top-left,background_alpha=0");
            }
            c.args([
                "--",
                // Sober por X11 (su permiso fallback-x11): por Wayland abría
                // su ventana a 800x600 y gamescope no podía llevarla a la
                // pantalla entera (se veía en 4:3 con el menú cortado). Por
                // X11, --force-windows-fullscreen la pone al tamaño interno.
                "env", "-u", "WAYLAND_DISPLAY",
            ]);
            c.args(&sober);
            c
        }
        None => {
            let mut c = Command::new(&sober[0]);
            c.args(&sober[1..]);
            c
        }
    };
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().context("no se pudo iniciar Sober")
}

/// Variables de entorno para las capas Vulkan de Sober: vkBasalt (SMAA y
/// nitidez CAS) y MAKO (generación de fotogramas). Se pasan en cada arranque
/// con `flatpak run --env=`, sin dejar cambios permanentes en Sober.
///
/// MangoHud no se usa como limitador: con él, Sober renuncia a recrear su
/// dispositivo Vulkan («Performance will be negatively affected due to a
/// worse GPU memory layout»). vkBasalt no provoca ese aviso.
fn variables_efectos(ajustes: &AjustesCliente) -> Result<Vec<(&'static str, String)>> {
    let vulkan = ajustes.api != ApiGrafica::OpenGL;
    let mako = ajustes.mako && vulkan && mako_instalado();
    let mut v = Vec::new();
    if mako {
        // Modo solo con variables (MAKO_ENV): no hace falta su conf.toml ni
        // dar a Sober acceso a ~/.config. El objetivo de FPS es el límite.
        let objetivo = ajustes.limite_fps.unwrap_or(60);
        v.extend([
            ("ENABLE_MAKO", "1".to_string()),
            ("MAKO_ENV", "1".into()),
            ("MAKO_FRAME_GENERATION_PROVISIONED", "1".into()),
            ("MAKO_FRAME_GENERATION_ENABLED", "1".into()),
            ("MAKO_ADAPTIVE", "1".into()),
            ("MAKO_TARGET_FPS", objetivo.to_string()),
            ("MAKO_ADAPTIVE_MAX_MULTIPLIER", "2".into()),
            // Preajuste ligero: flujo óptico al 60 %, para GPU integradas.
            ("MAKO_ULTRA_PERFORMANCE", "1".into()),
            ("MAKO_DISABLE_HDR_EXPOSURE", "1".into()),
            ("DISABLE_GAMESCOPE_WSI", "1".into()),
            ("DISABLE_VKBASALT", "1".into()),
            ("DISABLE_LSFG", "1".into()),
            ("DISABLE_LSFGVK", "1".into()),
            ("VK_IMPLICIT_LAYER_PATH", "/usr/lib/extensions/vulkan/makorender/share/vulkan/implicit_layer.d".into()),
        ]);
        return Ok(v);
    }
    if ajustes.smaa && vulkan && extension_instalada(EXT_VKBASALT) {
        v.push(("ENABLE_VKBASALT", "1".into()));
        v.push(("VKBASALT_CONFIG_FILE", config_vkbasalt()?.to_string_lossy().into_owned()));
    }
    Ok(v)
}

/// Escribe la configuración de vkBasalt del launcher en la carpeta de Sober
/// (visible dentro de su sandbox) y devuelve su ruta. No toca el
/// vkBasalt.conf del usuario, si lo tiene.
fn config_vkbasalt() -> Result<PathBuf> {
    let dir = dirs::home_dir().context("sin carpeta personal")?.join(".var/app").join(SOBER).join("config/rustlauncher");
    std::fs::create_dir_all(&dir)?;
    let ruta = dir.join("vkBasalt.conf");
    // SMAA antes de que gamescope amplíe la imagen (FSR 1 está pensado para
    // una imagen ya suavizada) y CAS suave: después FSR añade su nitidez.
    let texto = "# Generado por RustLauncher: se reescribe en cada partida.\n\
effects = smaa:cas\n\
smaaEdgeDetection = luma\n\
smaaThreshold = 0.05\n\
smaaMaxSearchSteps = 32\n\
smaaMaxSearchStepsDiag = 16\n\
smaaCornerRounding = 25\n\
casSharpness = 0.35\n\
toggleKey = Home\n";
    if std::fs::read_to_string(&ruta).ok().as_deref() != Some(texto) {
        std::fs::write(&ruta, texto)?;
    }
    Ok(ruta)
}

/// Si hay instalada alguna rama de una extensión de Flatpak (del sistema o
/// del usuario).
fn extension_instalada(id: &str) -> bool {
    let mut raices = vec![PathBuf::from("/var/lib/flatpak/runtime")];
    if let Some(d) = dirs::data_dir() {
        raices.push(d.join("flatpak/runtime"));
    }
    raices.iter().any(|r| r.join(id).join("x86_64").is_dir())
}

pub fn vkbasalt_instalado() -> bool {
    extension_instalada(EXT_VKBASALT)
}

pub fn mako_instalado() -> bool {
    extension_instalada(EXT_MAKO)
}

/// La rama de las capas Vulkan que carga Sober (la del runtime
/// org.freedesktop.Platform en el que se basa su runtime de GNOME).
fn rama_capas_sober() -> Result<String> {
    let salida = |args: &[&str]| -> Result<String> {
        let o = Command::new("flatpak").args(args).stderr(Stdio::null()).output()?;
        anyhow::ensure!(o.status.success(), "flatpak {} falló", args.join(" "));
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let runtime = salida(&["info", "--show-runtime", SOBER])?;
    let meta = salida(&["info", "--show-metadata", runtime.trim()])?;
    let mut en_capas = false;
    for linea in meta.lines().map(str::trim) {
        if linea.starts_with('[') {
            en_capas = linea == "[Extension org.freedesktop.Platform.VulkanLayer]";
        } else if en_capas && let Some(v) = linea.strip_prefix("version") {
            if let Some(v) = v.trim_start().strip_prefix('=') {
                return Ok(v.trim().split(';').next().unwrap_or("").to_string());
            }
        }
    }
    anyhow::bail!("no se encontró la rama de capas Vulkan del runtime {}", runtime.trim())
}

/// Descarga MAKO (su versión fija), comprueba la huella e instala para el
/// usuario la extensión de Flatpak de la rama que usa Sober. `progreso`
/// recibe la fracción descargada (0 a 1).
pub async fn instalar_mako(http: &reqwest::Client, progreso: impl Fn(f32)) -> Result<()> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;
    let rama = tokio::task::spawn_blocking(rama_capas_sober).await??;
    anyhow::ensure!(MAKO_RAMAS.contains(&rama.as_str()), "MAKO no tiene versión para el runtime {rama} de Sober");
    let dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("RustLauncher/mako");
    tokio::fs::create_dir_all(&dir).await?;
    let archivo = dir.join("flatpaks.tar.xz");
    let mut resp = http.get(MAKO_URL).send().await?.error_for_status().context("no se pudo descargar MAKO")?;
    let total = resp.content_length().unwrap_or(157_000_000).max(1);
    let mut f = tokio::fs::File::create(&archivo).await?;
    let mut huella = Sha256::new();
    let mut hechos = 0u64;
    while let Some(trozo) = resp.chunk().await.context("se cortó la descarga de MAKO")? {
        huella.update(&trozo);
        f.write_all(&trozo).await?;
        hechos += trozo.len() as u64;
        progreso((hechos as f32 / total as f32).min(1.0));
    }
    f.flush().await?;
    drop(f);
    let hex: String = huella.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if hex != MAKO_SHA256 {
        let _ = tokio::fs::remove_file(&archivo).await;
        anyhow::bail!("la descarga de MAKO no coincide con la versión revisada");
    }
    let paquete = format!("org.freedesktop.Platform.VulkanLayer.makorender-{rama}.flatpak");
    let r = tokio::task::spawn_blocking({
        let (dir, archivo, paquete) = (dir.clone(), archivo.clone(), paquete.clone());
        move || -> Result<()> {
            let ok = Command::new("tar")
                .arg("-xJf")
                .arg(&archivo)
                .arg("-C")
                .arg(&dir)
                .arg(format!("./{paquete}"))
                .status()?
                .success();
            anyhow::ensure!(ok, "no se pudo descomprimir MAKO");
            let o = Command::new("flatpak")
                .args(["install", "--user", "--noninteractive"])
                .arg(dir.join(&paquete))
                .stdout(Stdio::null())
                .output()?;
            anyhow::ensure!(
                o.status.success(),
                "flatpak no pudo instalar MAKO: {}",
                String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("").trim()
            );
            Ok(())
        }
    })
    .await?;
    let _ = tokio::fs::remove_file(&archivo).await;
    let _ = tokio::fs::remove_file(dir.join(&paquete)).await;
    r
}

/// Si gamescope está instalado (Bazzite y SteamOS lo traen).
fn gamescope_disponible() -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("gamescope").is_file()))
}

/// La resolución de la pantalla conectada (la mayor, si hay varias), de
/// /sys/class/drm: el primer modo de cada conector es el nativo.
fn resolucion_pantalla() -> Option<(u32, u32)> {
    let mut mejor: Option<(u32, u32)> = None;
    for e in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let d = e.path();
        if std::fs::read_to_string(d.join("status")).unwrap_or_default().trim() != "connected" {
            continue;
        }
        let modos = std::fs::read_to_string(d.join("modes")).unwrap_or_default();
        let Some((w, h)) = modos.lines().next().and_then(|m| m.split_once('x')) else { continue };
        let (Ok(w), Ok(h)) = (w.trim().parse::<u32>(), h.trim().trim_end_matches(|c: char| !c.is_ascii_digit()).parse::<u32>()) else { continue };
        if mejor.is_none_or(|(a, b)| w * h > a * b) {
            mejor = Some((w, h));
        }
    }
    mejor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rama_de_las_capas_de_sober() {
        if sober_instalado() {
            assert!(MAKO_RAMAS.contains(&rama_capas_sober().unwrap().as_str()));
        }
    }

    /// Descarga e instala MAKO de verdad: `cargo test instala_mako -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn instala_mako() {
        let http = crate::roblox::cliente_http().unwrap();
        instalar_mako(&http, |_| {}).await.unwrap();
        assert!(mako_instalado());
    }
}
