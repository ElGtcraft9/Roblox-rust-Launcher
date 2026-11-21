//! Sesión de Roblox para cambiar el avatar desde el launcher.
//!
//! El inicio de sesión usa el mecanismo oficial de «Quick Login» (el mismo que
//! las consolas): el launcher muestra un código, el usuario lo aprueba en
//! roblox.com con su sesión y Roblox devuelve una cookie de sesión. El launcher
//! nunca ve la contraseña.
//!
//! La cookie se guarda en el almacén de credenciales del sistema (Credential
//! Manager en Windows, Secret Service en Linux), nunca en un archivo propio.

use crate::api::Usuario;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Mutex;

const SERVICIO: &str = "RustLauncher";
const CUENTA: &str = "roblox-sesion";
const AUTH_TOKEN: &str = "https://apis.roblox.com/auth-token-service/v1/login";

// ---------- Almacén seguro ----------

fn entrada() -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICIO, CUENTA).map_err(|e| anyhow!("almacén de credenciales no disponible: {e}"))
}

pub fn guardar_cookie(cookie: &str) -> Result<()> {
    entrada()?.set_password(cookie).map_err(|e| anyhow!("no se pudo guardar la sesión: {e}"))
}

pub fn leer_cookie() -> Option<String> {
    entrada().ok()?.get_password().ok().filter(|c| !c.is_empty())
}

pub fn borrar_cookie() {
    if let Ok(e) = entrada() {
        let _ = e.delete_credential();
    }
}

// ---------- Peticiones con protección CSRF ----------

/// Roblox exige `x-csrf-token` en las peticiones que modifican algo: la
/// primera respuesta 403 trae el token y se repite la petición una vez.
async fn enviar(
    http: &reqwest::Client,
    metodo: reqwest::Method,
    url: &str,
    cuerpo: Option<&Value>,
    cookie: Option<&str>,
    csrf: &Mutex<String>,
) -> Result<reqwest::Response> {
    for _ in 0..2 {
        let mut req = http.request(metodo.clone(), url);
        if let Some(c) = cookie {
            req = req.header(reqwest::header::COOKIE, format!(".ROBLOSECURITY={c}"));
        }
        let token = csrf.lock().unwrap().clone();
        if !token.is_empty() {
            req = req.header("x-csrf-token", token);
        }
        req = match cuerpo {
            Some(b) => req.json(b),
            None => req.header(reqwest::header::CONTENT_TYPE, "application/json").body("{}"),
        };
        let resp = req.send().await?;
        if resp.status() == reqwest::StatusCode::FORBIDDEN
            && let Some(nuevo) = resp.headers().get("x-csrf-token").and_then(|v| v.to_str().ok())
            && *csrf.lock().unwrap() != nuevo
        {
            *csrf.lock().unwrap() = nuevo.to_string();
            continue;
        }
        return Ok(resp);
    }
    bail!("Roblox rechazó la petición (CSRF)")
}

async fn exigir_ok(resp: reqwest::Response, que: &str) -> Result<Value> {
    let estado = resp.status();
    let texto = resp.text().await.unwrap_or_default();
    if !estado.is_success() {
        let detalle = serde_json::from_str::<Value>(&texto)
            .ok()
            .and_then(|v| v["errors"][0]["message"].as_str().map(str::to_string))
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| format!("HTTP {estado}"));
        bail!("{que}: {detalle}");
    }
    Ok(serde_json::from_str(&texto).unwrap_or(Value::Null))
}

// ---------- Inicio de sesión con código ----------

