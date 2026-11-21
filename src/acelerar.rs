//! Lo que el launcher puede hacer para que el juego arranque antes.
//!
//! El tiempo de entrar a una experiencia lo deciden el cliente de Roblox y sus
//! servidores. El launcher solo puede quitar esperas que van antes o alrededor:
//!
//! 1. Precargar en la caché del sistema los archivos del cliente, sus cachés
//!    de sombreadores y los recursos de los juegos jugados hace poco: el
//!    arranque lee de memoria en lugar de disco (se nota sobre todo tras
//!    reiniciar el PC o con disco duro). Se repite mientras el launcher siga
//!    abierto, por si el sistema liberó esa memoria.
//! 2. Resolver por adelantado los dominios que usa Roblox al entrar: el sistema
//!    guarda la respuesta DNS y el cliente se ahorra esa consulta.
//! 3. No comprobar la versión al pulsar Jugar si ya se comprobó hace poco
//!    (ver `plataforma::actualizar`).

#[cfg(not(target_os = "linux"))]
use std::io::Read;
use std::path::{Path, PathBuf};

/// Dominios que el cliente de Roblox consulta al entrar a una experiencia.
const DOMINIOS: &[&str] = &[
    "www.roblox.com",
    "apis.roblox.com",
    "gamejoin.roblox.com",
    "assetdelivery.roblox.com",
    "clientsettings.roblox.com",
    "clientsettingscdn.roblox.com",
    "auth.roblox.com",
    "ecsv2.roblox.com",
    "setup.rbxcdn.com",
    "tr.rbxcdn.com",
    "c0.rbxcdn.com",
    "c1.rbxcdn.com",
    "c2.rbxcdn.com",
    "c3.rbxcdn.com",
    "c4.rbxcdn.com",
    "c5.rbxcdn.com",
    "c6.rbxcdn.com",
    "c7.rbxcdn.com",
];

pub async fn calentar_dns() {
    let consultas = DOMINIOS.iter().map(|d| tokio::net::lookup_host((*d, 443)));
    let _ = futures_util::future::join_all(consultas).await;
}

/// Qué precargar: carpetas completas y carpetas de las que solo interesan
/// los archivos recientes (la caché de recursos de juegos ocupa gigas).
pub struct Precarga {
    pub completas: Vec<PathBuf>,
    pub recientes: Vec<PathBuf>,
}

/// Los recursos de juegos usados en estos días son los que con más
/// probabilidad se vuelven a necesitar.
const RECIENTE: std::time::Duration = std::time::Duration::from_secs(14 * 24 * 3600);

fn listar(raiz: &Path, salida: &mut Vec<(PathBuf, u64, std::time::SystemTime)>) {
    let mut pendientes = vec![raiz.to_path_buf()];
    while let Some(ruta) = pendientes.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&ruta) else { continue };
        if meta.is_dir() {
            if let Ok(dir) = std::fs::read_dir(&ruta) {
                pendientes.extend(dir.flatten().map(|e| e.path()));
            }
        } else if meta.is_file() {
            let m = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            salida.push((ruta, meta.len(), m));
        }
    }
}

/// Pide al sistema que deje los archivos en su caché de disco, hasta
/// `limite` bytes. Devuelve cuántos archivos y bytes se pidieron.
///
/// En Linux no copia nada: le pide al núcleo que los lea por su cuenta
/// (`posix_fadvise`) con prioridad de disco baja, así no compite con nada.
/// Si ya están en memoria, no cuesta casi nada.
pub fn precargar(p: &Precarga, limite: u64) -> (usize, u64) {
    #[cfg(target_os = "linux")]
    prioridad_disco_baja();
    let mut archivos = Vec::new();
    for r in &p.completas {
        listar(r, &mut archivos);
    }
    let mut recientes = Vec::new();
    for r in &p.recientes {
        listar(r, &mut recientes);
    }
    let ahora = std::time::SystemTime::now();
    recientes.retain(|(_, _, m)| ahora.duration_since(*m).unwrap_or_default() < RECIENTE);
    // Lo más reciente primero.
    recientes.sort_by_key(|a| std::cmp::Reverse(a.2));
    archivos.extend(recientes);

    let (mut n, mut total) = (0usize, 0u64);
    for (ruta, largo, _) in archivos {
        if total + largo > limite {
            continue;
        }
        if cargar(&ruta) {
            n += 1;
            total += largo;
        }
    }
    (n, total)
}

#[cfg(target_os = "linux")]
fn prioridad_disco_baja() {
    // ioprio_set(IOPRIO_WHO_PROCESS, este hilo, clase IDLE): solo usa el disco
    // cuando nadie más lo necesita.
    const IOPRIO_WHO_PROCESS: libc::c_long = 1;
    const IOPRIO_CLASS_IDLE: libc::c_long = 3;
    unsafe {
        libc::syscall(libc::SYS_ioprio_set, IOPRIO_WHO_PROCESS, 0 as libc::c_long, IOPRIO_CLASS_IDLE << 13);
    }
}

#[cfg(target_os = "linux")]
fn cargar(ruta: &PathBuf) -> bool {
    use std::os::fd::AsRawFd;
    let Ok(f) = std::fs::File::open(ruta) else { return false };
    unsafe { libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_WILLNEED) == 0 }
}

#[cfg(not(target_os = "linux"))]
fn cargar(ruta: &PathBuf) -> bool {
    let Ok(mut f) = std::fs::File::open(ruta) else { return false };
    let mut buf = vec![0u8; 1 << 20];
    while let Ok(n) = f.read(&mut buf) {
        if n == 0 {
            break;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precarga_respeta_el_limite_y_la_antiguedad() {
        let dir = std::env::temp_dir().join("rust-launcher-precarga");
        let _ = std::fs::remove_dir_all(&dir);
        let (base, cache) = (dir.join("base"), dir.join("cache"));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(base.join("a"), vec![1u8; 1 << 20]).unwrap();
        std::fs::write(cache.join("nuevo"), vec![1u8; 1 << 20]).unwrap();
        let viejo = cache.join("viejo");
        std::fs::write(&viejo, vec![1u8; 1 << 10]).unwrap();
        let hace_un_mes = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 24 * 3600);
        std::fs::File::options().write(true).open(&viejo).unwrap().set_modified(hace_un_mes).unwrap();
        let p = Precarga { completas: vec![base], recientes: vec![cache] };
        // Caben los dos de 1 MB; el viejo se descarta por antigüedad.
        assert_eq!(precargar(&p, 3 << 20), (2, 2 << 20));
        // Con 1,5 MB solo cabe uno.
        assert_eq!(precargar(&p, 3 << 19).0, 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
