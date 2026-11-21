# RustLauncher — lenguaje de diseño

## Lectura del encargo

Launcher de Roblox para escritorio · jugadores de Roblox (adolescentes y adultos jóvenes) ·
«como una consola»: Xbox 360, Xbox One, Steam Big Picture / Steam Machine, PlayStation ·
familia: interfaz de consola de salón (10-foot UI).

Diales: creatividad = equilibrada-atrevida · densidad = aireada · variación = uniforme · movimiento = equilibrado.

## Tema

**cinematográfico, inmediato, sala de estar**

- *Cinematográfico*: el arte del juego es el protagonista; la interfaz se aparta.
- *Inmediato*: jugar está a un clic, una tecla o un botón del mando.
- *Sala de estar*: se puede usar con mando desde el sofá; texto grande, foco visible.

Prueba de rechazo: cualquier elemento que compita con el arte del juego, o que añada un paso antes
de jugar, viola el tema.

## Rasgos extraídos de las referencias

| Referencia | Rasgos que se conservan |
|---|---|
| PS5 | Arte a pantalla completa del juego enfocado; nombre solo en la ficha enfocada; botón principal blanco |
| Xbox One / Series | Fichas planas casi cuadradas; foco con borde blanco; reloj y perfil arriba a la derecha |
| Xbox 360 | Pestañas de texto en la parte superior como navegación principal |
| Steam Big Picture | Estantes horizontales; pistas de botones del mando abajo; volver al launcher al salir del juego |

## Tokens

### Color

| Token | Valor | Uso |
|---|---|---|
| `fondo` | color dominante del juego enfocado, oscurecido (L≈10 %) | Base bajo el arte; la interfaz se tiñe con cada juego |
| `velo` | `fondo` con opacidad 0→96 % | Degradados que aseguran la lectura sobre el arte |
| `texto` | `#F4F6F8` | Texto principal |
| `texto-2` | `#F4F6F8` al 66 % | Texto secundario |
| `foco` | `#FFFFFF` | Borde de foco, botón principal |
| `cristal` | `#FFFFFF` al 14 % | Botones secundarios y filas resaltadas |
| `error` | `#FF8A80` | Mensajes de error |

No hay color de acento fijo: el color lo pone el juego.

### Tipografía

Barlow (OFL), una sola familia en dos anchos.

| Token | Fuente | Tamaño | Uso |
|---|---|---|---|
| `display` | Barlow Condensed 700 | 61 | Título del juego enfocado |
| `titulo` | Barlow Condensed 700 | 39 | Títulos de página |
| `seccion` | Barlow Condensed 600 | 25 | Nombre de estante |
| `pestana` | Barlow 600 | 20 | Pestañas superiores |
| `cuerpo` | Barlow 400 | 16 | Texto |
| `etiqueta` | Barlow 500 | 14 | Datos secundarios |

Escala 1,25 sobre 16 px.

### Espacio y forma

- Unidad 8 px: 8 · 16 · 24 · 32 · 48 · 64.
- Margen lateral de la pantalla: 64 px.
- Radios: 6 (fichas, botones) y círculo (avatar). Nada más.
- Profundidad: sin sombras. La jerarquía la dan la escala, el foco y los velos.

### Movimiento

- Foco: escala 1,00 → 1,08 en 160 ms, desaceleración.
- Cambio de arte de fondo: fundido cruzado de 250 ms.
- Desplazamiento de estantes: 180 ms. Sin rebotes.

## Navegación

| Acción | Teclado | Mando | Ratón |
|---|---|---|---|
| Moverse | Flechas | Cruceta / stick izquierdo | Pasar por encima |
| Jugar | Enter | A | Botón Jugar |
| Información | I | Y | Clic en la ficha enfocada |
| Favorito | F | X | Botón |
| Volver | Esc | B | Volver |
| Cambiar de pestaña | Q / E | LB / RB | Pestañas |

## Modos

| | PC | Consola |
|---|---|---|
| Ventana | Normal, redimensionable | Pantalla completa |
| Escala | Se adapta a la ventana (0,8×–1,25×) | Texto grande, 10-foot (1×–1,8×) |
| Estantes | Rueda del ratón en cualquier punto; nombre bajo cada ficha; flechas laterales | Cruceta/stick; nombre solo en la ficha enfocada |
| Pistas de botones | Ocultas | Visibles (glifos del mando si hay uno conectado) |

Al conectar un mando en modo PC se pregunta si cambiar a modo consola; al aceptar
se reproduce la animación «Modo consola activado» (el mando aparece y enciende A, B, X, Y).