#[derive(Debug, Clone)]
pub struct Codigo {
    pub codigo: String,
    clave: String,
    /// Ruta del QR relativa a la API (se descarga como imagen).
    pub qr: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EstadoCodigo {
    /// Aún nadie ha escrito el código.
    Creado,
    /// Alguien lo escribió; falta que confirme. Incluye el nombre parcial (p. ej. «me****»).
    Vinculado(Option<String>),
    Validado,
    Cancelado,
}

pub async fn crear_codigo(http: &reqwest::Client, csrf: &Mutex<String>) -> Result<Codigo> {
    let resp = enviar(http, reqwest::Method::POST, &format!("{AUTH_TOKEN}/create"), None, None, csrf).await?;
    let v = exigir_ok(resp, "No se pudo crear el código").await?;
    Ok(Codigo {
        codigo: v["code"].as_str().context("respuesta sin código")?.to_string(),
        clave: v["privateKey"].as_str().context("respuesta sin clave")?.to_string(),
        qr: v["imagePath"].as_str().unwrap_or_default().to_string(),
    })
}

pub fn url_qr(c: &Codigo) -> String {
    format!("https://apis.roblox.com/auth-token-service{}", c.qr)
}

pub async fn estado_codigo(http: &reqwest::Client, c: &Codigo, csrf: &Mutex<String>) -> Result<EstadoCodigo> {
    let cuerpo = json!({ "code": c.codigo, "privateKey": c.clave });
    let resp = enviar(http, reqwest::Method::POST, &format!("{AUTH_TOKEN}/status"), Some(&cuerpo), None, csrf).await?;
    if resp.status() == reqwest::StatusCode::BAD_REQUEST {
        // «CodeInvalid»: caducado o inexistente.
        return Ok(EstadoCodigo::Cancelado);
    }
    let v = exigir_ok(resp, "No se pudo consultar el código").await?;
    Ok(match v["status"].as_str().unwrap_or("") {
        "UserLinked" => EstadoCodigo::Vinculado(v["accountName"].as_str().map(str::to_string)),
        "Validated" => EstadoCodigo::Validado,
        "Cancelled" => EstadoCodigo::Cancelado,
        _ => EstadoCodigo::Creado,
    })
}

pub async fn cancelar_codigo(http: &reqwest::Client, c: &Codigo, csrf: &Mutex<String>) {
    let cuerpo = json!({ "code": c.codigo });
    let _ = enviar(http, reqwest::Method::POST, &format!("{AUTH_TOKEN}/cancel"), Some(&cuerpo), None, csrf).await;
}

/// Canjea un código validado por la cookie de sesión.
pub async fn completar(http: &reqwest::Client, c: &Codigo, csrf: &Mutex<String>) -> Result<String> {
    let cuerpo = json!({ "ctype": "AuthToken", "cvalue": c.codigo, "password": c.clave });
    let resp = enviar(http, reqwest::Method::POST, "https://auth.roblox.com/v2/login", Some(&cuerpo), None, csrf).await?;
    if resp.headers().contains_key("rblx-challenge-id") {
        bail!("Roblox ha pedido una verificación adicional que el launcher no puede completar. Vuelve a intentarlo más tarde.");
    }
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.strip_prefix(".ROBLOSECURITY="))
        .map(|v| v.split(';').next().unwrap_or_default().to_string());
    match cookie {
        Some(c) if !c.is_empty() => Ok(c),
        _ => {
            exigir_ok(resp, "Roblox no aceptó el inicio de sesión").await?;
            bail!("Roblox no devolvió una sesión")
        }
    }
}

// ---------- Sesión iniciada ----------

#[derive(Debug, Clone)]
pub struct Puesto {
    pub id: u64,
    pub tipo: u32,
    meta: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct AvatarEditable {
    pub r15: bool,
    pub puestos: Vec<Puesto>,
}

#[derive(Debug, Clone)]
pub struct Traje {
    pub id: u64,
    pub nombre: String,
}

#[derive(Debug, Clone)]
pub struct Objeto {
    pub id: u64,
    pub nombre: String,
    pub tipo: u32,
}

/// Categorías del editor y los tipos de objeto de Roblox que agrupan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Categoria {
    Trajes,
    Cabeza,
    Superior,
    Inferior,
    Accesorios,
}

impl Categoria {
    pub const TODAS: [Categoria; 5] =
        [Categoria::Trajes, Categoria::Cabeza, Categoria::Superior, Categoria::Inferior, Categoria::Accesorios];

