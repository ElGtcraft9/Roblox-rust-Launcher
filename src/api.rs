//! APIs web públicas de Roblox (no requieren iniciar sesión).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Juego {
    pub universe_id: u64,
    pub place_id: u64,
    pub nombre: String,
    #[serde(default)]
    pub jugando: Option<u64>,
    #[serde(default)]
    pub votos: Option<(u64, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seccion {
    pub titulo: String,
    pub juegos: Vec<Juego>,
}

#[derive(Debug, Clone)]
pub struct Detalle {
    pub universe_id: u64,
    pub place_id: u64,
    pub nombre: String,
    pub descripcion: String,
    pub creador: String,
    pub jugando: u64,
    pub visitas: u64,
    pub max_jugadores: u64,
    pub votos: Option<(u64, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Usuario {
    pub id: u64,
    pub nombre: String,
    pub nombre_visible: String,
}

fn sesion() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{t:x}")
}

async fn get_json(c: &reqwest::Client, url: &str) -> Result<Value> {
    Ok(c.get(url)
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?
        .json()
        .await?)
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn juego_de(v: &Value) -> Option<Juego> {
    let universe_id = v["universeId"].as_u64()?;
    let place_id = v["rootPlaceId"].as_u64()?;
    let votos = match (v["totalUpVotes"].as_u64(), v["totalDownVotes"].as_u64()) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    };
    Some(Juego {
        universe_id,
        place_id,
        nombre: s(&v["name"]),
        jugando: v["playerCount"].as_u64(),
        votos,
    })
}

fn titulo_seccion(id: &str, nombre: &str) -> String {
    if id.to_ascii_lowercase().contains("continue") {
        return "Seguir jugando".into();
    }
    match id {
        "top-trending" => "En tendencia",
        "up-and-coming" => "En ascenso",
        "top-playing-now" => "Lo más jugado ahora",
        "fun-with-friends" => "Para jugar con amigos",
        "top-revisited" => "A los que más se vuelve",
        "top-earning" => "Los más populares",
        _ => nombre,
    }
    .to_string()
}

/// Con sesión iniciada, Roblox añade estantes personales (p. ej. los juegos
/// a los que jugaste: «Seguir jugando»).
pub async fn secciones_inicio(c: &reqwest::Client, cookie: Option<&str>) -> Result<Vec<Seccion>> {
    let url = format!(
        "https://apis.roblox.com/explore-api/v1/get-sorts?sessionId={}&device=computer&country=all",
        sesion()
    );
    let mut req = c.get(&url);
    if let Some(ck) = cookie {
        req = req.header(reqwest::header::COOKIE, format!(".ROBLOSECURITY={ck}"));
    }
    let v: Value = req.send().await?.error_for_status()?.json().await?;
    let mut out = Vec::new();
    for sort in v["sorts"].as_array().into_iter().flatten() {
        if sort["contentType"] != "Games" {
            continue;
        }
        let juegos: Vec<Juego> = sort["games"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|g| g["isSponsored"] != true)
            .filter_map(juego_de)
            .take(24)
            .collect();
        if !juegos.is_empty() {
            out.push(Seccion {
                titulo: titulo_seccion(sort["sortId"].as_str().unwrap_or(""), &s(&sort["sortDisplayName"])),
                juegos,
            });
        }
    }
    Ok(out)
}

pub async fn buscar(c: &reqwest::Client, consulta: &str) -> Result<Vec<Juego>> {
    let url = reqwest::Url::parse_with_params(
        "https://apis.roblox.com/search-api/omni-search",
        &[("searchQuery", consulta), ("sessionId", &sesion()), ("pageType", "all")],
    )?;
    let v = get_json(c, url.as_str()).await?;
    Ok(v["searchResults"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["contentGroupType"] == "Game")
        .flat_map(|g| g["contents"].as_array().cloned().unwrap_or_default())
        .filter(|g| g["isSponsored"] != true)
        .filter_map(|g| juego_de(&g))
        .collect())
}

pub async fn detalle(c: &reqwest::Client, universe_id: u64) -> Result<Detalle> {
    let url_info = format!("https://games.roblox.com/v1/games?universeIds={universe_id}");
    let url_votos = format!("https://games.roblox.com/v1/games/votes?universeIds={universe_id}");
    let (info, votos) = tokio::join!(get_json(c, &url_info), get_json(c, &url_votos));
    let info = info?;
    let g = info["data"].get(0).context("experiencia no encontrada")?;
    let votos = votos.ok().and_then(|v| {
        let d = v["data"].get(0)?.clone();
        Some((u(&d["upVotes"]), u(&d["downVotes"])))
    });
    Ok(Detalle {
        universe_id,
        place_id: u(&g["rootPlaceId"]),
        nombre: s(&g["name"]),
        descripcion: s(&g["description"]),
        creador: s(&g["creator"]["name"]),
        jugando: u(&g["playing"]),
        visitas: u(&g["visits"]),
        max_jugadores: u(&g["maxPlayers"]),
        votos,
    })
}

/// Clase de miniatura que se puede pedir a thumbnails.roblox.com.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoImg {
    Icono,
    Banner,
    Prenda,
    Avatar,
    Cabeza,
    Traje,
    Paquete,
    PrendaGrande,
    PaqueteGrande,
}

impl TipoImg {
    pub fn prefijo(self) -> &'static str {
        match self {
            TipoImg::Icono => "icono",
            TipoImg::Banner => "banner",
            TipoImg::Prenda => "prenda",
            TipoImg::Avatar => "avatar",
            TipoImg::Cabeza => "cabeza",
            TipoImg::Traje => "traje",
            TipoImg::Paquete => "paquete",
            TipoImg::PrendaGrande => "prenda-g",
            TipoImg::PaqueteGrande => "paquete-g",
        }
    }
}

