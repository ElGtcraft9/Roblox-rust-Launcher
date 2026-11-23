"""Monta el tráiler de RustLauncher: 1920x1080, 30 fps, ~24 s.

Uso: python3 -I montar.py <carpeta de capturas> <carpeta de fuentes> <logo> <salida.mp4>
"""
import math, random, struct, subprocess, sys, wave
from PIL import Image, ImageDraw, ImageFilter, ImageFont

CAP, FUENTES, LOGO, SALIDA = sys.argv[1:5]
W, H, FPS = 1920, 1080, 30
MARGEN = 96
NOCHE = (0x13, 0x0F, 0x0D)
TEXTO = (0xF4, 0xF6, 0xF8)
TEXTO_2 = (0xB8, 0xBA, 0xBD)

titular = ImageFont.truetype(f"{FUENTES}/barlow-condensed-700.ttf", 76)
titular_grande = ImageFont.truetype(f"{FUENTES}/barlow-condensed-700.ttf", 96)
cuerpo = ImageFont.truetype(f"{FUENTES}/barlow-400.ttf", 30)
pequena = ImageFont.truetype(f"{FUENTES}/barlow-500.ttf", 26)
cifra = ImageFont.truetype(f"{FUENTES}/barlow-condensed-700.ttf", 250)
etiqueta = ImageFont.truetype(f"{FUENTES}/barlow-400.ttf", 38)


def cargar(nombre):
    return Image.open(f"{CAP}/{nombre}.png").convert("RGB")


# El editor: sin el nombre de la cuenta, los grupos ni los lugares recientes.
editor = cargar("editor").crop((0, 0, 1102, 620))
zona = (0, 330, 230, 460)
editor.paste(editor.crop(zona).filter(ImageFilter.GaussianBlur(9)), zona[:2])
editor = editor.resize((W, H), Image.LANCZOS)

planos = {n: cargar(n) for n in ["inicio", "buscar", "mercado", "consola"]}
forja = [Image.open(f"{CAP}/forja/forja-{i:04d}.png").convert("RGB") for i in range(60)]
logo = Image.open(LOGO).convert("RGBA")
logo = logo.resize((int(logo.width * 0.42), int(logo.height * 0.42)), Image.LANCZOS)

