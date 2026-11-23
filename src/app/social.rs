//! Social: buscar personas, solicitudes de amistad y chat.

use super::*;

impl App {
    // ---------- Social ----------

    pub(super) fn buscar_usuarios(&mut self) {
        let q = self.usuarios_q.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.usuarios_n += 1;
        let n = self.usuarios_n;
        self.usuarios = Carga::Cargando;
        self.chat = None;
        let http = self.http.clone();
        self.tarea(async move { Msg::Usuarios(n, api::buscar_usuarios(&http, &q).await.map_err(texto_error)) });
    }

    pub(super) fn solicitar_amistad(&mut self, id: u64) {
        let Some(s) = self.sesion.clone() else { return };
        if matches!(self.solicitudes.get(&id), Some(Carga::Cargando | Carga::Listo(_))) {
            return;
        }
        self.solicitudes.insert(id, Carga::Cargando);
        self.tarea(async move { Msg::Solicitud(id, s.solicitar_amistad(id).await.map_err(texto_error)) });
    }

    pub(super) fn cargar_conversaciones(&mut self) {
        let Some(s) = self.sesion.clone() else { return };
        if matches!(self.conversaciones, Carga::Nada) {
            self.conversaciones = Carga::Cargando;
        }
        self.tarea(async move { Msg::Conversaciones(s.conversaciones().await.map_err(texto_error)) });
    }

    pub(super) fn cargar_mensajes(&mut self) {
        let (Some(s), Some((id, _))) = (self.sesion.clone(), self.chat.clone()) else { return };
        self.refresco_chat = Instant::now();
        self.tarea(async move {
            let r = s.mensajes(&id).await.map_err(texto_error);
            Msg::Mensajes(id, r)
        });
    }

    pub(super) fn abrir_chat(&mut self, c: &Conversacion) {
        self.chat = Some((c.id.clone(), c.nombre.clone()));
        self.mensajes = Carga::Cargando;
        self.cargar_mensajes();
    }

    pub(super) fn abrir_chat_con(&mut self, u: &Usuario) {
        let Some(s) = self.sesion.clone() else { return };
        self.mensajes = Carga::Cargando;
        let (id, nombre) = (u.id, u.nombre_visible.clone());
        self.tarea(async move { Msg::ChatAbierto(s.chat_con(id).await.map(|c| (c, nombre)).map_err(texto_error)) });
    }

    pub(super) fn enviar_borrador(&mut self) {
        let texto = self.borrador.trim().to_string();
        let (Some(s), Some((id, _))) = (self.sesion.clone(), self.chat.clone()) else { return };
        if texto.is_empty() || self.enviando {
            return;
        }
        self.enviando = true;
        self.tarea(async move { Msg::Enviado(s.enviar_mensaje(&id, &texto).await.map_err(texto_error)) });
    }

