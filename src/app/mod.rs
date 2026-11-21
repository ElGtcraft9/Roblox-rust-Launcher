//! Interfaz de consola (ver DESIGN.md).
//!
//! Nada de la red bloquea un fotograma: cada carga corre en tokio y vuelve como
//! un `Msg` por un canal. El mando corre en su propio hilo y también llega como
//! `Msg`, así que la interfaz solo se redibuja cuando algo cambia.

use crate::acelerar;
use crate::api::{self, Articulo, CATEGORIAS_MERCADO, Detalle, Juego, Seccion, TipoImg, Usuario};
use crate::config::{self, AlJugar, ApiGrafica, Config, Modo};
use crate::imagenes::{Img, Imagenes};
use crate::mando::{self, Accion};
use crate::plataforma::{self, Progreso};
use crate::sesion::{self, Amigo, Conversacion, Mensaje, AvatarEditable, Categoria, Codigo, EstadoCodigo, Objeto, Presencia, Sesion, Traje};
use crate::tema::*;
use crate::teclado::{self, Destino, Teclado};
use crate::vigia::{self, Evento};
use eframe::egui::{
    self, Align2, Color32, CursorIcon, Frame, Id, Key, Margin, Order, Pos2, Rect, Sense, Stroke, StrokeKind,
    TextEdit, TextureHandle, Ui, Vec2,
    text::{LayoutJob, TextFormat, TextWrapping},
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::runtime::Handle;

mod ajustes;
mod avatar;
mod barras;
mod buscar;
mod campos;
mod capas;
#[cfg(debug_assertions)]
mod captura;
mod dibujo;
mod inicio;
mod mercado;
mod modos;
mod perfil;
mod social;
mod tareas;

use dibujo::*;


pub enum Msg {
    Inicio(Result<Vec<Seccion>, String>),
    Busqueda(u64, Result<Vec<Juego>, String>),
    Detalle(u64, Result<Detalle, String>),
    Imagen(Img, Option<egui::ColorImage>),
    Qr(String, Option<egui::ColorImage>),
    Plataforma { aviso: Option<String>, enlaces: bool },
    ProgresoFondo(Progreso),
    FinFondo(Result<(), String>),
    ProgresoLanzar(Progreso),
    FinLanzar(Result<(), String>),
    JuegoCerrado,
    /// Acción del mando (si la hay) y si queda algún mando conectado.
    Mando(Option<Accion>, bool),
    Sesion(Result<Arc<Sesion>, String>),
    Codigo(Result<Codigo, String>),
    EstadoCodigo(Result<EstadoCodigo, String>),
    Avatar(Result<AvatarEditable, String>),
    Trajes(Result<Vec<Traje>, String>),
    Inventario(Categoria, Result<Vec<Objeto>, String>),
    CambioAvatar(Result<(), String>),
    Amigos(Result<Vec<Amigo>, String>),
    Descripcion(Result<String, String>),
    /// Hito del registro del cliente mientras se abre el juego.
    Hito(f32, &'static str),
    Mercado(u64, Result<Vec<Articulo>, String>),
    Robux(Result<u64, String>),
    Usuarios(u64, Result<Vec<Usuario>, String>),
    Solicitud(u64, Result<(), String>),
    Conversaciones(Result<Vec<Conversacion>, String>),
    Mensajes(String, Result<Vec<Mensaje>, String>),
    Enviado(Result<(), String>),
    ChatAbierto(Result<(String, String), String>),
    Actualizacion(EstadoActualizacion),
}

/// Estado de la actualización del propio launcher.
#[derive(Clone, PartialEq)]
pub enum EstadoActualizacion {
    Nada,
    Buscando,
    AlDia,
    Descargando(String),
    /// Versión nueva instalada en el disco; se usa al reiniciar.
    Lista(String),
    Error(String),
}

/// Foco en la pantalla del mercado (modo consola).
#[derive(Clone, Copy, PartialEq, Eq)]
enum FocoMercado {
    Campo,
    Categoria(usize),
    Articulo(usize),
}

/// Foco en la pantalla social (modo consola).
#[derive(Clone, Copy, PartialEq, Eq)]
enum FocoSocial {
    Campo,
    Chat(usize),
    Persona(usize),
    Entrada,
}

/// Envía mensajes a la interfaz y la despierta.
#[derive(Clone)]
pub struct Enviador {
    tx: Sender<Msg>,
    ctx: egui::Context,
}

impl Enviador {
    pub fn enviar(&self, m: Msg) {
        let _ = self.tx.send(m);
        self.ctx.request_repaint();
    }
}

enum Carga<T> {
    Nada,
    Cargando,
    Listo(T),
    Error(String),
}

impl<T> Carga<T> {
    fn de(r: Result<T, String>) -> Self {
        match r {
            Ok(v) => Carga::Listo(v),
            Err(e) => Carga::Error(e),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pestana {
    Inicio,
    Buscar,
    Mercado,
    Social,
    Perfil,
    Avatar,
    Ajustes,
}

const PESTANAS: [(Pestana, &str); 7] = [
    (Pestana::Inicio, "Inicio"),
    (Pestana::Buscar, "Buscar"),
    (Pestana::Mercado, "Mercado"),
    (Pestana::Social, "Social"),
    (Pestana::Perfil, "Perfil"),
    (Pestana::Avatar, "Avatar"),
    (Pestana::Ajustes, "Ajustes"),
];

/// Preguntas que aparecen encima de todo, una a la vez.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialogo {
    /// Se ha conectado un mando estando en modo PC.
    Consola,
    /// Al arrancar sin sesión iniciada.
    Sesion,
}

struct Lanzamiento {
    titulo: String,
    progreso: Progreso,
    resultado: Option<Result<(), String>>,
    fin: Option<Instant>,
    /// Avance según el registro del cliente (0 a 1) y qué está haciendo.
    fraccion: f32,
    hito: Option<&'static str>,
    /// Cuándo arrancó el proceso del juego.
    abierto: Option<Instant>,
}

enum Login {
    Nada,
    Creando,
    Esperando { codigo: Codigo, estado: EstadoCodigo },
    Error(String),
}

/// Filas de la pantalla de ajustes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ajuste {
    Api,
    Msaa,
    Texturas,
    Hierba,
    Geometria,
    Cielo,
    Dpi,
    Modo,
    AlJugar,
    CerrarAlSalir,
    Precargar,
    Fondo,
    Enlaces,
    BorrarImagenes,
    ActualizarAuto,
    BuscarActualizacion,
    CerrarSesion,
    IniciarSesion,
    Web(&'static str, &'static str, &'static str),
}

fn texto_error(e: anyhow::Error) -> String {
    format!("{e:#}")
}

pub struct App {
    rt: Handle,
    http: reqwest::Client,
    tx: Enviador,
    rx: Receiver<Msg>,
    imgs: Imagenes,
    cfg: Config,

    pestana: Pestana,
    info: Option<u64>,
    detalles: HashMap<u64, Carga<Detalle>>,
    inicio: Vec<Seccion>,
    inicio_estado: Carga<()>,

    // Navegación de Inicio.
    fila: usize,
    cols: HashMap<String, usize>,
    /// Desplazamiento horizontal de cada estante.
    desplaz: HashMap<String, f32>,
    rueda: Vec2,
    /// Juego enfocado y desde cuándo (para precargar su ficha).
    foco_desde: Option<(u64, Instant)>,
    precargado: bool,
    actualizacion: EstadoActualizacion,
    ultima_busqueda: Option<Instant>,
    /// Transición entre pestañas: cuándo empezó y hacia qué lado.
    vista_anterior: (Pestana, bool),
    cambio_vista: Instant,
    dir_vista: f32,
    fondo: Option<u64>,
    fondo_previo: Option<u64>,
    fondo_cambio: Instant,
    tinte_previo: Color32,

    // Buscar.
    consulta: String,
    buscada: String,
    busqueda: Carga<Vec<Juego>>,
    busqueda_n: u64,
    foco_busqueda: Option<usize>,
    pedir_foco_texto: bool,

    // Sesión y avatar.
    sesion: Option<Arc<Sesion>>,
    sesion_comprobada: bool,
    login: Login,
    login_cancelar: Arc<AtomicBool>,
    qr: Option<(String, Option<TextureHandle>)>,
    categoria: Categoria,
    avatar: Carga<AvatarEditable>,
    trajes: Carga<Vec<Traje>>,
    inventario: HashMap<Categoria, Carga<Vec<Objeto>>>,
    guardando: bool,
    aviso_avatar: Option<String>,

    // Ajustes.
    fila_ajuste: usize,

    // Cliente y lanzamiento.
    fondo_progreso: Option<Progreso>,
    fondo_error: Option<String>,
    lanzamiento: Option<Lanzamiento>,
    jugando: Arc<AtomicBool>,
    minimizado: bool,
    cerrar_tras_lanzar: bool,
    aviso_cliente: Option<String>,
    enlaces_web: bool,
    error_ajustes: Option<String>,
    dns_calentado: Option<Instant>,

    mando: bool,
    acciones: Vec<Accion>,

    // Modos, diálogos y perfil.
    modo_aplicado: Option<Modo>,
    dialogos: Vec<Dialogo>,
    foco_dialogo: usize,
    preguntado_mando: bool,
    preguntado_login: bool,
    animacion_consola: Option<Instant>,
    login_abierto: bool,
    copiado: Option<Instant>,
    amigos: Carga<Vec<Amigo>>,
    descripcion: Carga<String>,

    teclado: Option<Teclado>,
    // Mercado.
    mercado_cat: usize,
    mercado_q: String,
    mercado: Carga<Vec<Articulo>>,
    mercado_n: u64,
    mercado_cache: HashMap<(usize, String), Vec<Articulo>>,
    foco_mercado: FocoMercado,
    articulo: Option<Articulo>,
    robux: Option<u64>,
    // Social.
    usuarios_q: String,
    usuarios: Carga<Vec<Usuario>>,
    usuarios_n: u64,
    solicitudes: HashMap<u64, Carga<()>>,
    conversaciones: Carga<Vec<Conversacion>>,
    chat: Option<(String, String)>,
    mensajes: Carga<Vec<Mensaje>>,
    borrador: String,
    enviando: bool,
    refresco_chat: Instant,
    foco_social: FocoSocial,
    arranque: Instant,
    fotogramas: u32,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        rt: Handle,
        red: tokio::task::JoinHandle<(reqwest::Client, tokio::task::JoinHandle<anyhow::Result<Vec<Seccion>>>)>,
        uri: Option<String>,
        arranque: Instant,
    ) -> Self {
        crate::perfil::marca("ventana y GL listos (App::new)");
        aplicar(&cc.egui_ctx);
        crate::perfil::marca("fuentes y tema aplicados");
        let (tx, rx) = channel();
        let tx = Enviador { tx, ctx: cc.egui_ctx.clone() };
        // Cliente y petición de portada se lanzaron antes de crear la ventana.
        let (http, portada) = rt.block_on(red).expect("arranque de red");
        let mut cfg = Config::cargar();

        if cfg.abrir_enlaces_web.is_none() {
            let activar = plataforma::ENLACES_POR_DEFECTO;
            if activar {
                let _ = plataforma::registrar(true);
            }
            cfg.abrir_enlaces_web = Some(activar);
            cfg.guardar();
        }

        crate::perfil::marca("config cargada");
        let inicio = config::cargar_inicio();
        crate::perfil::marca("portada en caché leída");
        let mut app = Self {
            imgs: Imagenes::new(rt.clone(), http.clone(), tx.clone()),
            rt,
            http,
            tx,
            rx,
            pestana: Pestana::Inicio,
            info: None,
            detalles: HashMap::new(),
            inicio_estado: if inicio.is_empty() { Carga::Cargando } else { Carga::Listo(()) },
            inicio,
            fila: 0,
            cols: HashMap::new(),
            desplaz: HashMap::new(),
            rueda: Vec2::ZERO,
            foco_desde: None,
            precargado: false,
            actualizacion: EstadoActualizacion::Nada,
            ultima_busqueda: None,
            vista_anterior: (Pestana::Inicio, false),
            cambio_vista: Instant::now() - Duration::from_secs(1),
            dir_vista: 1.0,
            fondo: None,
            fondo_previo: None,
            fondo_cambio: Instant::now(),
            tinte_previo: FONDO_NEUTRO,
            consulta: String::new(),
            buscada: String::new(),
            busqueda: Carga::Nada,
            busqueda_n: 0,
            foco_busqueda: None,
            pedir_foco_texto: false,
            sesion: None,
            sesion_comprobada: false,
            login: Login::Nada,
            login_cancelar: Arc::new(AtomicBool::new(false)),
            qr: None,
            categoria: Categoria::Trajes,
            avatar: Carga::Nada,
            trajes: Carga::Nada,
            inventario: HashMap::new(),
            guardando: false,
            aviso_avatar: None,
            fila_ajuste: 0,
            fondo_progreso: None,
            fondo_error: None,
            lanzamiento: None,
            jugando: Arc::new(AtomicBool::new(false)),
            minimizado: false,
            cerrar_tras_lanzar: uri.is_some(),
            aviso_cliente: None,
            enlaces_web: cfg.abrir_enlaces_web.unwrap_or(false),
            error_ajustes: None,
            dns_calentado: None,
            mando: false,
            acciones: Vec::new(),
            modo_aplicado: None,
            dialogos: Vec::new(),
            foco_dialogo: 0,
            preguntado_mando: false,
            preguntado_login: false,
            animacion_consola: None,
            login_abierto: false,
            copiado: None,
            amigos: Carga::Nada,
            descripcion: Carga::Nada,
            teclado: None,
            mercado_cat: 0,
            mercado_q: String::new(),
            mercado: Carga::Nada,
            mercado_n: 0,
            mercado_cache: HashMap::new(),
            foco_mercado: FocoMercado::Articulo(0),
            articulo: None,
            robux: None,
            usuarios_q: String::new(),
            usuarios: Carga::Nada,
            usuarios_n: 0,
            solicitudes: HashMap::new(),
            conversaciones: Carga::Nada,
            chat: None,
            mensajes: Carga::Nada,
            borrador: String::new(),
            enviando: false,
            refresco_chat: Instant::now(),
            foco_social: FocoSocial::Campo,
            arranque,
            fotogramas: 0,
            cfg,
        };

        {
            let tx = app.tx.clone();
            app.rt.spawn(async move {
                let r = portada.await.map_err(anyhow::Error::from).and_then(|r| r);
                tx.enviar(Msg::Inicio(r.map_err(texto_error)));
            });
        }
        app.comprobar_plataforma();
        app.abrir_sesion_guardada();
        mando::iniciar(app.tx.clone());
        if uri.is_none() {
            app.mantenimiento_en_fondo();
        }
        if let Some(uri) = uri {
            app.lanzar("Abriendo la experiencia".into(), uri);
        }
        #[cfg(debug_assertions)]
        app.preparar_captura();
        app
    }

    // ---------- Estado derivado ----------

    fn estantes(&self) -> Vec<(String, Vec<Juego>)> {
        const SEGUIR: &str = "Seguir jugando";
        let mut v = Vec::new();
        // Lo jugado desde este launcher primero; después el historial de la cuenta.
        let mut seguir = self.cfg.recientes.clone();
        for s in self.inicio.iter().filter(|s| s.titulo == SEGUIR) {
            for j in &s.juegos {
                if !seguir.iter().any(|x| x.universe_id == j.universe_id) {
                    seguir.push(j.clone());
                }
            }
        }
        if !seguir.is_empty() {
            v.push((SEGUIR.to_string(), seguir));
        }
        if !self.cfg.favoritos.is_empty() {
            v.push(("Favoritos".to_string(), self.cfg.favoritos.clone()));
        }
        for s in self.inicio.iter().filter(|s| s.titulo != SEGUIR) {
            v.push((s.titulo.clone(), s.juegos.clone()));
        }
        v
    }

    fn fondo_a(&mut self, universe_id: u64) {
        if self.fondo != Some(universe_id) {
            self.tinte_previo = self.tinte_actual();
            self.fondo_previo = self.fondo;
            self.fondo = Some(universe_id);
            self.fondo_cambio = Instant::now();
        }
    }

    fn tinte_de(&self, id: Option<u64>) -> Color32 {
        id.and_then(|id| self.imgs.color(id)).map(tinte).unwrap_or(FONDO_NEUTRO)
    }

    fn tinte_actual(&self) -> Color32 {
        mezclar(self.tinte_previo, self.tinte_de(self.fondo), self.progreso_fundido())
    }

    fn progreso_fundido(&self) -> f32 {
        (self.fondo_cambio.elapsed().as_secs_f32() / 0.25).min(1.0)
    }

    fn img(&mut self, tipo: TipoImg, id: u64) -> Option<TextureHandle> {
        self.imgs.get((tipo, id))
    }

    fn cambiar_pestana(&mut self, p: Pestana) {
        self.pestana = p;
        self.info = None;
        match p {
            Pestana::Buscar => {
                if self.cfg.modo == Modo::Consola {
                    if self.consulta.is_empty() {
                        self.teclado = Some(Teclado::nuevo(Destino::Juegos, String::new()));
                    }
                } else {
                    self.pedir_foco_texto = true;
                }
            }
            Pestana::Mercado => {
                if matches!(self.mercado, Carga::Nada) {
                    self.cargar_mercado();
                }
                self.cargar_robux();
            }
            Pestana::Social => self.cargar_conversaciones(),
            Pestana::Avatar => {
                if self.sesion.is_some() {
                    if matches!(self.avatar, Carga::Nada | Carga::Error(_)) {
                        self.cargar_avatar();
                    }
                    self.cargar_categoria(self.categoria);
                }
            }
            Pestana::Perfil => self.cargar_perfil(),
            _ => {}
        }
    }

    fn pestana_relativa(&mut self, d: i32) {
        let i = PESTANAS.iter().position(|(p, _)| *p == self.pestana).unwrap_or(0) as i32;
        let n = PESTANAS.len() as i32;
        self.cambiar_pestana(PESTANAS[((i + d).rem_euclid(n)) as usize].0);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let t_fotograma = Instant::now();
        let ctx = ui.ctx().clone();
        if crate::perfil::activo() {
            if self.fotogramas == 0 {
                crate::perfil::marca("primer fotograma (empieza)");
            }
            if let Some(d) = crate::perfil::duracion()
                && self.arranque.elapsed().as_secs_f64() > d
            {
                crate::perfil::resumen();
                std::process::exit(0);
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.procesar(&ctx);
        self.fotogramas = self.fotogramas.saturating_add(1);
        if self.fotogramas == 2 && std::env::var_os("RUST_LAUNCHER_MEDIR").is_some() {
            println!("primer fotograma: {} ms", self.arranque.elapsed().as_millis());
            std::process::exit(0);
        }
        #[cfg(debug_assertions)]
        self.capturar(&ctx);

        self.aplicar_modo(&ctx);
        self.actualizar_solo(&ctx);
        self.precargar_pestanas(&ctx);
        self.tras_lanzar(&ctx);
        self.leer_teclado(&ctx);
        let todas = std::mem::take(&mut self.acciones);
        // Las capas (animación, diálogo, inicio de sesión) se quedan la entrada.
        let (acciones, para_capa) = if self.hay_capa() { (Vec::new(), todas) } else { (todas, Vec::new()) };

        let pantalla = ctx.content_rect();
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            self.pintar_fondo(ui, pantalla);
            if self.lanzamiento.is_some() {
                self.capa_lanzamiento(ui, pantalla, &acciones);
                return;
            }
            self.barra_superior(ui, pantalla);
            let k = self.transicion();
            if k < 1.0 {
                ui.ctx().request_repaint();
            }
            // Desplazamiento corto en la dirección de la pestaña nueva.
            let vista = pantalla.translate(Vec2::new(self.dir_vista * 56.0 * (1.0 - k), 0.0));
            ui.scope(|ui| {
                ui.multiply_opacity(k);
                if let Some(id) = self.info {
                    self.pantalla_info(ui, vista, id, &acciones);
                } else {
                    match self.pestana {
                        Pestana::Inicio => self.pantalla_inicio(ui, vista, &acciones),
                        Pestana::Buscar => self.pantalla_buscar(ui, vista, &acciones),
                        Pestana::Mercado => self.pantalla_mercado(ui, vista, &acciones),
                        Pestana::Social => self.pantalla_social(ui, vista, &acciones),
                        Pestana::Perfil => self.pantalla_perfil(ui, vista),
                        Pestana::Avatar => self.pantalla_avatar(ui, vista),
                        Pestana::Ajustes => self.pantalla_ajustes(ui, vista, &acciones),
                    }
                }
            });
            self.barra_inferior(ui, pantalla);
        });

        let fondo_teclado = mezclar(self.tinte_actual(), Color32::WHITE, 0.06);
        let mando = self.mando;
        if let Some(t) = self.teclado.as_mut() {
            if let Some(r) = t.mostrar(&ctx, pantalla, &para_capa, mando, fondo_teclado) {
                self.teclado = None;
                self.al_teclado(r);
            }
        } else if self.articulo.is_some() {
            self.capa_articulo(&ctx, pantalla, &para_capa);
        } else if self.animacion_consola.is_some() {
            self.capa_animacion(&ctx, pantalla);
        } else if !self.dialogos.is_empty() && !self.login_abierto {
            self.capa_dialogo(&ctx, pantalla, &para_capa);
        } else if self.login_abierto {
            self.capa_login(&ctx, pantalla, &para_capa);
        }

        if self.lanzamiento.is_none() {
            for a in &acciones {
                match a {
                    Accion::PestanaAnterior => self.pestana_relativa(-1),
                    Accion::PestanaSiguiente => self.pestana_relativa(1),
                    _ => {}
                }
            }
        }
        if self.progreso_fundido() < 1.0 {
            ctx.request_repaint();
        }
        // El reloj cambia cada minuto.
        let seg = chrono::Local::now().timestamp().rem_euclid(60) as u64;
        ctx.request_repaint_after(Duration::from_secs(60 - seg));
        self.imgs.despachar();
        if self.fotogramas > 6 {
            crate::perfil::fotograma(t_fotograma.elapsed().as_micros() as u64);
        } else if crate::perfil::activo() && self.fotogramas <= 6 {
            crate::perfil::marca(&format!(
                "fotograma {} terminado ({:.1} ms)",
                self.fotogramas,
                t_fotograma.elapsed().as_secs_f64() * 1000.0
            ));
        }
    }
}

impl App {
    fn leer_teclado(&mut self, ctx: &egui::Context) {
        let escribiendo = ctx.egui_wants_keyboard_input();
        // Modo PC: cada «clic» de la rueda es un paso arriba o abajo (con
        // Mayús o en horizontal, a los lados). Así se navega igual que con las flechas.
        if self.cfg.modo == Modo::Pc && self.teclado.is_none() {
            let mut rueda = self.rueda;
            ctx.input(|i| {
                for e in &i.events {
                    if let egui::Event::MouseWheel { unit, delta, modifiers, .. } = e {
                        let k = match unit {
                            egui::MouseWheelUnit::Line => 50.0,
                            egui::MouseWheelUnit::Page => 200.0,
                            egui::MouseWheelUnit::Point => 1.0,
                        };
                        let d = *delta * k;
                        rueda += if modifiers.shift { Vec2::new(d.y, 0.0) } else { d };
                    }
                }
            });
            const PASO: f32 = 50.0;
            while rueda.y >= PASO {
                self.acciones.push(Accion::Arriba);
                rueda.y -= PASO;
            }
            while rueda.y <= -PASO {
                self.acciones.push(Accion::Abajo);
                rueda.y += PASO;
            }
            while rueda.x >= PASO {
                self.acciones.push(Accion::Izquierda);
                rueda.x -= PASO;
            }
            while rueda.x <= -PASO {
                self.acciones.push(Accion::Derecha);
                rueda.x += PASO;
            }
            self.rueda = rueda;
        }
        ctx.input(|i| {
            for (k, a) in [
                (Key::ArrowUp, Accion::Arriba),
                (Key::ArrowDown, Accion::Abajo),
                (Key::ArrowLeft, Accion::Izquierda),
                (Key::ArrowRight, Accion::Derecha),
                (Key::Enter, Accion::Aceptar),
                (Key::Escape, Accion::Volver),
            ] {
                // Mientras se escribe, solo Escape y Abajo salen del campo.
                let permitido = !escribiendo || matches!(a, Accion::Volver | Accion::Abajo);
                if permitido && i.key_pressed(k) {
                    self.acciones.push(a);
                }
            }
            if !escribiendo && self.teclado.is_none() {
                for (k, a) in [
                    (Key::I, Accion::Info),
                    (Key::F, Accion::Favorito),
                    (Key::Q, Accion::PestanaAnterior),
                    (Key::E, Accion::PestanaSiguiente),
                ] {
                    if i.key_pressed(k) {
                        self.acciones.push(a);
                    }
                }
            }
        });
    }

    fn tras_lanzar(&mut self, ctx: &egui::Context) {
        let Some(l) = &self.lanzamiento else { return };
        let listo = l.fin.is_some_and(|f| Instant::now() >= f);
        // Si el registro no llega (otra versión del cliente), la capa se va sola.
        let vencido = l.abierto.is_some_and(|t| t.elapsed() > Duration::from_secs(45));
        if l.abierto.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if !(listo || vencido) {
            return;
        }
        self.lanzamiento = None;
        if self.cerrar_tras_lanzar {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        match self.cfg.al_jugar {
            AlJugar::Minimizar => {
                self.minimizado = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            AlJugar::Cerrar => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            AlJugar::Nada => {}
        }
    }

}
