//! Carga de miniaturas sin bloquear la interfaz.
//!
//! Las peticiones de un fotograma se agrupan por tipo (una sola llamada a la
//! API para resolver hasta 100 URLs), se descargan en paralelo, se decodifican
//! fuera del hilo de la interfaz y se guardan en disco para el siguiente
//! arranque.

use crate::api::{self, TipoImg};
use crate::app::{Enviador, Msg};
use crate::config::base;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use futures_util::{StreamExt, stream};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::runtime::Handle;

pub type Img = (TipoImg, u64);

enum Estado {
    Cargando,
    Lista(TextureHandle),
    Fallo,
}

pub struct Imagenes {
    estado: HashMap<Img, Estado>,
    /// Color medio de cada icono de juego, para teñir la interfaz.
    colores: HashMap<u64, egui::Color32>,
    /// En el orden en que se pidieron: lo visible se pide primero.
    pendientes: Vec<Img>,
    rt: Handle,
    http: reqwest::Client,
    tx: Enviador,
}

fn dir() -> PathBuf {
    base().join("Cache").join("img")
}

fn ruta((tipo, id): Img) -> PathBuf {
    dir().join(format!("{}-{id}.webp", tipo.prefijo()))
}

/// Cuánto tiempo se considera vigente una imagen guardada.
fn vigencia(tipo: TipoImg) -> Duration {
    match tipo {
        TipoImg::Avatar | TipoImg::Cabeza => Duration::from_secs(10 * 60),
        _ => Duration::from_secs(3 * 24 * 3600),
    }
}

