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
    let mut cmd = Command::new("flatpak");
    cmd.args(["run", "--command=sober", SOBER]);
    if !uri.is_empty() {
        cmd.args(["--", uri]);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().context("no se pudo iniciar Sober")
}
