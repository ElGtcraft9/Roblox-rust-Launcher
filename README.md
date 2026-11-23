<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/rustlauncher-logo-claro.png">
  <img alt="RustLauncher" src="docs/rustlauncher-logo-oscuro.png" height="72">
</picture>

Launcher alternativo para Roblox, escrito en Rust, con una interfaz de consola
(inspirada en Xbox, PlayStation y Steam Big Picture) y pensado para abrir rápido.

Funciona en **Windows** (con el cliente oficial de Roblox) y en **Linux**
(a través de [Sober](https://sober.vinegarhq.org/)).

> **Aviso:** RustLauncher no está afiliado ni respaldado por Roblox Corporation.
> No modifica el cliente de Roblox: lo descarga de los servidores oficiales y lo
> abre tal cual. Usa APIs web de Roblox que no están documentadas oficialmente y
> pueden cambiar sin aviso.

## Descargar

Descarga el ejecutable de tu sistema desde la
[última versión](https://github.com/ElGtcraft9/Roblox-rust-Launcher/releases/latest):

- **Windows:** `RustLauncher-windows.exe`. Como aún no está firmado con un
  certificado de Windows, SmartScreen avisará de «editor desconocido»: pulsa
  *Más información → Ejecutar de todas formas*.
- **Linux:** `RustLauncher-linux`. Dale permiso de ejecución
  (`chmod +x RustLauncher-linux`). Necesita [Sober](https://sober.vinegarhq.org/).

## Actualizaciones automáticas

Al abrirse (y cada 6 horas) el launcher consulta la última release de este
repositorio. Si hay una versión nueva, la descarga, **comprueba su firma** y se
reemplaza; la nueva versión se usa al reiniciar (*Reiniciar ahora*). Se puede
desactivar en *Ajustes → Launcher*.

Cada ejecutable de una release va acompañado de su firma `minisign`
(`.minisig`). El launcher solo se actualiza si la firma es válida para la clave
pública del proyecto y si nombra esa misma versión y plataforma. La clave
privada no está en el repositorio. Para comprobar una descarga a mano:

```sh
minisign -Vm RustLauncher-linux -P RWT4XCk0ZhR7fDVZ0FmaixOKiHwyUAdqzRWcrm5+Pd9lGtyogm84s1UT
```

## Qué hace

- **Abre los juegos directamente.** En Windows se salta el lanzador oficial
  (que comprueba la versión por internet en cada partida) y mantiene Roblox
  actualizado en segundo plano, así *Jugar* nunca espera a una descarga.
- **Dos modos de interfaz:**
  - **PC:** ventana adaptable, rueda del ratón y flechas.
  - **Consola:** pantalla completa, texto grande, mando y teclado en pantalla.
    Al conectar un mando, el launcher pregunta si quieres cambiar a este modo.
- **Inicio:** el arte del juego enfocado ocupa la pantalla; estantes con
  *Seguir jugando*, *Favoritos* y las listas de Roblox.
- **Buscar** experiencias, con ficha de cada juego.
- **Mercado:** catálogo por categorías y búsqueda (la compra se hace en roblox.com).
- **Social:** buscar personas, enviar solicitudes de amistad y chatear.
- **Perfil:** tu avatar, tu descripción y tus amigos con su estado.
- **Avatar:** ponerte tus trajes y las prendas de tu inventario, y cambiar entre R15 y R6.
- **Crear:** un editor de lugares de Roblox
  ([rustudio](https://github.com/rippelz/rustudio)) dentro de la misma ventana,
  sin abrir otro programa. Desde su menú *RustLauncher* se vuelve a las
  pestañas o se juega a la experiencia abierta.
- **Español e inglés:** se elige según el idioma del sistema y se cambia en
  *Ajustes → Launcher → Idioma*. También traduce los menús del editor.
- **Escalado FSR estilo Lossless Scaling (Linux):** Sober se abre dentro de
  [gamescope](https://github.com/ValveSoftware/gamescope) (incluido en Bazzite
  y SteamOS), que lo dibuja al 77, 67 o 50 % de la resolución de la pantalla
  y lo amplía con FSR 1. Activado por defecto en *Calidad*; se cambia en
  *Ajustes → Cliente de Roblox*. Sin gamescope, Sober se abre como siempre.
- **SMAA y nitidez (Linux):** con Sober en Vulkan, la capa
  [vkBasalt](https://github.com/DadSchoorse/vkBasalt) (extensión de Flatpak)
  suaviza los bordes con SMAA antes de que gamescope amplíe la imagen y la
  afina con CAS. Activado por defecto.
- **Límite de FPS:** el del propio Roblox (sus «FPS máximos», `FramerateCap`),
  60 por defecto. No añade nada al proceso del juego.
- **MAKO (beta, Linux):** generación de fotogramas con
  [MAKO](https://github.com/eugeniosegala/MAKO) (GPL-3.0). Al activarlo, el
  launcher descarga una versión fija, comprueba su huella SHA-256 e instala
  su extensión de Flatpak para el usuario; se activa en cada partida con
  variables de entorno, sin cambios permanentes en Sober.
- **Escalado del editor:** la vista de la pestaña Crear permite elegir FSR 1,
  FSR 2 o FSR 3.1 (los dos últimos, temporales; adaptados a WGSL desde el SDK
  de AMD, licencia MIT). Opcional y desactivada por defecto: *generar
  fotogramas* (experimental; recalcula un fotograma de cada dos con el
  movimiento de la cámara).
- **Optimización gráfica de Sober:** el ajuste propio `graphics_optimization_mode`.
- **Fingir FPS (broma):** muestra «6767 FPS» encima del juego; es solo un texto.
- **Rendimiento del sistema (Linux):** `herramientas/rendimiento-linux.sh`
  (con `sudo`, reversible) fija la GPU en reloj alto, la CPU en «performance»
  y activa el planificador scx_lavd.
- **Ajustes del cliente:** solo los de la
  [lista oficial de Roblox](https://devforum.roblox.com/t/allowlist-for-local-client-configuration-via-fast-flags/3966569)
  (motor gráfico, suavizado, texturas, hierba, detalle lejano…).
- **Barra de carga real:** avanza según lo que el cliente escribe en su registro.
- **Vuelta al launcher:** al salir de una experiencia, cierra Roblox y vuelve el launcher.

## Velocidad

Medido en Linux (Ryzen 7 5700U, NVMe):

| | Tiempo |
|---|---|
| Ventana y GPU listas | ~110 ms |
| Primer fotograma con la portada | ~250 ms |
| 100 iconos de la portada con caché | listos a los ~245 ms |
| 100 iconos sin caché | ~1,0–1,3 s |
| Coste de dibujar un fotograma | ~0,22 ms |
| Editor de la pestaña Crear (plantilla Laser Tag, gráfica integrada, calidad automática) | ~60 fps en edición y en Play |

Desde la versión con la pestaña Crear, el launcher dibuja con wgpu (antes,
OpenGL): el editor lo necesita, y el primer fotograma tarda ~0,2 s más.

Cómo se consigue:

- miniaturas en WebP (de 5 a 9 veces menos datos que PNG),
- respuestas comprimidas,
- la red empieza a trabajar antes de que exista la ventana,
- caché en disco con decodificación en paralelo,
- precarga de los archivos del cliente con prioridad de disco baja.

Para medirlo en tu equipo: `RUST_LAUNCHER_PERFIL=5 ./rust-launcher`.

Para medir el editor (las mismas medidas que `--bench` y `--profile-capture`
de rustudio):

```sh
# Dentro del launcher
RUSTUDIO_EMBED_OPEN=lugar.rbxl RUSTUDIO_EMBED_BENCH=dentro.json ./rust-launcher
# rustudio solo, con el mismo ejecutable
RUST_LAUNCHER_RUSTUDIO=1 ./rust-launcher lugar.rbxl --bench solo.json --bench-vsync
# Intervalo entre fotogramas y quién pide redibujar, por la salida de errores
RUST_LAUNCHER_PERFIL_CREAR=1 ./rust-launcher
```

## Inicio de sesión y privacidad

El inicio de sesión usa el mecanismo oficial de **Quick Login** de Roblox:
el launcher muestra un código (y un QR) que apruebas desde otro dispositivo con tu
sesión. **El launcher nunca ve tu contraseña.**

La sesión resultante se guarda en el almacén de credenciales del sistema
(Credential Manager en Windows, Secret Service en Linux), nunca en un archivo
propio. Esa sesión da acceso completo a la cuenta: ciérrala desde *Ajustes →
Cuenta* si dejas de usar el launcher.

## Estado

Versión **0.3.0**, en desarrollo.

| Parte | Estado |
|---|---|
| Interfaz, búsqueda, mercado, caché de imágenes | Probado en Linux |
| Abrir juegos con Sober (Linux) | Probado |
| Inicio de sesión: creación del código y QR | Probado |
| **Windows** (abrir juegos, enlaces de roblox.com, actualizaciones) | **Sin probar** |
| Paso final del inicio de sesión, editar el avatar, chat, solicitudes de amistad | **Sin probar con una cuenta real** |
| Mando de juego | **Sin probar con un mando real** |

## Compilar

Necesitas Rust 1.88 o posterior (edición 2024). Como el editor de la pestaña
Crear va dentro, la primera compilación tarda unos 10 minutos. En Linux hacen
falta las cabeceras de ALSA (`alsa-lib-devel` o `libasound2-dev`).

```sh
# Linux
cargo build --release

# Windows, compilando desde Linux (necesita mingw-w64)
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

Pruebas: `cargo test`. Las que usan la red de Roblox están marcadas como
ignoradas y se ejecutan con `cargo test -- --ignored`.

En Linux hace falta tener Sober instalado:
`flatpak install flathub org.vinegarhq.Sober`.

## Publicar una versión

1. Sube la versión en `Cargo.toml` y haz commit.
2. Ejecuta `herramientas/publicar.sh [notas.md]`.

El script compila Windows y Linux, firma ambos ejecutables con la clave privada
(`~/.config/rustlauncher-firma/minisign.key` o `$RUSTLAUNCHER_CLAVE`), genera
`SHA256SUMS` y crea la release con `gh`.

## Estructura

| Archivo | Contenido |
|---|---|
| `src/app/mod.rs` | Estado de la interfaz, arranque y bucle de dibujo |
| `src/app/inicio.rs`, `buscar.rs`, `mercado.rs`, `social.rs`, `perfil.rs`, `avatar.rs`, `ajustes.rs` | Una pantalla cada uno |
| `src/app/capas.rs` | Lanzamiento, inicio de sesión, diálogos y animación del modo consola |
| `src/app/modos.rs` | Modos PC y consola, transiciones y precarga de pestañas |
| `src/app/tareas.rs` | Trabajo en segundo plano, sesión y mensajes |
| `src/app/barras.rs`, `campos.rs`, `dibujo.rs` | Barras, campos de texto y utilidades de dibujo |
| `src/tema.rs` | Colores, tipografía y piezas visuales (ver `DESIGN.md`) |
| `src/api.rs` | APIs web públicas de Roblox (juegos, búsqueda, mercado, miniaturas) |
| `src/sesion.rs` | Inicio de sesión con código, avatar, amigos y chat |
| `src/roblox.rs` | Descarga, verificación e instalación del cliente de Windows |
| `src/windows.rs`, `src/linux.rs` | Lanzamiento del cliente en cada plataforma |
| `src/vigia.rs` | Lectura del registro del cliente (barra de carga y salida del juego) |
| `src/imagenes.rs` | Carga de miniaturas con caché en disco |
| `src/acelerar.rs` | Precarga de archivos y DNS |
| `src/actualizar.rs` | Actualizaciones automáticas firmadas |
| `src/teclado.rs` | Teclado en pantalla del modo consola |
| `src/app/crear.rs` | Pestaña Crear: animación de entrada y editor integrado |
| `src/idioma.rs`, `src/idioma/` | Idiomas: tabla del launcher en inglés y de rustudio en español |
| `rustudio/` | Código de rustudio, con dos cambios: `src/launcher.rs` (modo integrado y menú *RustLauncher*) y el traductor de textos en `vendor/egui` |
| `src/mando.rs` | Mando de juego |
| `src/flags.rs` | Ajustes del cliente (fast flags permitidas) |
| `herramientas/logo.py` | Genera el logo y los iconos |
| `herramientas/publicar.sh` | Compila, firma y publica una versión |
| `herramientas/minisign.pub` | Clave pública de firma |

## Créditos

Creado por **Gtcraft9355** (en YouTube) con la ayuda de Claude.

- Tipografía [Barlow](https://github.com/jpt/barlow), de Jeremy Tribby, bajo
  licencia SIL Open Font License (ver `assets/OFL-Barlow.txt`).
- Interfaz hecha con [egui](https://github.com/emilk/egui).
- Editor de la pestaña Crear: [rustudio](https://github.com/rippelz/rustudio),
  bajo licencia MIT o Apache 2.0 (ver `rustudio/LICENSE-MIT` y
  `rustudio/LICENSE-APACHE`).

## Licencia

[MIT](LICENSE). Las fuentes Barlow mantienen su propia licencia (SIL Open Font
License) y rustudio la suya (MIT o Apache 2.0).
