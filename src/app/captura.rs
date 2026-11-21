//! Capturas de prueba (solo en compilaciones de depuración).

use super::*;

#[cfg(debug_assertions)]
impl App {
    /// RUST_LAUNCHER_PAGINA = inicio | buscar:<texto> | info:<universeId> | avatar | ajustes | lanzando
    /// RUST_LAUNCHER_MOVER  = secuencia de flechas (d, i, a, b) aplicada al arrancar
    /// RUST_LAUNCHER_MANDO  = mostrar las pistas del mando
    pub(super) fn preparar_captura(&mut self) {
        if let Ok(p) = std::env::var("RUST_LAUNCHER_PAGINA") {
            match p.split_once(':') {
                Some(("buscar", q)) => {
                    self.pestana = Pestana::Buscar;
                    self.consulta = q.to_string();
                    self.buscar();
                }
                Some(("info", id)) => {
                    if let Ok(id) = id.parse() {
                        self.abrir_info(id);
                    }
                }
                Some(("dialogo", d)) => self.dialogos.push(if d == "consola" { Dialogo::Consola } else { Dialogo::Sesion }),
                _ => match p.as_str() {
                    "avatar" => self.cambiar_pestana(Pestana::Avatar),
                    "perfil" => self.cambiar_pestana(Pestana::Perfil),
                    "mercado" => self.cambiar_pestana(Pestana::Mercado),
                    "social" => self.cambiar_pestana(Pestana::Social),
                    "teclado" => {
                        self.cfg.modo = Modo::Consola;
                        self.mando = true;
                        self.pestana = Pestana::Buscar;
                        self.teclado = Some(Teclado::nuevo(Destino::Juegos, "Blox F".into()));
                    }
                    "login" => self.abrir_login(),
                    "animacion" => {
                        // Que la captura caiga en el segundo 1,4 de la animación.
                        let espera: f32 =
                            std::env::var("RUST_LAUNCHER_ESPERA").ok().and_then(|s| s.parse().ok()).unwrap_or(6.0);
                        self.animacion_consola = Some(Instant::now() + Duration::from_secs_f32(espera - 1.4));
                    }
                    "ajustes" => self.pestana = Pestana::Ajustes,
                    "lanzando" => {
                        self.fondo_a(994732206);
                        self.lanzamiento = Some(Lanzamiento {
                            titulo: "Blox Fruits".into(),
                            progreso: Progreso::Descargando { hechos: 7, total: 20 },
                            resultado: None,
                            fin: None,
                            fraccion: 0.4,
                            hito: Some("Preparando la experiencia"),
                            abierto: None,
                        });
                    }
                    _ => {}
                },
            }
        }
        if let Ok(n) = std::env::var("RUST_LAUNCHER_MOVER") {
            for c in n.chars() {
                self.acciones.push(match c {
                    'd' => Accion::Derecha,
                    'b' => Accion::Abajo,
                    'a' => Accion::Arriba,
                    _ => Accion::Izquierda,
                });
            }
        }
        if std::env::var_os("RUST_LAUNCHER_MANDO").is_some() {
            self.mando = true;
        }
        if std::env::var("RUST_LAUNCHER_MODO").is_ok_and(|m| m == "consola") {
            self.cfg.modo = Modo::Consola;
        }
    }

    pub(super) fn capturar(&mut self, ctx: &egui::Context) {
        let Some(ruta) = std::env::var_os("RUST_LAUNCHER_CAPTURA") else { return };
        let espera: f32 = std::env::var("RUST_LAUNCHER_ESPERA").ok().and_then(|s| s.parse().ok()).unwrap_or(6.0);
        if self.arranque.elapsed().as_secs_f32() < espera {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        }
        if self.fotogramas == u32::MAX {
            return;
        }
        self.fotogramas = u32::MAX;
        ctx.request_screenshot(move |img| {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save_with_format(&ruta, image::ImageFormat::Png);
            }
            std::process::exit(0);
        });
        ctx.request_repaint();
    }
}