# Velo inferior para leer el texto sobre las capturas.
velo = Image.new("L", (W, H), 0)
dv = ImageDraw.Draw(velo)
for y in range(H // 2, H):
    k = (y - H / 2) / (H / 2)
    dv.line([(0, y), (W, y)], fill=int(215 * k ** 1.6))
negro = Image.new("RGB", (W, H), (0, 0, 0))


def acercar(im, k):
    """Empuje lento de cámara: escala k centrada."""
    if k <= 1.0001:
        return im
    w, h = int(W / k), int(H / k)
    x, y = (W - w) // 2, (H - h) // 2
    return im.crop((x, y, x + w, y + h)).resize((W, H), Image.BILINEAR)


def suave(x):
    x = max(0.0, min(1.0, x))
    return x * x * (3 - 2 * x)


def fundir(im, color, k):
    if k <= 0:
        return im
    return Image.blend(im, Image.new("RGB", (W, H), color), min(1.0, k))


def rotulo(im, linea, sub, alfa, y_base=H - MARGEN):
    """Texto abajo a la izquierda sobre el velo."""
    if alfa <= 0:
        return im
    im = Image.composite(negro, im, velo.point(lambda v: int(v * min(1, alfa * 1.4))))
    capa = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(capa)
    a = int(255 * alfa)
    y = y_base
    if sub:
        d.text((MARGEN, y), sub, font=cuerpo, fill=TEXTO_2 + (a,), anchor="ls")
        y -= 52
    d.text((MARGEN, y), linea, font=titular, fill=TEXTO + (a,), anchor="ls")
    im = im.convert("RGBA")
    im.alpha_composite(capa)
    return im.convert("RGB")


def tarjeta(lineas, alfa):
    """Fondo de forja con texto a la izquierda, centrado en vertical."""
    im = Image.new("RGB", (W, H), NOCHE)
    d = ImageDraw.Draw(im)
    a = alfa
    col = lambda c: tuple(int(NOCHE[i] + (c[i] - NOCHE[i]) * a) for i in range(3))
    y = H / 2
    for texto, fuente, color, dy in lineas:
        d.text((MARGEN * 2, y + dy), texto, font=fuente, fill=col(color), anchor="ls")
    return im


# ── Guion: (inicio, fin, función(t_local, duración) -> imagen) ─────────────
def intro(t, d):
    a = suave((t - 0.4) / 0.5) * (1 - suave((t - (d - 0.5)) / 0.5))
    return tarjeta(
        [("Un launcher para Roblox, hecho en Rust.", titular_grande, TEXTO, 0), ("Código nativo. Sin Electron, sin navegador.", cuerpo, TEXTO_2, 56)],
        a,
    )


# Pantalla: la captura a un 80 %, con radio 6, sobre la noche de forja; el
# texto va debajo, en su propio espacio, sin tocar nunca la interfaz.
PW, PH = 1536, 864
PX, PY = (W - PW) // 2, 52
mascara = Image.new("L", (PW, PH), 0)
ImageDraw.Draw(mascara).rounded_rectangle((0, 0, PW - 1, PH - 1), radius=8, fill=255)


def pantalla(im, empuje, linea, sub, alfa_texto):
    lienzo = Image.new("RGB", (W, H), NOCHE)
    vista = acercar(im, empuje).resize((PW, PH), Image.LANCZOS)
    lienzo.paste(vista, (PX, PY), mascara)
    d = ImageDraw.Draw(lienzo)
    col = lambda c: tuple(int(NOCHE[i] + (c[i] - NOCHE[i]) * alfa_texto) for i in range(3))
    base = PY + PH + 96
    d.text((PX, base), linea, font=titular, fill=col(TEXTO), anchor="ls")
    if sub:
        d.text((PX + PW, base), sub, font=cuerpo, fill=col(TEXTO_2), anchor="rs")
    return lienzo


def plano(nombre, linea, sub, empuje=0.05, entrada=0.3):
    def f(t, d):
        return pantalla(planos[nombre], 1 + empuje * (t / d), linea, sub, suave((t - entrada) / 0.3))
    return f


def buscar_mercado(t, d):
    mitad = d / 2
    nombre = "buscar" if t < mitad else "mercado"
    return pantalla(planos[nombre], 1 + 0.03 * ((t % mitad) / mitad), "Imágenes en WebP y caché en disco.", "Busca juegos y ropa sin esperar.", suave((t - 0.2) / 0.3))


CIFRAS = [
    ("0,25 s", "hasta que ves la portada"),
    ("0,22 ms", "lo que cuesta dibujar cada fotograma"),
    ("100", "iconos listos en un cuarto de segundo"),
]
PASO_CIFRA = 1.5


def cifras(t, d):
    i = min(len(CIFRAS) - 1, int(t / PASO_CIFRA))
    numero, texto = CIFRAS[i]
    im = Image.new("RGB", (W, H), NOCHE)
    dr = ImageDraw.Draw(im)
    tl = t - i * PASO_CIFRA
    a = suave(tl / 0.18)
    if i == len(CIFRAS) - 1:
        a *= 1 - suave((tl - (d - i * PASO_CIFRA - 0.3)) / 0.3)
    col = lambda c, k=a: tuple(int(NOCHE[j] + (c[j] - NOCHE[j]) * k) for j in range(3))
    x = MARGEN * 2
    dr.text((x, H / 2 + 70), numero, font=cifra, fill=col(TEXTO), anchor="ls")
    dr.text((x, H / 2 + 140), texto, font=etiqueta, fill=col(TEXTO_2), anchor="ls")
    # La nota de la medida no parpadea entre cifras.
    k = suave(t / 0.3) * (1 - suave((t - (d - 0.3)) / 0.3))
    dr.text((x, H - MARGEN), "Medido en un Ryzen 7 5700U con Linux.", font=pequena, fill=col(TEXTO_2, k * 0.8), anchor="ls")
    dr.text((W - x, H - MARGEN), "Rendimiento, la prioridad.", font=pequena, fill=col(TEXTO_2, k * 0.8), anchor="rs")
    return im


def transicion_crear(t, d):
    a = suave((t - 0.3) / 0.4) * (1 - suave((t - (d - 0.35)) / 0.35))
    return tarjeta([("Y ahora, también puedes crear.", titular_grande, TEXTO, 0)], a)


def escena_forja(t, d):
    i = min(59, int(t * FPS))
    return forja[i]


def escena_editor(t, d):
    im = pantalla(editor, 1 + 0.04 * (t / d), "Un editor de Roblox, también en Rust.", "Dentro del launcher, sin otra ventana.", suave((t - 0.45) / 0.3))
    return fundir(im, (0, 0, 0), 1 - suave(t / 0.25))


def final(t, d):
    a = suave((t - 0.2) / 0.6)
    im = Image.new("RGB", (W, H), NOCHE)
    capa = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    l = logo.copy()
    l.putalpha(l.getchannel("A").point(lambda v: int(v * a)))
    y_logo = H // 2 - l.height // 2 - 50
    capa.alpha_composite(l, (MARGEN * 2, y_logo))
    d_ = ImageDraw.Draw(capa)
    b = suave((t - 0.9) / 0.5)
    d_.text((MARGEN * 2, y_logo + l.height + 70), "Hecho en Rust. Gratis y de código abierto.", font=cuerpo, fill=TEXTO + (int(255 * b),), anchor="ls")
    d_.text(
        (MARGEN * 2, y_logo + l.height + 118),
        "github.com/ElGtcraft9/Roblox-rust-Launcher",
        font=pequena,
        fill=TEXTO_2 + (int(255 * b),),
        anchor="ls",
    )
    im = im.convert("RGBA")
    im.alpha_composite(capa)
    im = im.convert("RGB")
    return fundir(im, (0, 0, 0), suave((t - (d - 0.7)) / 0.7))


GUION = []
t0 = 0.0
for dur, f in [
    (3.2, intro),
    (4.5, cifras),
    (3.6, plano("inicio", "Abre en un cuarto de segundo.", "La red empieza a trabajar antes que la ventana.")),
    (2.8, buscar_mercado),
    (2.6, plano("consola", "Con ratón o con mando.", "Sin perder un fotograma.", 0.03)),
    (2.0, transicion_crear),
    (2.0, escena_forja),
    (3.4, escena_editor),
    (4.2, final),
]:
    GUION.append((t0, t0 + dur, f))
    t0 += dur
TOTAL = t0
CORTES = [a for a, _, _ in GUION[1:]] + [GUION[1][0] + PASO_CIFRA * k for k in (1, 2)]
INICIO_FORJA = next(a for a, _, f in GUION if f is escena_forja)


# ── Sonido ────────────────────────────────────────────────────────────────
def sonido(ruta):
    sr = 44100
    n = int(TOTAL * sr)
    m = [0.0] * n
    rnd = random.Random(7)
    # Zumbido grave que crece hacia la forja y se apaga al final.
    for i in range(n):
        t = i / sr
        env = suave(t / 2.5) * (0.55 + 0.45 * suave((t - 10) / 6)) * (1 - suave((t - (TOTAL - 2.5)) / 2.5))
        m[i] += env * 0.07 * (math.sin(2 * math.pi * 55 * t) + 0.6 * math.sin(2 * math.pi * 82.4 * t + 0.3 * math.sin(t * 0.7)))

    def suma(t_ini, dur, gen):
        i0 = int(t_ini * sr)
        for j in range(int(dur * sr)):
            if 0 <= i0 + j < n:
                m[i0 + j] += gen(j / sr)

    # Cortes: un chasquido corto y suave.
    for c in CORTES:
        suma(c, 0.06, lambda t: 0.10 * math.exp(-t * 90) * (rnd.random() * 2 - 1))
    # Golpes de yunque: parciales inarmónicos con caída exponencial.
    impactos = [INICIO_FORJA + (k * 0.45 + 0.8 * 0.45) for k in range(4)]
    for k, ti in enumerate(impactos):
        f0 = 1250 + 40 * k
        fuerza = 0.22 + 0.06 * k
        parciales = [(1.0, 1.0, 9), (2.76, 0.55, 14), (5.40, 0.30, 20), (8.93, 0.16, 28)]
        def g(t, f0=f0, fuerza=fuerza):
            s = sum(a * math.exp(-t * dec) * math.sin(2 * math.pi * f0 * r * t) for r, a, dec in parciales)
            golpe = math.exp(-t * 160) * (rnd.random() * 2 - 1) * 0.8
            return fuerza * (0.5 * s + golpe)
        suma(ti, 1.2, g)
    # Final: un golpe grave cuando aparece el logo.
    suma(GUION[-1][0] + 0.2, 2.5, lambda t: 0.35 * math.exp(-t * 2.2) * math.sin(2 * math.pi * (48 + 20 * math.exp(-t * 8)) * t))
    pico = max(abs(v) for v in m) or 1
    with wave.open(ruta, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(b"".join(struct.pack("<h", int(v / pico * 0.85 * 32767)) for v in m))


audio = SALIDA + ".wav"
sonido(audio)

ff = subprocess.Popen(
    ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
     "-i", audio, "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p",
     "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart", "-shortest", SALIDA],
    stdin=subprocess.PIPE,
)
for k in range(int(TOTAL * FPS)):
    t = k / FPS
    for a, b, f in GUION:
        if a <= t < b:
            ff.stdin.write(f(t - a, b - a).tobytes())
            break
    else:
        ff.stdin.write(negro.tobytes())
ff.stdin.close()
ff.wait()
print("listo", round(TOTAL, 1), "s")