/// Resuelve las URLs de las miniaturas de varios ids de una sola vez.
/// Se piden en WebP: de 5 a 9 veces menos datos que PNG con la misma calidad.
pub async fn urls_miniaturas(c: &reqwest::Client, tipo: TipoImg, ids: &[u64]) -> Result<HashMap<u64, String>> {
    let lista = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
    let base = "https://thumbnails.roblox.com/v1";
    let url = match tipo {
        TipoImg::Icono => format!("{base}/games/icons?universeIds={lista}&size=256x256&format=Webp"),
        TipoImg::Banner => format!(
            "{base}/games/multiget/thumbnails?universeIds={lista}&size=768x432&format=Webp&countPerUniverse=1"
        ),
        TipoImg::Prenda => format!("{base}/assets?assetIds={lista}&size=150x150&format=Webp"),
        TipoImg::Avatar => format!("{base}/users/avatar?userIds={lista}&size=720x720&format=Webp"),
        TipoImg::Cabeza => format!("{base}/users/avatar-headshot?userIds={lista}&size=150x150&format=Webp"),
        TipoImg::Traje => format!("{base}/users/outfits?userOutfitIds={lista}&size=150x150&format=Webp"),
        TipoImg::Paquete => format!("{base}/bundles/thumbnails?bundleIds={lista}&size=150x150&format=Webp"),
        TipoImg::PrendaGrande => format!("{base}/assets?assetIds={lista}&size=420x420&format=Webp"),
        TipoImg::PaqueteGrande => format!("{base}/bundles/thumbnails?bundleIds={lista}&size=420x420&format=Webp"),
    };
    let v = get_json(c, &url).await?;
    let mut out = HashMap::new();
    for d in v["data"].as_array().into_iter().flatten() {
        let (id, img) = if tipo == TipoImg::Banner {
            (d["universeId"].as_u64(), d["thumbnails"].get(0).map(|t| t["imageUrl"].clone()))
        } else {
            (d["targetId"].as_u64(), Some(d["imageUrl"].clone()))
        };
        let completada = tipo == TipoImg::Banner || d["state"] == "Completed";
        if let (true, Some(id), Some(Value::String(url))) = (completada, id, img) {
            out.insert(id, url);
        }
    }
    Ok(out)
}

// ---------- Mercado ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Articulo {
    pub id: u64,
    /// Paquete (cuerpo, animaciones…) en lugar de un objeto suelto.
    pub paquete: bool,
    pub nombre: String,
    pub creador: String,
    /// `None` si no está a la venta.
    pub precio: Option<u64>,
    pub descripcion: String,
}

impl Articulo {
    pub fn url(&self) -> String {
        if self.paquete {
            format!("https://www.roblox.com/bundles/{}", self.id)
        } else {
            format!("https://www.roblox.com/catalog/{}", self.id)
        }
    }
}

/// Categorías del mercado. El catálogo solo filtra por un tipo de objeto a la vez.
pub const CATEGORIAS_MERCADO: &[(&str, Option<u32>)] = &[
    ("Lo más vendido", None),
    ("Camisas", Some(11)),
    ("Pantalones", Some(12)),
    ("Camisetas", Some(2)),
    ("Sombreros", Some(8)),
    ("Pelo", Some(41)),
    ("Caras", Some(18)),
    ("Emotes", Some(61)),
];

pub async fn mercado(c: &reqwest::Client, tipo: Option<u32>, palabra: &str) -> Result<Vec<Articulo>> {
    let mut params: Vec<(&str, String)> = vec![("Limit", "30".into())];
    // Con palabra clave se ordena por relevancia; sin ella, lo más vendido.
    params.push(("SortType", if palabra.is_empty() { "2" } else { "0" }.into()));
    if let Some(t) = tipo {
        params.push(("assetTypeIds", t.to_string()));
    }
    if !palabra.is_empty() {
        params.push(("Keyword", palabra.to_string()));
    }
    let url = reqwest::Url::parse_with_params("https://catalog.roblox.com/v2/search/items/details", &params)?;
    let v = get_json(c, url.as_str()).await.context("el mercado no respondió (puede estar limitando peticiones; espera unos segundos)")?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some(Articulo {
                id: a["id"].as_u64()?,
                paquete: a["itemType"] == "Bundle",
                nombre: s(&a["name"]).trim().to_string(),
                creador: s(&a["creatorName"]),
                precio: a["price"].as_u64().or_else(|| a["lowestPrice"].as_u64()),
                descripcion: s(&a["description"]),
            })
        })
        .collect())
}

// ---------- Usuarios ----------

pub async fn buscar_usuarios(c: &reqwest::Client, consulta: &str) -> Result<Vec<Usuario>> {
    let url = reqwest::Url::parse_with_params(
        "https://users.roblox.com/v1/users/search",
        &[("keyword", consulta), ("limit", "25")],
    )?;
    let v = get_json(c, url.as_str()).await.context("la búsqueda de usuarios no respondió")?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|u| {
            Some(Usuario {
                id: u["id"].as_u64()?,
                nombre: s(&u["name"]),
                nombre_visible: s(&u["displayName"]),
            })
        })
        .collect())
}
