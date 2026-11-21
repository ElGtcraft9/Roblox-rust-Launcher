//! Windows: descarga el cliente oficial, registra los protocolos y lanza
//! RobloxPlayerBeta.exe.

use crate::config::{AjustesCliente, base};
use crate::plataforma::{Progreso, ProgresoFn};
use crate::{flags, roblox};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const PROTOCOLOS: &[&str] = &["roblox-player", "roblox"];

pub const NOMBRE_CLIENTE: &str = "Roblox";
/// Windows permite elegir DirectX 11 y Vulkan además de OpenGL.
pub const APIS_GRAFICAS: bool = true;
pub const ACTUALIZA_CLIENTE: bool = true;
/// En Windows, la primera vez se registra para abrir los juegos de roblox.com.
pub const ENLACES_POR_DEFECTO: bool = true;

fn exe_instalado() -> PathBuf {
    base().join("RustLauncher.exe")
}

pub fn protocolos_registrados() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Classes\roblox-player\shell\open\command")
        .and_then(|k| k.get_value::<String, _>(""))
        .is_ok_and(|v| v.contains("RustLauncher"))
}

/// Registra (o quita) el launcher como programa para los enlaces de roblox.com.
pub fn registrar(activar: bool) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if !activar {
        if protocolos_registrados() {
            for proto in PROTOCOLOS {
                let _ = hkcu.delete_subkey_all(format!(r"Software\Classes\{proto}"));
            }
        }
        return Ok(());
    }

    // Copia estable del ejecutable, para que el registro no dependa de dónde
    // se descargó.
    let destino = exe_instalado();
    std::fs::create_dir_all(base())?;
    let actual = std::env::current_exe()?;
    if actual.canonicalize().ok() != destino.canonicalize().ok() {
        // Si la copia instalada está en uso, se renombra antes de sustituirla.
        if destino.exists() {
            let viejo = destino.with_extension("old.exe");
            let _ = std::fs::remove_file(&viejo);
            let _ = std::fs::rename(&destino, &viejo);
        }
        std::fs::copy(&actual, &destino).context("no se pudo copiar el launcher")?;
    }

    let exe = destino.display().to_string();
    for proto in PROTOCOLOS {
        let (clave, _) = hkcu.create_subkey(format!(r"Software\Classes\{proto}"))?;
        clave.set_value("", &"URL: Roblox Protocol")?;
        clave.set_value("URL Protocol", &"")?;
        let (icono, _) = clave.create_subkey("DefaultIcon")?;
        icono.set_value("", &format!("\"{exe}\",0"))?;
        let (cmd, _) = clave.create_subkey(r"shell\open\command")?;
        cmd.set_value("", &format!("\"{exe}\" \"%1\""))?;
    }
    Ok(())
}

/// Última versión comprobada y cuándo. Si es reciente, Jugar no espera a la red.
static ULTIMA: std::sync::Mutex<Option<(String, std::time::Instant)>> = std::sync::Mutex::new(None);
const VIGENCIA_VERSION: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Instala la última versión del cliente si hace falta. Devuelve su carpeta.
pub async fn actualizar(progreso: ProgresoFn) -> Result<PathBuf> {
    let raiz = base().join("Versions");
    let reciente = ULTIMA
        .lock()
        .unwrap()
        .clone()
        .filter(|(v, t)| t.elapsed() < VIGENCIA_VERSION && roblox::version_completa(&raiz.join(v)));
    if let Some((v, _)) = reciente {
        return Ok(raiz.join(v));
    }
    let cliente = roblox::cliente_http()?;
    progreso(Progreso::Comprobando);
    let version = roblox::version_actual(&cliente).await?;
    let p = progreso.clone();
    let dir = roblox::instalar(&cliente, &version, &raiz, &base().join("Downloads"), &move |h, t| {
        p(Progreso::Descargando { hechos: h, total: t })
    })
    .await?;
    roblox::limpiar_versiones(&raiz, &version);
    *ULTIMA.lock().unwrap() = Some((version, std::time::Instant::now()));
    Ok(dir)
}

/// Archivos del cliente que conviene tener en la caché del sistema.
pub fn rutas_precarga() -> crate::acelerar::Precarga {
    let mut completas = Vec::new();
    if let Some((v, _)) = &*ULTIMA.lock().unwrap() {
        completas.push(base().join("Versions").join(v));
    }
    let mut recientes = Vec::new();
    if let Some(local) = dirs::data_local_dir() {
        // Cachés del cliente de Roblox: base de datos de recursos y recursos
        // de juegos (de estos, solo lo reciente).
        let roblox = local.join("Roblox");
        completas.push(roblox.join("rbx-storage.db"));
        recientes.push(roblox.join("rbx-storage"));
    }
    crate::acelerar::Precarga { completas, recientes }
}

/// Devuelve el proceso del juego para saber cuándo se cierra.
pub async fn lanzar(uri: &str, ajustes: &AjustesCliente, progreso: ProgresoFn) -> Result<std::process::Child> {
    let dir = actualizar(progreso.clone()).await?;
    // Roblox solo admite una copia: si ya está abierto, la nueva trae al frente
    // la antigua y se cierra sin abrir el juego. Se cierra la anterior primero.
    if roblox_abierto() {
        progreso(Progreso::CerrandoAnterior);
        cerrar_por_nombre();
        for _ in 0..60 {
            if !roblox_abierto() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    progreso(Progreso::Iniciando);

    let ajustes_dir = dir.join("ClientSettings");
    std::fs::create_dir_all(&ajustes_dir)?;
    std::fs::write(ajustes_dir.join("ClientAppSettings.json"), flags::json_windows(ajustes))?;

    let mut cmd = Command::new(dir.join("RobloxPlayerBeta.exe"));
    cmd.current_dir(&dir);
    if uri.is_empty() {
        cmd.arg("--app");
    } else {
        cmd.arg(uri);
    }
    cmd.spawn().context("no se pudo iniciar RobloxPlayerBeta.exe")
}

/// Carpeta de registros del cliente y el texto que identifica su archivo.
pub fn registros() -> (PathBuf, &'static str) {
    let dir = dirs::data_local_dir().unwrap_or_default().join("Roblox").join("logs");
    (dir, "Player")
}

fn sin_ventana(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000)
}

fn roblox_abierto() -> bool {
    sin_ventana(Command::new("tasklist").args(["/FI", "IMAGENAME eq RobloxPlayerBeta.exe", "/NH"]))
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("RobloxPlayerBeta.exe"))
}

fn cerrar_por_nombre() {
    let _ = sin_ventana(Command::new("taskkill").args(["/IM", "RobloxPlayerBeta.exe", "/F"])).status();
}

/// Cierra el cliente de Roblox.
pub fn cerrar_cliente(pid: u32) {
    use std::os::windows::process::CommandExt;
    const SIN_VENTANA: u32 = 0x0800_0000;
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(SIN_VENTANA)
        .status();
}

pub fn estado_cliente() -> Option<String> {
    None
}
