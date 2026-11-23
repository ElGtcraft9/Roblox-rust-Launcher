//! Mercado (catálogo) y ficha de artículo.

use super::*;

impl App {
    // ---------- Mercado ----------

    pub(super) fn cargar_mercado(&mut self) {
        self.mercado_q = self.mercado_q.trim().to_string();
        let clave = (self.mercado_cat, self.mercado_q.clone());
        // Lo ya visto se muestra al instante y no gasta peticiones (el catálogo
        // limita mucho las peticiones seguidas).
        if let Some(l) = self.mercado_cache.get(&clave) {
            self.mercado = Carga::Listo(l.clone());
            return;
        }
        self.mercado_n += 1;
        let n = self.mercado_n;
        self.mercado = Carga::Cargando;
        let tipo = CATEGORIAS_MERCADO[self.mercado_cat].1;
        let http = self.http.clone();
        let q = clave.1;
        self.tarea(async move { Msg::Mercado(n, api::mercado(&http, tipo, &q).await.map_err(texto_error)) });
    }

    pub(super) fn cargar_robux(&mut self) {
        let Some(s) = self.sesion.clone() else { return };
        self.tarea(async move { Msg::Robux(s.robux().await.map_err(texto_error)) });
    }

    pub(super) fn pantalla_mercado(&mut self, ui: &mut Ui, pantalla: Rect, acciones: &[Accion]) {
        let x0 = pantalla.left() + MARGEN;
        let consola = self.cfg.modo == Modo::Consola;
        let lista: Vec<Articulo> = match &self.mercado {
            Carga::Listo(l) => l.clone(),
            _ => Vec::new(),
        };
        let lado = 150.0;
        let paso = Vec2::new(lado + 20.0, lado + 74.0);
        let columnas = ((pantalla.width() - MARGEN * 2.0 + 20.0) / paso.x).floor().max(1.0) as usize;

        // Navegación con mando o flechas.
        let mut foco_m = self.foco_mercado;
        let mut abrir: Option<usize> = None;
        let mut elegir_cat: Option<usize> = None;
        let ultimo = lista.len().saturating_sub(1);
        for a in acciones {
            foco_m = match (a, foco_m) {
                (Accion::Abajo, FocoMercado::Campo) => FocoMercado::Categoria(self.mercado_cat),
                (Accion::Abajo, FocoMercado::Categoria(_)) if !lista.is_empty() => FocoMercado::Articulo(0),
                (Accion::Abajo, FocoMercado::Articulo(i)) => FocoMercado::Articulo((i + columnas).min(ultimo)),
                (Accion::Arriba, FocoMercado::Articulo(i)) if i < columnas => {
                    if consola { FocoMercado::Categoria(self.mercado_cat) } else { foco_m }
                }
                (Accion::Arriba, FocoMercado::Articulo(i)) => FocoMercado::Articulo(i - columnas),
                (Accion::Arriba, FocoMercado::Categoria(_)) => FocoMercado::Campo,
                (Accion::Izquierda, FocoMercado::Categoria(i)) => FocoMercado::Categoria(i.saturating_sub(1)),
                (Accion::Derecha, FocoMercado::Categoria(i)) => FocoMercado::Categoria((i + 1).min(CATEGORIAS_MERCADO.len() - 1)),
                (Accion::Izquierda, FocoMercado::Articulo(i)) => FocoMercado::Articulo(i.saturating_sub(1)),
                (Accion::Derecha, FocoMercado::Articulo(i)) => FocoMercado::Articulo((i + 1).min(ultimo)),
                (Accion::Aceptar, FocoMercado::Campo) => {
                    self.abrir_teclado(Destino::Mercado);
                    FocoMercado::Campo
                }
                (Accion::Aceptar, FocoMercado::Categoria(i)) => {
                    elegir_cat = Some(i);
                    foco_m
                }
                (Accion::Aceptar, FocoMercado::Articulo(i)) => {
                    abrir = Some(i);
                    foco_m
                }
                (_, f) => f,
            };
        }

        // Buscador y saldo.
        let campo_r = Rect::from_min_size(Pos2::new(x0, pantalla.top() + 96.0), Vec2::new((pantalla.width() - MARGEN * 2.0).min(560.0), 52.0));
        let c = self.campo(ui, campo_r, Destino::Mercado, "Busca ropa, accesorios, emotes…", consola && foco_m == FocoMercado::Campo);
        if c.enviado {
            self.cargar_mercado();
        }
        let p = ui.painter().clone();
        if let Some(r) = self.robux {
            p.text(
                Pos2::new(pantalla.right() - MARGEN, campo_r.center().y),
                Align2::RIGHT_CENTER,
                format!("{} Robux", miles(r)),
                semi(escala::PESTANA),
                TEXTO,
            );
        }

        // Categorías.
        let mut x = x0;
        let y_chips = campo_r.bottom() + 20.0;
        for (i, (nombre, _)) in CATEGORIAS_MERCADO.iter().enumerate() {
            let activa = i == self.mercado_cat;
            let g = p.layout_no_wrap(nombre.to_string(), semi(15.0), if activa { TINTA } else { TEXTO });
            let r = Rect::from_min_size(Pos2::new(x, y_chips), Vec2::new(g.size().x + 32.0, 40.0));
            let resp = ui.interact(r, Id::new(("categoria", i)), Sense::click());
            let fondo = if activa { FOCO } else if resp.hovered() { cristal_fuerte() } else { cristal() };
            p.rect_filled(r, 20, fondo);
            p.galley(r.center() - g.size() / 2.0, g, TEXTO);
            if consola && foco_m == FocoMercado::Categoria(i) {
                p.rect_stroke(r.expand(4.0), 24, Stroke::new(3.0, FOCO), StrokeKind::Inside);
            }
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                elegir_cat = Some(i);
            }
            x = r.right() + 10.0;
        }
        if let Some(i) = elegir_cat
            && i != self.mercado_cat
        {
            self.mercado_cat = i;
            self.cargar_mercado();
        }

