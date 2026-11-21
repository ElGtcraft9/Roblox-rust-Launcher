//! Vigila el registro del cliente de Roblox (también dentro de Sober).
//!
//! Sirve para dos cosas:
//! - avanzar la barra de carga según lo que el cliente va haciendo de verdad;
//! - saber cuándo se sale de una experiencia (`leaveUGCGame`) para cerrar el
//!   cliente y volver al launcher. Los teletransportes no escriben esa línea.

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Evento {
    /// Fracción de 0 a 1 de la carga y qué está haciendo el cliente.
    Progreso(f32, &'static str),
    /// El jugador salió de la experiencia al menú de Roblox.
    Salida,
}

const ENTRA: &str = "! Joining game";
const SALE: &str = "leaveUGCGame";

/// Hitos del registro en el orden en que aparecen al abrir una experiencia.
const HITOS: &[(&str, f32, &str)] = &[
    // Sober abre una ventana propia (aviso o error) antes de arrancar Roblox.
    ("UIServicesConduit", 0.29, "Sober está mostrando un aviso en su ventana"),
    ("RobloxChannel has been set", 0.32, "Cargando el cliente"),
    ("nativeInitClientSettings", 0.36, "Cargando el cliente"),
    ("launchUGCGame", 0.46, "Preparando la experiencia"),
    ("replaceDataModel", 0.56, "Preparando la experiencia"),
    (ENTRA, 0.70, "Uniéndose a un servidor"),
    ("Connection accepted", 0.85, "Conectando con el servidor"),
    ("Replicator created", 1.0, "Cargando el mundo"),
];

/// Busca el registro más reciente modificado desde `desde`.
fn registro_actual(dir: &PathBuf, desde: SystemTime, filtro: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(filtro))
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?;
            (m >= desde).then(|| (m, e.path()))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p)
}

/// Sigue el registro mientras `activo` sea cierto. Devuelve `true` si el
/// jugador salió de la experiencia.
pub async fn vigilar(
    dir: PathBuf,
    filtro: &'static str,
    desde: SystemTime,
    activo: Arc<AtomicBool>,
    mut al_evento: impl FnMut(Evento) + Send,
) -> bool {
    // Margen por diferencias de reloj del sistema de archivos.
    let desde = desde - Duration::from_secs(2);
    let mut ruta: Option<PathBuf> = None;
    let mut posicion = 0u64;
    let mut resto = String::new();
    let mut dentro = false;
    let mut avance = 0.0f32;
    loop {
        // Mientras carga se mira a menudo para que la barra vaya fluida.
        let pausa = if avance < 1.0 { 200 } else { 700 };
        tokio::time::sleep(Duration::from_millis(pausa)).await;
        if !activo.load(Ordering::Relaxed) {
            return false;
        }
        let Some(actual) = registro_actual(&dir, desde, filtro) else { continue };
        if ruta.as_ref() != Some(&actual) {
            ruta = Some(actual.clone());
            posicion = 0;
            resto.clear();
        }
        let leido = tokio::task::spawn_blocking({
            let actual = actual.clone();
            move || -> std::io::Result<(Vec<u8>, u64)> {
                let mut f = std::fs::File::open(&actual)?;
                let largo = f.metadata()?.len();
                let inicio = if largo < posicion { 0 } else { posicion };
                f.seek(SeekFrom::Start(inicio))?;
                let mut buf = Vec::new();
                f.take(8 << 20).read_to_end(&mut buf)?;
                Ok((buf, inicio))
            }
        })
        .await;
        let Ok(Ok((buf, inicio))) = leido else { continue };
        posicion = inicio + buf.len() as u64;
        if !buf.is_empty() && avance < 0.28 {
            avance = 0.28;
            al_evento(Evento::Progreso(avance, "Arrancando Roblox"));
        }
        resto.push_str(&String::from_utf8_lossy(&buf));
        // Solo se analizan líneas completas.
        let Some(corte) = resto.rfind('\n') else { continue };
        let lineas: String = resto.drain(..=corte).collect();
        for l in lineas.lines() {
            for (texto, fraccion, etiqueta) in HITOS {
                if *fraccion > avance && l.contains(texto) {
                    avance = *fraccion;
                    al_evento(Evento::Progreso(avance, etiqueta));
                }
            }
            if l.contains(ENTRA) {
                dentro = true;
            } else if dentro && l.contains(SALE) {
                al_evento(Evento::Salida);
                return true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn progreso_y_salida() {
        let dir = std::env::temp_dir().join("rust-launcher-vigia");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("latest.log");
        std::fs::write(&log, "arranque\n").unwrap();
        let eventos = Arc::new(Mutex::new(Vec::new()));
        let activo = Arc::new(AtomicBool::new(true));
        let ev = eventos.clone();
        let tarea = tokio::spawn(vigilar(dir.clone(), "latest", SystemTime::now(), activo, move |e| {
            ev.lock().unwrap().push(e)
        }));
        tokio::time::sleep(Duration::from_millis(500)).await;
        // Una salida antes de entrar no cuenta.
        std::fs::write(
            &log,
            "arranque\n[x] leaveUGCGame\n[FLog::Output] ! Joining game 'a'\nConnection accepted from x\nReplicator created\n",
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(!tarea.is_finished());
        {
            let e = eventos.lock().unwrap();
            assert!(e.contains(&Evento::Progreso(0.70, "Uniéndose a un servidor")));
            assert!(e.contains(&Evento::Progreso(1.0, "Cargando el mundo")));
        }
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        std::io::Write::write_all(&mut f, b"[FLog::SingleSurfaceApp] leaveUGCGame: x\n").unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(3), tarea).await.unwrap().unwrap());
        assert_eq!(eventos.lock().unwrap().last(), Some(&Evento::Salida));
        let _ = std::fs::remove_dir_all(dir);
    }
}