/// Color medio de una imagen, ignorando píxeles casi negros o casi blancos.
fn color_medio(img: &ColorImage) -> egui::Color32 {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for p in img.pixels.iter().step_by(7) {
        let [pr, pg, pb, _] = p.to_array();
        let max = pr.max(pg).max(pb);
        let min = pr.min(pg).min(pb);
        if max < 24 || min > 232 {
            continue;
        }
        r += pr as u64;
        g += pg as u64;
        b += pb as u64;
        n += 1;
    }
    if n == 0 {
        return egui::Color32::from_rgb(40, 44, 52);
    }
    egui::Color32::from_rgb((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

fn decodificar(bytes: &[u8]) -> Option<ColorImage> {
    // WebP normalmente; PNG para lo que llegue en ese formato.
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

fn leer_disco(k: Img) -> Option<ColorImage> {
    let r = ruta(k);
    let edad = std::fs::metadata(&r).ok()?.modified().ok()?.elapsed().ok()?;
    if edad > vigencia(k.0) {
        return None;
    }
    decodificar(&std::fs::read(r).ok()?)
}

impl Imagenes {
    pub fn new(rt: Handle, http: reqwest::Client, tx: Enviador) -> Self {
        // La caché antigua en PNG ya no se usa: se borra sin estorbar.
        rt.spawn_blocking(|| {
            for e in std::fs::read_dir(dir()).into_iter().flatten().flatten() {
                if e.path().extension().is_some_and(|x| x == "png") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        });
        Self {
            estado: HashMap::new(),
            colores: HashMap::new(),
            pendientes: Vec::new(),
            rt,
            http,
            tx,
        }
    }

    /// Devuelve la textura si está lista; si no, la pide (una sola vez).
    pub fn get(&mut self, k: Img) -> Option<TextureHandle> {
        match self.estado.get(&k) {
            Some(Estado::Lista(t)) => Some(t.clone()),
            Some(_) => None,
            None => {
                self.estado.insert(k, Estado::Cargando);
                self.pendientes.push(k);
                None
            }
        }
    }

    /// Fuerza a volver a pedir una imagen (p. ej. el avatar tras cambiarlo).
    pub fn olvidar(&mut self, k: Img) {
        self.estado.remove(&k);
        let _ = std::fs::remove_file(ruta(k));
    }

    /// Color medio del icono de un juego (si ya se cargó).
    pub fn color(&self, universe_id: u64) -> Option<egui::Color32> {
        self.colores.get(&universe_id).copied()
    }

    pub fn recibir(&mut self, ctx: &egui::Context, k: Img, img: Option<ColorImage>) {
        if crate::perfil::activo() && k.0 == TipoImg::Icono {
            let n = self.estado.values().filter(|e| matches!(e, Estado::Lista(_))).count() + 1;
            if [1, 8, 30, 60, 100].contains(&n) {
                crate::perfil::marca(&format!("icono nº {n} recibido"));
            }
        }
        if let (TipoImg::Icono, Some(img)) = (k.0, &img) {
            self.colores.insert(k.1, color_medio(img));
        }
        let estado = match img {
            Some(img) => {
                let nombre = format!("{}-{}", k.0.prefijo(), k.1);
                Estado::Lista(ctx.load_texture(nombre, img, TextureOptions::LINEAR))
            }
            None => Estado::Fallo,
        };
        self.estado.insert(k, estado);
    }

    pub fn borrar_cache(&mut self) {
        let _ = std::fs::remove_dir_all(dir());
        self.estado.clear();
    }

    /// Lanza la carga de todo lo pedido en este fotograma.
    pub fn despachar(&mut self) {
        if self.pendientes.is_empty() {
            return;
        }
        crate::perfil::marca(&format!("se piden {} imágenes", self.pendientes.len()));
        // Se agrupan por tipo conservando el orden de petición.
        let mut grupos: Vec<(TipoImg, Vec<u64>)> = Vec::new();
        for (t, id) in self.pendientes.drain(..) {
            match grupos.iter_mut().find(|(g, _)| *g == t) {
                Some((_, ids)) => ids.push(id),
                None => grupos.push((t, vec![id])),
            }
        }
        for (tipo, ids) in grupos {
            for trozo in ids.chunks(100) {
                let ids = trozo.to_vec();
                let http = self.http.clone();
                let tx = self.tx.clone();
                self.rt.spawn(cargar_grupo(http, tx, tipo, ids));
            }
        }
    }
}

async fn cargar_grupo(http: reqwest::Client, tx: Enviador, tipo: TipoImg, ids: Vec<u64>) {
    // 1. Lo que ya está en disco se muestra sin tocar la red. Se decodifica
    //    con tantos hilos como núcleos, en orden (lo visible primero), y cada
    //    imagen se envía en cuanto está lista.
    let nucleos = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let mut en_disco = stream::iter(ids.clone())
        .map(|id| tokio::task::spawn_blocking(move || (id, leer_disco((tipo, id)))))
        .buffered(nucleos);
    let mut faltan = Vec::new();
    let mut primera = true;
    while let Some(r) = en_disco.next().await {
        let Ok((id, img)) = r else { continue };
        if primera && img.is_some() {
            primera = false;
            crate::perfil::marca(&format!("primera imagen {tipo:?} decodificada (segundo plano)"));
        }
        match img {
            Some(img) => tx.enviar(Msg::Imagen((tipo, id), Some(img))),
            None => faltan.push(id),
        }
    }
    if faltan.is_empty() {
        return;
    }

    // 2. Una llamada para todas las URLs. Los avatares recién cambiados tardan
    //    unos segundos en renderizarse: se reintenta mientras estén pendientes.
    let reintentos = if matches!(tipo, TipoImg::Avatar | TipoImg::Cabeza | TipoImg::Traje) { 8 } else { 1 };
    let mut urls = HashMap::new();
    for intento in 0..reintentos {
        if intento > 0 {
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        let pedir: Vec<u64> = faltan.iter().copied().filter(|id| !urls.contains_key(id)).collect();
        match api::urls_miniaturas(&http, tipo, &pedir).await {
            Ok(u) => urls.extend(u),
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("miniaturas {tipo:?} ({} ids): {e:#}", pedir.len());
                let _ = e;
            }
        }
        if urls.len() == faltan.len() {
            break;
        }
    }
    let _ = std::fs::create_dir_all(dir());
    stream::iter(faltan)
        .for_each_concurrent(24, |id| {
            let url = urls.get(&id).cloned();
            let (http, tx) = (http.clone(), tx.clone());
            async move {
                let img = match url {
                    Some(url) => descargar(&http, &url, (tipo, id)).await,
                    None => None,
                };
                tx.enviar(Msg::Imagen((tipo, id), img));
            }
        })
        .await;
}

async fn descargar(http: &reqwest::Client, url: &str, k: Img) -> Option<ColorImage> {
    let r = async { http.get(url).send().await?.error_for_status()?.bytes().await }.await;
    #[cfg(debug_assertions)]
    if let Err(e) = &r {
        eprintln!("descarga {k:?}: {e:#}");
    }
    let bytes = r.ok()?;
    tokio::task::spawn_blocking(move || {
        let img = decodificar(&bytes)?;
        let _ = std::fs::write(ruta(k), &bytes);
        Some(img)
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    /// Mide cuánto cuesta decodificar las imágenes de una caché real.
    /// `RUST_LAUNCHER_CACHE_IMG=<carpeta> cargo test --release -- --ignored --nocapture decodificacion`
    #[test]
    #[ignore]
    fn decodificacion() {
        let Some(dir) = std::env::var_os("RUST_LAUNCHER_CACHE_IMG") else { return };
        let archivos: Vec<Vec<u8>> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("icono"))
            .map(|e| std::fs::read(e.path()).unwrap())
            .collect();
        let t = std::time::Instant::now();
        for b in &archivos {
            super::decodificar(b).unwrap();
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("{} iconos WebP: {:.1} ms en total, {:.2} ms cada uno", archivos.len(), ms, ms / archivos.len() as f64);
    }
}