        let y = y_chips + 40.0 + 28.0;
        let aviso = match &self.mercado {
            Carga::Nada | Carga::Cargando => Some(("Cargando el mercado…".to_string(), TEXTO_2)),
            Carga::Error(e) => Some((trf("No se pudo cargar el mercado. {}", &[e]), ERROR)),
            Carga::Listo(l) if l.is_empty() => Some(("No hay artículos que coincidan. Prueba con otras palabras.".to_string(), TEXTO_2)),
            Carga::Listo(_) => None,
        };
        if let Some((t, c)) = aviso {
            let g = galley(ui, &t, cuerpo(escala::CUERPO), c, pantalla.width() - MARGEN * 2.0, 3);
            p.galley(Pos2::new(x0, y), g, c);
            self.foco_mercado = foco_m;
            return;
        }

        let zona = Rect::from_min_max(Pos2::new(pantalla.left(), y - 8.0), Pos2::new(pantalla.right(), pantalla.bottom() - 60.0));
        let fila_foco = match foco_m {
            FocoMercado::Articulo(i) => (i / columnas) as f32,
            _ => 0.0,
        };
        let filas_visibles = (zona.height() / paso.y).floor().max(1.0);
        let objetivo = (fila_foco - filas_visibles + 1.0).max(0.0) * paso.y;
        let off = ui.ctx().animate_value_with_time(Id::new("mercado_v"), objetivo, 0.18);
        let pc = p.with_clip_rect(zona);
        let movido = raton_movido(ui, self.cfg.modo == Modo::Consola);
        for (i, a) in lista.iter().enumerate() {
            let (f, col) = (i / columnas, i % columnas);
            let r = Rect::from_min_size(Pos2::new(x0 + col as f32 * paso.x, y + f as f32 * paso.y - off), Vec2::splat(lado));
            if r.bottom() + 60.0 < zona.top() || r.top() > zona.bottom() {
                continue;
            }
            let tipo = if a.paquete { TipoImg::Paquete } else { TipoImg::Prenda };
            let tex = self.img(tipo, a.id);
            pc.rect_filled(r, RADIO, cristal());
            if let Some(t) = &tex {
                cubrir(&pc, t, r.shrink(10.0), 1.0);
            }
            let enfocado = foco_m == FocoMercado::Articulo(i);
            if enfocado {
                foco(&pc, r);
            }
            let g = galley(ui, &a.nombre, medio(escala::ETIQUETA), if enfocado { TEXTO } else { TEXTO_2 }, lado, 2);
            let h = g.size().y;
            pc.galley(Pos2::new(r.left(), r.bottom() + 8.0), g, TEXTO);
            let precio = match a.precio {
                Some(0) => "Gratis".to_string(),
                Some(n) => format!("{} Robux", miles(n)),
                None => "No está a la venta".to_string(),
            };
            pc.text(Pos2::new(r.left(), r.bottom() + 12.0 + h), Align2::LEFT_TOP, precio, semi(14.0), TEXTO);
            let resp = ui.interact(r.intersect(zona), Id::new(("articulo", i)), Sense::click());
            if !enfocado && resp.hovered() {
                resalte(&pc, r);
            }
            if resp.hovered() && movido {
                foco_m = FocoMercado::Articulo(i);
            }
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                abrir = Some(i);
            }
        }
        self.foco_mercado = foco_m;
        if let Some(i) = abrir
            && let Some(a) = lista.get(i)
        {
            self.articulo = Some(a.clone());
        }
    }

    /// Ficha de un artículo del mercado. La compra se hace en roblox.com.
    pub(super) fn capa_articulo(&mut self, ctx: &egui::Context, pantalla: Rect, acciones: &[Accion]) {
        let Some(a) = self.articulo.clone() else { return };
        let tipo = if a.paquete { TipoImg::PaqueteGrande } else { TipoImg::PrendaGrande };
        let tex = self.img(tipo, a.id);
        let fondo = mezclar(self.tinte_actual(), Color32::WHITE, 0.07);
        let mut cerrar = acciones.contains(&Accion::Volver);
        let mut ver = acciones.contains(&Accion::Aceptar);
        egui::Area::new(Id::new("articulo")).order(Order::Foreground).fixed_pos(pantalla.min).show(ctx, |ui| {
            ui.allocate_rect(pantalla, Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(pantalla, 0, Color32::from_black_alpha(170));
            let caja = Rect::from_center_size(pantalla.center(), Vec2::new((pantalla.width() - 64.0).min(880.0), 460.0));
            p.rect_filled(caja, RADIO + 4, fondo);
            let img = Rect::from_min_size(caja.min + Vec2::splat(40.0), Vec2::splat(380.0));
            p.rect_filled(img, RADIO, cristal());
            if let Some(t) = &tex {
                cubrir(&p, t, img.shrink(16.0), 1.0);
            }
            let x = img.right() + 40.0;
            let ancho = caja.right() - 40.0 - x;
            let mut y = img.top();
            let g = galley(ui, &a.nombre, estrecha_fuerte(escala::TITULO), TEXTO, ancho, 2);
            let h = g.size().y;
            p.galley(Pos2::new(x, y), g, TEXTO);
            y += h + 4.0;
            p.text(Pos2::new(x, y), Align2::LEFT_TOP, trf("de {}", &[&a.creador]), medio(escala::CUERPO), TEXTO_2);
            y += 34.0;
            let precio = match a.precio {
                Some(0) => "Gratis".to_string(),
                Some(n) => format!("{} Robux", miles(n)),
                None => "No está a la venta".to_string(),
            };
            p.text(Pos2::new(x, y), Align2::LEFT_TOP, precio, estrecha(escala::SECCION), TEXTO);
            y += 44.0;
            let fuente = cuerpo(escala::ETIQUETA);
            let desc: String = a.descripcion.lines().map(|l| limpiar(ui, l, &fuente)).collect::<Vec<_>>().join("\n");
            let g = galley(ui, desc.trim(), fuente, TEXTO_2, ancho, 6);
            p.galley(Pos2::new(x, y), g, TEXTO_2);
            let yb = img.bottom() - 52.0;
            let (r_ver, w) = boton(ui, Pos2::new(x, yb), "Ver en Roblox", EstiloBoton::Principal, "art_ver");
            let (r_volver, _) = boton(ui, Pos2::new(x + w + 12.0, yb), "Volver", EstiloBoton::Secundario, "art_volver");
            ver |= r_ver.clicked();
            cerrar |= r_volver.clicked();
        });
        if ver {
            ctx.open_url(egui::OpenUrl::new_tab(a.url()));
        }
        if cerrar {
            self.articulo = None;
        }
    }
}
