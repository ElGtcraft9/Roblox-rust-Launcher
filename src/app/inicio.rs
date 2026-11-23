//! Pantalla de inicio (estantes) y «Más información».

use super::*;

impl App {
    // ---------- Inicio ----------

    pub(super) fn pantalla_inicio(&mut self, ui: &mut Ui, pantalla: Rect, acciones: &[Accion]) {
        let estantes = self.estantes();
        let x0 = pantalla.left() + MARGEN;
        if estantes.is_empty() {
            let p = ui.painter().clone();
            let (t, d) = match &self.inicio_estado {
                Carga::Error(e) => ("No se pudo cargar la portada".to_string(), trf("Comprueba tu conexión a internet. {}", &[e])),
                _ => ("Cargando juegos".to_string(), String::new()),
            };
            p.text(Pos2::new(x0, pantalla.center().y - 20.0), Align2::LEFT_BOTTOM, t, estrecha_fuerte(escala::TITULO), TEXTO);
            p.text(Pos2::new(x0, pantalla.center().y), Align2::LEFT_TOP, d, cuerpo(escala::CUERPO), TEXTO_2);
            if matches!(self.inicio_estado, Carga::Error(_)) {
                let pos = Pos2::new(x0, pantalla.center().y + 40.0);
                if boton(ui, pos, "Reintentar", EstiloBoton::Secundario, "reintentar").0.clicked() {
                    self.inicio_estado = Carga::Cargando;
                    self.cargar_inicio();
                }
            }
            return;
        }

        // --- Entrada: teclado, mando y rueda ---
        self.fila = self.fila.min(estantes.len() - 1);
        let col_de = |s: &Self, f: usize| (*s.cols.get(&estantes[f].0).unwrap_or(&0)).min(estantes[f].1.len() - 1);
        let mut col = col_de(self, self.fila);
        let mut abrir_info = false;
        let mut jugar = false;
        let mut favorito = false;
        for a in acciones {
            match a {
                Accion::Izquierda => col = col.saturating_sub(1),
                Accion::Derecha => col = (col + 1).min(estantes[self.fila].1.len() - 1),
                Accion::Arriba | Accion::Abajo => {
                    self.cols.insert(estantes[self.fila].0.clone(), col);
                    self.fila = if *a == Accion::Arriba {
                        self.fila.saturating_sub(1)
                    } else {
                        (self.fila + 1).min(estantes.len() - 1)
                    };
                    col = col_de(self, self.fila);
                }
                Accion::Aceptar => jugar = true,
                Accion::Info => abrir_info = true,
                Accion::Favorito => favorito = true,
                _ => {}
            }
        }

        let alto_fila = 280.0;
        let y_est = pantalla.bottom() - 64.0 - alto_fila;
        // Solo se ve el estante activo: el siguiente queda oculto, como en PS5.
        let clip = Rect::from_min_max(Pos2::new(pantalla.left(), y_est - 4.0), Pos2::new(pantalla.right(), y_est + alto_fila - 8.0));
        let pc = self.cfg.modo == Modo::Pc;
        // En modo PC la rueda cambia de estante desde cualquier punto de la pantalla.
        if !self.hay_capa() && !pc && ui.rect_contains_pointer(clip) {
            self.rueda += ui.input(|i| i.smooth_scroll_delta);
            if self.rueda.y.abs() > 60.0 {
                self.cols.insert(estantes[self.fila].0.clone(), col);
                self.fila = if self.rueda.y < 0.0 { (self.fila + 1).min(estantes.len() - 1) } else { self.fila.saturating_sub(1) };
                col = col_de(self, self.fila);
                self.rueda = Vec2::ZERO;
            }
            if self.rueda.x.abs() > 60.0 {
                let n = estantes[self.fila].1.len();
                col = if self.rueda.x < 0.0 { (col + 1).min(n - 1) } else { col.saturating_sub(1) };
                self.rueda = Vec2::ZERO;
            }
        }
        self.cols.insert(estantes[self.fila].0.clone(), col);

        let enfocado = estantes[self.fila].1[col].clone();
        self.fondo_a(enfocado.universe_id);
        // Si el foco se queda medio segundo en un juego, su ficha se carga ya:
        // «Más información» se abrirá al instante.
        match self.foco_desde {
            Some((id, t)) if id == enfocado.universe_id => {
                if t.elapsed() > Duration::from_millis(500) && !self.detalles.contains_key(&id) {
                    self.detalles.insert(id, Carga::Cargando);
                    let http = self.http.clone();
                    self.tarea(async move { Msg::Detalle(id, api::detalle(&http, id).await.map_err(texto_error)) });
                } else if !self.detalles.contains_key(&id) {
                    ui.ctx().request_repaint_after(Duration::from_millis(520));
                }
            }
            _ => self.foco_desde = Some((enfocado.universe_id, Instant::now())),
        }
        // El arte de los vecinos se pide ya para que el cambio sea instantáneo.
        // Todos los iconos de todos los estantes se piden ya (una llamada por
        // cada 100): al bajar de estante ya están cargados.
        for (_, juegos) in &estantes {
            for j in juegos {
                self.img(TipoImg::Icono, j.universe_id);
            }
        }
        for vecino in [col.wrapping_sub(3), col.wrapping_sub(2), col.wrapping_sub(1), col + 1, col + 2, col + 3] {
            if let Some(j) = estantes[self.fila].1.get(vecino) {
                self.img(TipoImg::Banner, j.universe_id);
            }
        }

        // --- Héroe: juego enfocado ---
        let ancho_texto = (pantalla.width() * 0.55).min(760.0);
        let y_botones = y_est - 52.0 - 36.0;
        let y_meta = y_botones - 20.0;
        let mut meta = Vec::new();
        if let Some(n) = enfocado.jugando {
            meta.push(trf("{} jugando ahora", &[&abreviar(n)]));
        }
        if let Some((a, b)) = enfocado.votos.filter(|(a, b)| a + b > 0) {
            meta.push(trf("{} % de valoraciones positivas", &[&format!("{:.0}", a as f64 * 100.0 / (a + b) as f64)]));
        }
        let p = ui.painter().clone();
        let mut xm = x0;
        for parte in meta {
            let g = p.layout_no_wrap(parte, medio(escala::CUERPO), TEXTO_2);
            let w = g.size().x;
            p.galley(Pos2::new(xm, y_meta - g.size().y), g, TEXTO_2);
            xm += w + 24.0;
        }
        let entrada = self.progreso_fundido();
        let color_titulo = TEXTO.gamma_multiply(0.35 + 0.65 * entrada);
        let g_titulo = galley(ui, &enfocado.nombre, estrecha_fuerte(escala::DISPLAY), color_titulo, ancho_texto, 2);
        let y_titulo = y_meta - 30.0 - g_titulo.size().y + 10.0 * (1.0 - entrada);
        p.galley(Pos2::new(x0, y_titulo), g_titulo, color_titulo);

        let (r_jugar, w) = boton(ui, Pos2::new(x0, y_botones), "Jugar", EstiloBoton::Principal, "jugar");
        let ventana_estrecha = pantalla.width() < 980.0;
        let texto_info = if ventana_estrecha { "Información" } else { "Más información" };
        let (r_info, w2) = boton(ui, Pos2::new(x0 + w + 12.0, y_botones), texto_info, EstiloBoton::Secundario, "info");
        let fav = self.cfg.es_favorito(enfocado.universe_id);
        let texto_fav = match (fav, ventana_estrecha) {
            (true, true) => "Quitar",
            (true, false) => "Quitar de favoritos",
            (false, true) => "Favorito",
            (false, false) => "Añadir a favoritos",
        };
        let (r_fav, _) = boton(ui, Pos2::new(x0 + w + w2 + 24.0, y_botones), texto_fav, EstiloBoton::Secundario, "fav");
        jugar |= r_jugar.clicked();
        abrir_info |= r_info.clicked();
        favorito |= r_fav.clicked();

        // --- Estantes ---
        let desplaz = ui.ctx().animate_value_with_time(Id::new("desplaz_v"), self.fila as f32 * alto_fila, 0.18);
        let base = 150.0;
        let hueco = 16.0;
        let pc_modo = pc;
        let pc = p.with_clip_rect(clip);
        let movido = raton_movido(ui, self.cfg.modo == Modo::Consola);
        let mut nuevo_foco: Option<(usize, usize)> = None;
        let mut clic_enfocada = false;
        let mut fila_activa_geo: Option<(f32, f32, f32)> = None;

        for (fi, (titulo, juegos)) in estantes.iter().enumerate() {
            let y = y_est + fi as f32 * alto_fila - desplaz;
            if y > clip.bottom() || y + alto_fila < clip.top() {
                continue;
            }
            let activa = fi == self.fila;
            pc.text(Pos2::new(x0, y), Align2::LEFT_TOP, titulo, estrecha(escala::SECCION), if activa { TEXTO } else { TEXTO_2 });

            let col_fila = if activa { col } else { col_de(self, fi) };
            // La ficha enfocada crece y empuja al resto.
            let anchos: Vec<f32> = juegos
                .iter()
                .enumerate()
                .map(|(ci, j)| {
                    let t = ui
                        .ctx()
                        .animate_bool_with_time(Id::new(("ficha", fi, j.universe_id)), activa && ci == col_fila, 0.16);
                    base * (1.0 + 0.12 * t)
                })
                .collect();
            // Solo se desplaza cuando la ficha enfocada se saldría de la vista
            // (así no «huye» del ratón).
            let inicio_foco = col_fila as f32 * (base + hueco);
            let fin_foco = inicio_foco + base * 1.12;
            let visible = pantalla.width() - MARGEN * 2.0;
            let previo = *self.desplaz.get(titulo).unwrap_or(&0.0);
            let objetivo = previo.clamp((fin_foco - visible).max(0.0), inicio_foco);
            self.desplaz.insert(titulo.clone(), objetivo);
            let off = ui.ctx().animate_value_with_time(Id::new(("desplaz_h", titulo)), objetivo, 0.18);
            if activa {
                let total: f32 = anchos.iter().sum::<f32>() + hueco * anchos.len().saturating_sub(1) as f32;
                fila_activa_geo = Some((y + 40.0, off, total));
            }

            let mut x = x0 - off;
            for (ci, j) in juegos.iter().enumerate() {
                let w = anchos[ci];
                if x > pantalla.right() {
                    break;
                }
                if x + w >= pantalla.left() {
                    let r = Rect::from_min_size(Pos2::new(x, y + 40.0), Vec2::splat(w));
                    let tex = self.img(TipoImg::Icono, j.universe_id);
                    let atenuado = if activa { Color32::WHITE } else { Color32::from_gray(140) };
                    ficha_img(&pc, tex.as_ref(), r, atenuado);
                    let foco_aqui = activa && ci == col_fila;
                    if foco_aqui {
                        foco(&pc, r);
                    }
                    if pc_modo && activa {
                        // Modo PC: nombre bajo cada ficha, para elegir con el ratón.
                        let color = if foco_aqui { TEXTO } else { TEXTO_2 };
                        let g = galley(ui, &j.nombre, medio(14.0), color, w, 2);
                        pc.galley(Pos2::new(r.left(), r.bottom() + 10.0), g, TEXTO);
                    } else if foco_aqui {
                        let ancho_nombre = (pantalla.right() - MARGEN - r.left()).max(w);
                        let g = galley(ui, &j.nombre, semi(18.0), TEXTO, ancho_nombre, 1);
                        pc.galley(Pos2::new(r.left(), r.bottom() + 12.0), g, TEXTO);
                    }
                    let resp = ui.interact(r.intersect(clip), Id::new(("ficha_i", fi, ci)), Sense::click());
                    if pc_modo && resp.hovered() && !foco_aqui {
                        resalte(&pc, r);
                    }
                    if resp.hovered() && movido && !foco_aqui {
                        nuevo_foco = Some((fi, ci));
                    }
                    if resp.clicked() {
                        if foco_aqui {
                            clic_enfocada = true;
                        } else {
                            nuevo_foco = Some((fi, ci));
                        }
                    }
                    resp.on_hover_cursor(CursorIcon::PointingHand);
                }
                x += w + hueco;
            }
        }

        // Modo PC: flechas a los lados del estante para pasar página con el ratón.
        if pc_modo
            && let Some((y_fichas, off, total)) = fila_activa_geo
            && ui.rect_contains_pointer(clip)
        {
            let visible = pantalla.width() - MARGEN * 2.0;
            let por_pagina = ((visible / (base + hueco)).floor() as usize).max(1);
            let n = estantes[self.fila].1.len();
            let yc = y_fichas + base / 2.0;
            for (izq, mostrar) in [(true, off > 1.0), (false, off + visible < total - 1.0)] {
                if !mostrar {
                    continue;
                }
                let c = Pos2::new(if izq { pantalla.left() + 32.0 } else { pantalla.right() - 32.0 }, yc);
                let r = Rect::from_center_size(c, Vec2::splat(44.0));
                let resp = ui.interact(r, Id::new(("flecha", izq)), Sense::click());
                p.circle_filled(c, 22.0, if resp.hovered() { FOCO } else { Color32::from_black_alpha(150) });
                let color = if resp.hovered() { TINTA } else { TEXTO };
                p.text(c - Vec2::new(0.0, 2.0), Align2::CENTER_CENTER, if izq { "‹" } else { "›" }, semi(30.0), color);
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    let nueva = if izq { col.saturating_sub(por_pagina) } else { (col + por_pagina).min(n - 1) };
                    nuevo_foco = Some((self.fila, nueva));
                }
            }
        }