    pub(super) fn pantalla_social(&mut self, ui: &mut Ui, pantalla: Rect, acciones: &[Accion]) {
        let Some(sesion) = self.sesion.clone() else {
            return self.invitar_login(ui, pantalla, "Inicia sesión para chatear y añadir amigos");
        };
        let consola = self.cfg.modo == Modo::Consola;
        // El chat abierto se refresca cada 5 segundos.
        if self.chat.is_some() {
            if self.refresco_chat.elapsed() > Duration::from_secs(5) {
                self.cargar_mensajes();
            }
            ui.ctx().request_repaint_after(Duration::from_secs(5));
        }
        let convs: Vec<Conversacion> = match &self.conversaciones {
            Carga::Listo(l) => l.clone(),
            _ => Vec::new(),
        };
        let personas: Vec<Usuario> = match &self.usuarios {
            Carga::Listo(l) => l.clone(),
            _ => Vec::new(),
        };

        // Navegación.
        let mut foco_s = self.foco_social;
        let mut abrir_conv: Option<usize> = None;
        for a in acciones {
            foco_s = match (a, foco_s) {
                (Accion::Abajo, FocoSocial::Campo) if !convs.is_empty() => FocoSocial::Chat(0),
                (Accion::Abajo, FocoSocial::Chat(i)) => FocoSocial::Chat((i + 1).min(convs.len().saturating_sub(1))),
                (Accion::Arriba, FocoSocial::Chat(0)) => FocoSocial::Campo,
                (Accion::Arriba, FocoSocial::Chat(i)) => FocoSocial::Chat(i - 1),
                (Accion::Derecha, FocoSocial::Campo | FocoSocial::Chat(_)) if self.chat.is_some() => FocoSocial::Entrada,
                (Accion::Derecha, FocoSocial::Campo | FocoSocial::Chat(_)) if !personas.is_empty() => FocoSocial::Persona(0),
                (Accion::Izquierda, FocoSocial::Entrada | FocoSocial::Persona(_)) => FocoSocial::Campo,
                (Accion::Abajo, FocoSocial::Persona(i)) => FocoSocial::Persona((i + 1).min(personas.len().saturating_sub(1))),
                (Accion::Arriba, FocoSocial::Persona(i)) => FocoSocial::Persona(i.saturating_sub(1)),
                (Accion::Aceptar, FocoSocial::Campo) => {
                    self.abrir_teclado(Destino::Usuarios);
                    foco_s
                }
                (Accion::Aceptar, FocoSocial::Entrada) => {
                    self.abrir_teclado(Destino::Chat);
                    foco_s
                }
                (Accion::Aceptar, FocoSocial::Chat(i)) => {
                    abrir_conv = Some(i);
                    foco_s
                }
                (Accion::Aceptar, FocoSocial::Persona(i)) => {
                    if let Some(u) = personas.get(i) {
                        self.solicitar_amistad(u.id);
                    }
                    foco_s
                }
                (Accion::Info, FocoSocial::Persona(i)) => {
                    if let Some(u) = personas.get(i) {
                        self.abrir_chat_con(u);
                    }
                    FocoSocial::Entrada
                }
                (_, f) => f,
            };
        }

        let x0 = pantalla.left() + MARGEN;
        let izq = (pantalla.width() * 0.3).clamp(280.0, 380.0);
        let p = ui.painter().clone();

        // --- Columna izquierda: buscar personas y chats ---
        let campo_r = Rect::from_min_size(Pos2::new(x0, pantalla.top() + 96.0), Vec2::new(izq, 52.0));
        let c = self.campo(ui, campo_r, Destino::Usuarios, "Buscar personas", consola && foco_s == FocoSocial::Campo);
        if c.enviado {
            self.buscar_usuarios();
        }
        let mut y = campo_r.bottom() + 28.0;
        p.text(Pos2::new(x0, y), Align2::LEFT_TOP, "Chats", estrecha(escala::SECCION), TEXTO);
        y += 40.0;
        let zona_izq = Rect::from_min_max(Pos2::new(x0 - 8.0, y), Pos2::new(x0 + izq + 8.0, pantalla.bottom() - 60.0));
        match &self.conversaciones {
            Carga::Error(e) => {
                let g = galley(ui, &trf("No se pudieron cargar los chats. {}", &[e]), cuerpo(escala::ETIQUETA), ERROR, izq, 4);
                p.galley(Pos2::new(x0, y), g, ERROR);
            }
            Carga::Listo(l) if l.is_empty() => {
                p.text(Pos2::new(x0, y), Align2::LEFT_TOP, "Aún no tienes chats.", cuerpo(escala::CUERPO), TEXTO_2);
            }
            Carga::Listo(_) => {
                let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(zona_izq));
                let abierto = self.chat.as_ref().map(|(id, _)| id.clone());
                egui::ScrollArea::vertical().id_salt("chats").show(&mut hijo, |ui| {
                    for (i, cv) in convs.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(zona_izq.width() - 8.0, 64.0), Sense::click());
                        let r = r.shrink2(Vec2::new(4.0, 2.0));
                        let elegido = abierto.as_deref() == Some(cv.id.as_str());
                        if elegido || resp.hovered() {
                            ui.painter().rect_filled(r, RADIO, cristal());
                        }
                        if consola && foco_s == FocoSocial::Chat(i) {
                            ui.painter().rect_stroke(r.expand(2.0), RADIO + 2, Stroke::new(2.0, FOCO), StrokeKind::Inside);
                            ui.scroll_to_rect(r, None);
                        }
                        let cc = Pos2::new(r.left() + 30.0, r.center().y);
                        ui.painter().circle_filled(cc, 22.0, cristal_fuerte());
                        if let Some(o) = cv.otro
                            && let Some(t) = self.imgs.get((TipoImg::Cabeza, o))
                        {
                            egui::Image::from_texture(egui::load::SizedTexture::from_handle(&t))
                                .corner_radius(22)
                                .paint_at(ui, Rect::from_center_size(cc, Vec2::splat(44.0)));
                        }
                        let xt = r.left() + 64.0;
                        let ancho_t = r.right() - xt - 40.0;
                        let g = galley(ui, &cv.nombre, semi(16.0), TEXTO, ancho_t, 1);
                        ui.painter().galley(Pos2::new(xt, r.top() + 10.0), g, TEXTO);
                        let g = galley(ui, &cv.previa, medio(13.0), TEXTO_2, ancho_t, 1);
                        ui.painter().galley(Pos2::new(xt, r.top() + 34.0), g, TEXTO_2);
                        if cv.no_leidos > 0 {
                            let cb = Pos2::new(r.right() - 18.0, r.center().y);
                            ui.painter().circle_filled(cb, 11.0, FOCO);
                            ui.painter().text(cb, Align2::CENTER_CENTER, cv.no_leidos.min(99).to_string(), semi(12.0), TINTA);
                        }
                        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            abrir_conv = Some(i);
                        }
                    }
                });
            }
            _ => {
                p.text(Pos2::new(x0, y), Align2::LEFT_TOP, "Cargando chats…", cuerpo(escala::CUERPO), TEXTO_2);
            }
        }
        if let Some(i) = abrir_conv
            && let Some(cv) = convs.get(i)
        {
            self.abrir_chat(cv);
        }

        // --- Columna derecha: chat abierto o resultados de búsqueda ---
        let xr = x0 + izq + 48.0;
        let ancho_r = pantalla.right() - MARGEN - xr;
        let arriba = pantalla.top() + 96.0;
        if let Some((_, nombre)) = self.chat.clone() {
            p.text(Pos2::new(xr, arriba + 26.0), Align2::LEFT_CENTER, &nombre, estrecha(escala::SECCION), TEXTO);
            let (r_cerrar, _) = boton(ui, Pos2::new(pantalla.right() - MARGEN - 110.0, arriba), "Cerrar", EstiloBoton::Secundario, "chat_cerrar");
            if r_cerrar.clicked() {
                self.chat = None;
                self.foco_social = FocoSocial::Campo;
                return;
            }
            let entrada = Rect::from_min_size(Pos2::new(xr, pantalla.bottom() - 60.0 - 52.0 - 8.0), Vec2::new(ancho_r - 132.0, 52.0));
            let zona = Rect::from_min_max(Pos2::new(xr, arriba + 68.0), Pos2::new(xr + ancho_r, entrada.top() - 16.0));
            match &self.mensajes {
                Carga::Listo(m) => {
                    let m = m.clone();
                    let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(zona));
                    egui::ScrollArea::vertical().id_salt(("mensajes", nombre.clone())).stick_to_bottom(true).show(&mut hijo, |ui| {
                        ui.set_width(zona.width() - 12.0);
                        if m.is_empty() {
                            ui.label(egui::RichText::new("No hay mensajes todavía. Escribe el primero.").color(TEXTO_2));
                        }
                        for msg in &m {
                            let mio = msg.autor == sesion.usuario.id;
                            let max = zona.width() * 0.7;
                            let fuente = cuerpo(escala::CUERPO);
                            let texto = limpiar(ui, &msg.texto, &fuente);
                            let g = galley(ui, &texto, fuente, if mio { TINTA } else { TEXTO }, max - 28.0, 40);
                            let tam = g.size() + Vec2::new(28.0, 18.0);
                            let (fila, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), tam.y + 8.0), Sense::hover());
                            let x = if mio { fila.right() - tam.x } else { fila.left() };
                            let burbuja = Rect::from_min_size(Pos2::new(x, fila.top()), tam);
                            ui.painter().rect_filled(burbuja, 12, if mio { FOCO } else { cristal() });
                            ui.painter().galley(burbuja.min + Vec2::new(14.0, 9.0), g, TEXTO);
                        }
                    });
                }
                Carga::Error(e) => {
                    let g = galley(ui, e, cuerpo(escala::CUERPO), ERROR, ancho_r, 4);
                    p.galley(zona.min, g, ERROR);
                }
                _ => {
                    p.text(zona.min, Align2::LEFT_TOP, "Cargando mensajes…", cuerpo(escala::CUERPO), TEXTO_2);
                }
            }
            let c = self.campo(ui, entrada, Destino::Chat, "Escribe un mensaje", consola && foco_s == FocoSocial::Entrada);
            let texto_boton = if self.enviando { "Enviando…" } else { "Enviar" };
            let (r_env, _) = boton(ui, Pos2::new(entrada.right() + 12.0, entrada.top()), texto_boton, EstiloBoton::Principal, "chat_enviar");
            if c.enviado || r_env.clicked() {
                self.enviar_borrador();
                if let Some(r) = c.resp {
                    r.request_focus();
                }
            }
        } else {
            let mut y = arriba;
            match &self.usuarios {
                Carga::Nada => {
                    let g = galley(
                        ui,
                        "Busca a alguien por su nombre de usuario para enviarle una solicitud de amistad, o elige un chat de la izquierda.",
                        cuerpo(escala::CUERPO),
                        TEXTO_2,
                        ancho_r.min(560.0),
                        3,
                    );
                    p.galley(Pos2::new(xr, y + 14.0), g, TEXTO_2);
                }
                Carga::Cargando => {
                    p.text(Pos2::new(xr, y + 14.0), Align2::LEFT_TOP, "Buscando…", cuerpo(escala::CUERPO), TEXTO_2);
                }
                Carga::Error(e) => {
                    let g = galley(ui, e, cuerpo(escala::CUERPO), ERROR, ancho_r, 3);
                    p.galley(Pos2::new(xr, y + 14.0), g, ERROR);
                }
                Carga::Listo(l) if l.is_empty() => {
                    p.text(Pos2::new(xr, y + 14.0), Align2::LEFT_TOP, "No se encontró a nadie con ese nombre.", cuerpo(escala::CUERPO), TEXTO_2);
                }
                Carga::Listo(_) => {
                    p.text(Pos2::new(xr, y), Align2::LEFT_TOP, "Personas", estrecha(escala::SECCION), TEXTO);
                    y += 44.0;
                    let zona = Rect::from_min_max(Pos2::new(xr - 8.0, y), Pos2::new(pantalla.right() - MARGEN + 8.0, pantalla.bottom() - 60.0));
                    let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(zona));
                    let mut accion: Option<(usize, bool)> = None;
                    egui::ScrollArea::vertical().id_salt("personas").show(&mut hijo, |ui| {
                        for (i, u) in personas.iter().enumerate() {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(zona.width() - 12.0, 76.0), Sense::hover());
                            let r = r.shrink2(Vec2::new(4.0, 3.0));
                            if consola && foco_s == FocoSocial::Persona(i) {
                                ui.painter().rect_filled(r, RADIO, cristal());
                                ui.painter().rect_stroke(r.expand(2.0), RADIO + 2, Stroke::new(2.0, FOCO), StrokeKind::Inside);
                                ui.scroll_to_rect(r, None);
                            }
                            let cc = Pos2::new(r.left() + 34.0, r.center().y);
                            ui.painter().circle_filled(cc, 26.0, cristal_fuerte());
                            if let Some(t) = self.imgs.get((TipoImg::Cabeza, u.id)) {
                                egui::Image::from_texture(egui::load::SizedTexture::from_handle(&t))
                                    .corner_radius(26)
                                    .paint_at(ui, Rect::from_center_size(cc, Vec2::splat(52.0)));
                            }
                            let xt = r.left() + 76.0;
                            let g = galley(ui, &u.nombre_visible, semi(17.0), TEXTO, r.width() - 400.0, 1);
                            ui.painter().galley(Pos2::new(xt, r.top() + 12.0), g, TEXTO);
                            ui.painter().text(Pos2::new(xt, r.top() + 38.0), Align2::LEFT_TOP, format!("@{}", u.nombre), medio(13.0), TEXTO_2);
                            if u.id == sesion.usuario.id {
                                continue;
                            }
                            let (texto_añadir, estilo) = match self.solicitudes.get(&u.id) {
                                Some(Carga::Cargando) => ("Enviando…", EstiloBoton::Secundario),
                                Some(Carga::Listo(())) => ("Solicitud enviada", EstiloBoton::Secundario),
                                Some(Carga::Error(_)) => ("Reintentar", EstiloBoton::Secundario),
                                _ => ("Añadir", EstiloBoton::Principal),
                            };
                            // «Mensaje» a la derecha y «Añadir» a su izquierda.
                            let (r_msg, _) = boton(ui, Pos2::new(r.right() - 130.0, r.center().y - 26.0), "Mensaje", EstiloBoton::Secundario, &format!("msg{i}"));
                            let g = ui.painter().layout_no_wrap(texto_añadir.to_string(), semi(18.0), TEXTO);
                            let w_add = g.size().x + 56.0;
                            let (r_add, _) = boton(ui, Pos2::new(r.right() - 130.0 - 12.0 - w_add, r.center().y - 26.0), texto_añadir, estilo, &format!("add{i}"));
                            if let Some(Carga::Error(e)) = self.solicitudes.get(&u.id) {
                                let g = galley(ui, e, medio(12.0), ERROR, 360.0, 1);
                                ui.painter().galley(Pos2::new(xt, r.top() + 56.0), g, ERROR);
                            }
                            if r_add.clicked() {
                                accion = Some((i, true));
                            }
                            if r_msg.clicked() {
                                accion = Some((i, false));
                            }
                        }
                    });
                    match accion {
                        Some((i, true)) => self.solicitar_amistad(personas[i].id),
                        Some((i, false)) => {
                            let u = personas[i].clone();
                            self.abrir_chat_con(&u);
                        }
                        None => {}
                    }
                }
            }
        }
        self.foco_social = foco_s;
    }
}