    pub fn nombre(self) -> &'static str {
        match self {
            Categoria::Trajes => "Trajes",
            Categoria::Cabeza => "Cabeza y pelo",
            Categoria::Superior => "Parte de arriba",
            Categoria::Inferior => "Parte de abajo",
            Categoria::Accesorios => "Accesorios",
        }
    }

    pub fn tipos(self) -> &'static [u32] {
        match self {
            Categoria::Trajes => &[],
            // Sombrero, pelo, cara, accesorio de cara.
            Categoria::Cabeza => &[8, 41, 18, 42],
            // Camiseta, camisa, ropa por capas de arriba y chaquetas.
            Categoria::Superior => &[2, 11, 64, 65, 67, 68],
            // Pantalón, por capas, shorts, faldas y zapatos.
            Categoria::Inferior => &[12, 66, 69, 72, 70, 71],
            // Cuello, hombros, delante, espalda, cintura.
            Categoria::Accesorios => &[43, 44, 45, 46, 47],
        }
    }
}

/// Ropa por capas: necesita un orden de capa al ponérsela.
fn es_capa(tipo: u32) -> bool {
    (64..=72).contains(&tipo)
}

/// Tipos de los que solo se puede llevar uno a la vez.
fn es_unico(tipo: u32) -> bool {
    matches!(tipo, 2 | 11 | 12 | 18)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Presencia {
    Desconectado,
    EnLinea,
    /// Jugando; incluye el nombre de la experiencia si su privacidad lo permite.
    Jugando(Option<String>),
    EnStudio,
}

#[derive(Debug, Clone)]
pub struct Amigo {
    pub id: u64,
    pub nombre: String,
    pub nombre_visible: String,
    pub presencia: Presencia,
}

const CHAT: &str = "https://apis.roblox.com/platform-chat-api/v1";

#[derive(Debug, Clone)]
pub struct Conversacion {
    pub id: String,
    pub nombre: String,
    pub no_leidos: u64,
    pub previa: String,
    /// En chats privados, el otro usuario (para su foto).
    pub otro: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Mensaje {
    pub texto: String,
    pub autor: u64,
    pub fecha: String,
}

pub struct Sesion {
    http: reqwest::Client,
    cookie: String,
    csrf: Mutex<String>,
    pub usuario: Usuario,
}

impl Sesion {
    /// Comprueba la cookie con Roblox y devuelve la sesión.
    pub async fn abrir(http: reqwest::Client, cookie: String) -> Result<Sesion> {
        let resp = http
            .get("https://users.roblox.com/v1/users/authenticated")
            .header(reqwest::header::COOKIE, format!(".ROBLOSECURITY={cookie}"))
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            bail!("La sesión ha caducado. Vuelve a iniciar sesión.");
        }
        let v: Value = resp.error_for_status()?.json().await?;
        Ok(Sesion {
            usuario: Usuario {
                id: v["id"].as_u64().context("respuesta sin id")?,
                nombre: v["name"].as_str().unwrap_or_default().to_string(),
                nombre_visible: v["displayName"].as_str().unwrap_or_default().to_string(),
            },
            http,
            cookie,
            csrf: Mutex::new(String::new()),
        })
    }

    async fn get(&self, url: &str) -> Result<Value> {
        let resp = self
            .http
            .get(url)
            .header(reqwest::header::COOKIE, format!(".ROBLOSECURITY={}", self.cookie))
            .send()
            .await?;
        exigir_ok(resp, "Roblox devolvió un error").await
    }

    async fn post(&self, url: &str, cuerpo: Option<Value>, que: &str) -> Result<Value> {
        let resp = enviar(&self.http, reqwest::Method::POST, url, cuerpo.as_ref(), Some(&self.cookie), &self.csrf).await?;
        exigir_ok(resp, que).await
    }

    pub async fn avatar(&self) -> Result<AvatarEditable> {
        let v = self.get("https://avatar.roblox.com/v2/avatar/avatar").await?;
        let puestos = v["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| Puesto {
                id: a["id"].as_u64().unwrap_or(0),
                tipo: a["assetType"]["id"].as_u64().unwrap_or(0) as u32,
                meta: a.get("meta").cloned().filter(|m| !m.is_null()),
            })
            .collect();
        Ok(AvatarEditable {
            r15: v["playerAvatarType"].as_str() == Some("R15"),
            puestos,
        })
    }

    pub async fn trajes(&self) -> Result<Vec<Traje>> {
        let url = format!(
            "https://avatar.roblox.com/v2/avatar/users/{}/outfits?page=1&itemsPerPage=50&isEditable=true",
            self.usuario.id
        );
        let v = self.get(&url).await?;
        Ok(v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| {
                Some(Traje {
                    id: t["id"].as_u64()?,
                    nombre: t["name"].as_str().unwrap_or_default().to_string(),
                })
            })
            .collect())
    }

    pub async fn inventario(&self, cat: Categoria) -> Result<Vec<Objeto>> {
        let urls: Vec<(u32, String)> = cat
            .tipos()
            .iter()
            .map(|t| {
                (
                    *t,
                    format!(
                        "https://inventory.roblox.com/v2/users/{}/inventory/{t}?limit=100&sortOrder=Desc",
                        self.usuario.id
                    ),
                )
            })
            .collect();
        let respuestas = futures_util::future::join_all(urls.iter().map(|(_, u)| self.get(u))).await;
        let mut out = Vec::new();
        for ((tipo, _), r) in urls.iter().zip(respuestas) {
            for o in r?["data"].as_array().into_iter().flatten() {
                if let Some(id) = o["assetId"].as_u64() {
                    out.push(Objeto {
                        id,
                        nombre: o["assetName"].as_str().unwrap_or_default().to_string(),
                        tipo: *tipo,
                    });
                }
            }
        }
        Ok(out)
    }

    pub async fn poner_traje(&self, id: u64) -> Result<()> {
        self.post(&format!("https://avatar.roblox.com/v1/outfits/{id}/wear"), None, "No se pudo poner el traje")
            .await?;
        self.redibujar().await;
        Ok(())
    }

    /// Se pone o se quita un objeto y guarda el avatar resultante.
    pub async fn alternar(&self, actual: &AvatarEditable, obj: &Objeto) -> Result<()> {
        let mut puestos = actual.puestos.clone();
        if puestos.iter().any(|p| p.id == obj.id) {
            puestos.retain(|p| p.id != obj.id);
        } else {
            if es_unico(obj.tipo) {
                puestos.retain(|p| p.tipo != obj.tipo);
            }
            let meta = es_capa(obj.tipo).then(|| {
                let orden = puestos.iter().filter(|p| es_capa(p.tipo)).count() as u64 + 1;
                json!({ "order": orden, "version": 1 })
            });
            puestos.push(Puesto { id: obj.id, tipo: obj.tipo, meta });
        }
        let assets: Vec<Value> = puestos
            .iter()
            .map(|p| match &p.meta {
                Some(m) => json!({ "id": p.id, "meta": m }),
                None => json!({ "id": p.id }),
            })
            .collect();
        let v = self
            .post(
                "https://avatar.roblox.com/v2/avatar/set-wearing-assets",
                Some(json!({ "assets": assets })),
                "No se pudo cambiar el avatar",
            )
            .await?;
        if v["success"] == false {
            bail!("Roblox no aceptó ese cambio de avatar");
        }
        self.redibujar().await;
        Ok(())
    }

    pub async fn tipo_cuerpo(&self, r15: bool) -> Result<()> {
        let tipo = if r15 { "R15" } else { "R6" };
        self.post(
            "https://avatar.roblox.com/v1/avatar/set-player-avatar-type",
            Some(json!({ "playerAvatarType": tipo })),
            "No se pudo cambiar el tipo de cuerpo",
        )
        .await?;
        self.redibujar().await;
        Ok(())
    }

    async fn redibujar(&self) {
        let _ = self.post("https://avatar.roblox.com/v1/avatar/redraw-thumbnail", None, "").await;
    }

    pub fn cookie(&self) -> &str {
        &self.cookie
    }

    pub async fn descripcion(&self) -> Result<String> {
        let v = self.get(&format!("https://users.roblox.com/v1/users/{}", self.usuario.id)).await?;
        Ok(v["description"].as_str().unwrap_or_default().trim().to_string())
    }

    /// Amigos con nombre y presencia, primero los que están jugando o en línea.
    pub async fn amigos(&self) -> Result<Vec<Amigo>> {
        let v = self.get(&format!("https://friends.roblox.com/v1/users/{}/friends", self.usuario.id)).await?;
        let mut amigos: Vec<Amigo> = v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| {
                Some(Amigo {
                    id: a["id"].as_u64()?,
                    nombre: a["name"].as_str().unwrap_or_default().to_string(),
                    nombre_visible: a["displayName"].as_str().unwrap_or_default().to_string(),
                    presencia: Presencia::Desconectado,
                })
            })
            .take(200)
            .collect();
        let ids: Vec<u64> = amigos.iter().map(|a| a.id).collect();

        // Roblox ya no siempre manda los nombres en la lista: se piden aparte.
        if amigos.iter().any(|a| a.nombre.is_empty()) {
            let mut nombres = std::collections::HashMap::new();
            for trozo in ids.chunks(100) {
                let r = self
                    .post("https://users.roblox.com/v1/users", Some(json!({ "userIds": trozo, "excludeBannedUsers": false })), "")
                    .await;
                for u in r.ok().and_then(|v| v["data"].as_array().cloned()).unwrap_or_default() {
                    if let Some(id) = u["id"].as_u64() {
                        let n = u["name"].as_str().unwrap_or_default().to_string();
                        let d = u["displayName"].as_str().unwrap_or_default().to_string();
                        nombres.insert(id, (n, d));
                    }
                }
            }
            for a in &mut amigos {
                if let Some((n, d)) = nombres.get(&a.id) {
                    a.nombre = n.clone();
                    a.nombre_visible = d.clone();
                }
            }
        }

        for trozo in ids.chunks(50) {
            let r = self
                .post("https://presence.roblox.com/v1/presence/users", Some(json!({ "userIds": trozo })), "")
                .await;
            for p in r.ok().and_then(|v| v["userPresences"].as_array().cloned()).unwrap_or_default() {
                let Some(id) = p["userId"].as_u64() else { continue };
                let lugar = p["lastLocation"].as_str().filter(|l| !l.is_empty() && *l != "Website").map(str::to_string);
                let presencia = match p["userPresenceType"].as_u64() {
                    Some(1) => Presencia::EnLinea,
                    Some(2) => Presencia::Jugando(lugar),
                    Some(3) => Presencia::EnStudio,
                    _ => Presencia::Desconectado,
                };
                if let Some(a) = amigos.iter_mut().find(|a| a.id == id) {
                    a.presencia = presencia;
                }
            }
        }
        let orden = |p: &Presencia| match p {
            Presencia::Jugando(_) => 0,
            Presencia::EnLinea | Presencia::EnStudio => 1,
            Presencia::Desconectado => 2,
        };
        amigos.sort_by_key(|a| orden(&a.presencia));
        Ok(amigos)
    }

    pub async fn robux(&self) -> Result<u64> {
        let v = self.get(&format!("https://economy.roblox.com/v1/users/{}/currency", self.usuario.id)).await?;
        Ok(v["robux"].as_u64().unwrap_or(0))
    }

    pub async fn solicitar_amistad(&self, id: u64) -> Result<()> {
        let v = self
            .post(
                &format!("https://friends.roblox.com/v1/users/{id}/request-friendship"),
                Some(json!({ "friendshipOriginSourceType": "PlayerSearch" })),
                "No se pudo enviar la solicitud",
            )
            .await?;
        if v["success"] == false {
            bail!("Roblox no aceptó la solicitud");
        }
        Ok(())
    }

    // ---------- Chat ----------
    // API de chat de la web de Roblox (platform-chat-api). No está documentada
    // oficialmente: los nombres de campos salen de clientes de código abierto.

    pub async fn conversaciones(&self) -> Result<Vec<Conversacion>> {
        let v = self
            .get(&format!("{CHAT}/get-user-conversations?cursor=&include_user_data=true&pageSize=30"))
            .await?;
        let yo = self.usuario.id;
        Ok(v["conversations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| {
                let id = c["id"].as_str()?.to_string();
                let otro = c["participant_user_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .find(|u| *u != yo);
                let datos_otro = otro.map(|o| &c["user_data"][o.to_string()]);
                let nombre_otro = datos_otro
                    .and_then(|d| d["display_name"].as_str().or(d["combined_name"].as_str()))
                    .map(str::to_string);
                let nombre = match c["name"].as_str() {
                    Some(n) if !n.is_empty() && c["type"] != "one_to_one" => n.to_string(),
                    _ => nombre_otro.unwrap_or_else(|| "Chat".into()),
                };
                let previa = c["preview_message"]["content"]
                    .as_str()
                    .or(c["messages"][0]["content"].as_str())
                    .unwrap_or_default()
                    .to_string();
                Some(Conversacion {
                    id,
                    nombre,
                    no_leidos: c["unread_message_count"].as_u64().unwrap_or(0),
                    previa,
                    otro: if c["type"] == "one_to_one" { otro } else { None },
                })
            })
            .collect())
    }

    pub async fn mensajes(&self, conversacion: &str) -> Result<Vec<Mensaje>> {
        let url = reqwest::Url::parse_with_params(
            &format!("{CHAT}/get-conversation-messages"),
            &[("conversation_id", conversacion)],
        )?;
        let v = self.get(url.as_str()).await?;
        let mut m: Vec<Mensaje> = v["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m["is_deleted"] != true)
            .map(|m| Mensaje {
                texto: m["content"].as_str().unwrap_or_default().to_string(),
                autor: m["sender_user_id"].as_u64().unwrap_or(0),
                fecha: m["created_at"].as_str().unwrap_or_default().to_string(),
            })
            .collect();
        // Fechas ISO: el orden alfabético es el cronológico.
        m.sort_by(|a, b| a.fecha.cmp(&b.fecha));
        Ok(m)
    }

    pub async fn enviar_mensaje(&self, conversacion: &str, texto: &str) -> Result<()> {
        self.post(
            &format!("{CHAT}/send-messages"),
            Some(json!({ "conversation_id": conversacion, "messages": [{ "content": texto }] })),
            "No se pudo enviar el mensaje",
        )
        .await?;
        Ok(())
    }

    /// Abre (o crea) un chat privado con un usuario y devuelve su id.
    pub async fn chat_con(&self, usuario: u64) -> Result<String> {
        let v = self
            .post(
                &format!("{CHAT}/create-conversations"),
                Some(json!({
                    "conversations": [{ "type": "one_to_one", "participant_user_ids": [usuario] }],
                    "include_user_data": true,
                })),
                "No se pudo abrir el chat",
            )
            .await?;
        v["conversations"][0]["id"]
            .as_str()
            .map(str::to_string)
            .context("Roblox no devolvió el chat (puede que no podáis chatear: solo amigos y según la edad)")
    }

    /// Cierra la sesión en Roblox (invalida la cookie) y la borra del equipo.
    pub async fn cerrar(&self) {
        let _ = self.post("https://auth.roblox.com/v2/logout", None, "").await;
        borrar_cookie();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorias_sin_tipos_repetidos() {
        let mut vistos = std::collections::HashSet::new();
        for c in Categoria::TODAS {
            for t in c.tipos() {
                assert!(vistos.insert(*t), "tipo {t} repetido");
            }
        }
    }

    /// Prueba real: crea un código de inicio de sesión y lo cancela.
    #[tokio::test]
    #[ignore]
    async fn codigo_real() {
        let http = crate::roblox::cliente_http().unwrap();
        let csrf = Mutex::new(String::new());
        let c = crear_codigo(&http, &csrf).await.unwrap();
        assert_eq!(c.codigo.len(), 6);
        assert_eq!(estado_codigo(&http, &c, &csrf).await.unwrap(), EstadoCodigo::Creado);
        cancelar_codigo(&http, &c, &csrf).await;
    }
}