        if let Some((f, c)) = nuevo_foco {
            self.fila = f;
            self.cols.insert(estantes[f].0.clone(), c);
        }
        if jugar {
            self.jugar(enfocado);
        } else if abrir_info || clic_enfocada {
            self.abrir_info(enfocado.universe_id);
        } else if favorito {
            self.cfg.alternar_favorito(enfocado);
        }
    }

    // ---------- Información del juego ----------

    pub(super) fn pantalla_info(&mut self, ui: &mut Ui, pantalla: Rect, id: u64, acciones: &[Accion]) {
        self.fondo_a(id);
        let x0 = pantalla.left() + MARGEN;
        let ancho = (pantalla.width() * 0.5).clamp(420.0, 720.0);
        let p = ui.painter().clone();
        // Velo extra: aquí se lee bastante texto.
        degradado_h(
            &p,
            Rect::from_min_size(pantalla.min, Vec2::new(ancho + MARGEN * 2.5, pantalla.height())),
            self.tinte_actual().gamma_multiply(0.85),
            self.tinte_actual().gamma_multiply(0.0),
        );
        let volver_pedido = acciones.contains(&Accion::Volver)
            || ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra1));

        let d = match self.detalles.get(&id) {
            Some(Carga::Listo(d)) => d.clone(),
            Some(Carga::Error(e)) => {
                p.text(Pos2::new(x0, 140.0), Align2::LEFT_TOP, "No se pudo cargar esta experiencia", estrecha_fuerte(escala::TITULO), TEXTO);
                p.text(Pos2::new(x0, 192.0), Align2::LEFT_TOP, e, cuerpo(escala::CUERPO), ERROR);
                if volver_pedido {
                    self.info = None;
                }
                return;
            }
            _ => {
                if volver_pedido {
                    self.info = None;
                }
                return;
            }
        };
        let juego = Juego {
            universe_id: d.universe_id,
            place_id: d.place_id,
            nombre: d.nombre.clone(),
            jugando: Some(d.jugando),
            votos: d.votos,
        };

        let mut y = pantalla.top() + 112.0;
        let g = galley(ui, &d.nombre, estrecha_fuerte(escala::DISPLAY), TEXTO, ancho, 2);
        let h = g.size().y;
        p.galley(Pos2::new(x0, y), g, TEXTO);
        y += h + 4.0;
        let g = galley(ui, &format!("de {}", d.creador), medio(escala::CUERPO), TEXTO_2, ancho, 1);
        p.galley(Pos2::new(x0, y), g, TEXTO_2);
        y += 40.0;

        let (r_jugar, w) = boton(ui, Pos2::new(x0, y), "Jugar", EstiloBoton::Principal, "info_jugar");
        let fav = self.cfg.es_favorito(id);
        let texto_fav = if fav { "Quitar de favoritos" } else { "Añadir a favoritos" };
        let (r_fav, w2) = boton(ui, Pos2::new(x0 + w + 12.0, y), texto_fav, EstiloBoton::Secundario, "info_fav");
        let (r_volver, _) = boton(ui, Pos2::new(x0 + w + w2 + 24.0, y), "Volver", EstiloBoton::Secundario, "info_volver");
        y += 52.0 + 32.0;

        // Datos como frases cortas.
        let valoracion = d
            .votos
            .filter(|(a, b)| a + b > 0)
            .map(|(a, b)| trf("{} % de valoraciones positivas", &[&format!("{:.0}", a as f64 * 100.0 / (a + b) as f64)]));
        let datos = [
            Some(trf("{} jugando ahora", &[&abreviar(d.jugando)])),
            Some(trf("{} visitas", &[&abreviar(d.visitas)])),
            valoracion,
            Some(trf("Servidores de {} jugadores", &[&d.max_jugadores])),
        ];
        let mut x = x0;
        for t in datos.into_iter().flatten() {
            let g = p.layout_no_wrap(t, medio(escala::ETIQUETA), TEXTO);
            let r = Rect::from_min_size(Pos2::new(x, y), g.size() + Vec2::new(24.0, 14.0));
            if r.right() > pantalla.right() - MARGEN {
                break;
            }
            p.rect_filled(r, RADIO, cristal());
            p.galley(r.min + Vec2::new(12.0, 7.0), g, TEXTO);
            x = r.right() + 8.0;
        }
        y += 52.0;

        // Descripción con desplazamiento propio.
        let zona = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x0 + ancho, pantalla.bottom() - 72.0));
        let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(zona));
        egui::ScrollArea::vertical().id_salt(("desc", id)).show(&mut hijo, |ui| {
            let f = cuerpo(escala::CUERPO);
            let texto: String = d.descripcion.lines().map(|l| limpiar(ui, l, &f)).collect::<Vec<_>>().join("\n");
            ui.label(egui::RichText::new(texto.trim()).font(f).color(TEXTO.gamma_multiply(0.88)));
        });

        let jugar = r_jugar.clicked() || acciones.contains(&Accion::Aceptar);
        let favorito = r_fav.clicked() || acciones.contains(&Accion::Favorito);
        if jugar {
            self.jugar(juego);
        } else if favorito {
            self.cfg.alternar_favorito(juego);
        } else if r_volver.clicked() || volver_pedido {
            self.info = None;
        }
    }
}
