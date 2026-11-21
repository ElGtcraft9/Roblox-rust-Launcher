"""Genera el logo de RustLauncher (icono + logotipo) con Pillow.

Uso: python3 -I logo.py <carpeta_fuentes> <carpeta_salida>
"""
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

FUENTES = Path(sys.argv[1])
SALIDA = Path(sys.argv[2])
SALIDA.mkdir(parents=True, exist_ok=True)

BLANCO = (244, 246, 248, 255)
ARRIBA = (44, 51, 66)
ABAJO = (16, 19, 25)
SS = 4  # supermuestreo para bordes suaves


def degradado(tam):
    img = Image.new("RGBA", (tam, tam))
    d = ImageDraw.Draw(img)
    for y in range(tam):
        t = y / (tam - 1)
        c = tuple(int(a + (b - a) * t) for a, b in zip(ARRIBA, ABAJO))
        d.line([(0, y), (tam, y)], fill=c + (255,))
    return img


def icono(tam_final):
    tam = tam_final * SS
    radio = int(tam * 0.22)
    # Fondo: losa oscura redondeada con degradado vertical.
    mascara = Image.new("L", (tam, tam), 0)
    ImageDraw.Draw(mascara).rounded_rectangle([0, 0, tam - 1, tam - 1], radius=radio, fill=255)
    img = Image.new("RGBA", (tam, tam), (0, 0, 0, 0))
    img.paste(degradado(tam), (0, 0), mascara)

    # Dibujo recortado a la losa: el estante sale por el borde derecho.
    capa = Image.new("RGBA", (tam, tam), (0, 0, 0, 0))
    d = ImageDraw.Draw(capa)
    # Ficha enfocada (como en Inicio): blanca, con su marco de foco separado.
    lado = tam * 0.46
    x0, y0 = tam * 0.17, (tam - lado) / 2
    r_ficha = int(tam * 0.07)
    hueco = tam * 0.045
    grosor = max(int(tam * 0.035), SS)
    d.rounded_rectangle(
        [x0 - hueco - grosor, y0 - hueco - grosor, x0 + lado + hueco + grosor, y0 + lado + hueco + grosor],
        radius=r_ficha + int(hueco + grosor), outline=BLANCO, width=grosor,
    )
    d.rounded_rectangle([x0, y0, x0 + lado, y0 + lado], radius=r_ficha, fill=BLANCO)
    # Triángulo de «Jugar» recortado en la ficha.
    c = (x0 + lado / 2, y0 + lado / 2)
    h = lado * 0.42
    w = h * 0.9
    dx = w * 0.1
    d.polygon([(c[0] - w / 2 + dx, c[1] - h / 2), (c[0] - w / 2 + dx, c[1] + h / 2), (c[0] + w / 2 + dx, c[1])], fill=(0, 0, 0, 0))
    # Ficha siguiente, más pequeña y apagada, saliendo de la losa.
    lado2 = lado * 0.86
    x1 = x0 + lado + hueco + grosor + tam * 0.075
    y1 = (tam - lado2) / 2
    d.rounded_rectangle([x1, y1, x1 + lado2, y1 + lado2], radius=r_ficha, fill=(244, 246, 248, 95))
    recorte = Image.new("RGBA", (tam, tam), (0, 0, 0, 0))
    recorte.paste(capa, (0, 0), mascara)
    img = Image.alpha_composite(img, recorte)
    return img.resize((tam_final, tam_final), Image.LANCZOS)


def logotipo(color_texto, nombre):
    alto = 256
    ic = icono(alto * 2).resize((alto, alto), Image.LANCZOS)
    fuente = ImageFont.truetype(str(FUENTES / "barlow-condensed-700.ttf"), int(alto * 0.62))
    caja = fuente.getbbox("RustLauncher")
    ancho_texto = caja[2] - caja[0]
    separacion = int(alto * 0.22)
    lienzo = Image.new("RGBA", (alto + separacion + ancho_texto + 8, alto), (0, 0, 0, 0))
    lienzo.paste(ic, (0, 0), ic)
    d = ImageDraw.Draw(lienzo)
    alto_texto = caja[3] - caja[1]
    d.text((alto + separacion - caja[0], (alto - alto_texto) / 2 - caja[1]), "RustLauncher", font=fuente, fill=color_texto)
    lienzo.save(SALIDA / nombre)


for t in (1024, 512, 256, 128, 64, 32):
    icono(t).save(SALIDA / f"rustlauncher-icono-{t}.png")
icono(256).save(SALIDA / "rustlauncher.ico", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
logotipo(BLANCO, "rustlauncher-logo-claro.png")  # para fondos oscuros
logotipo((18, 20, 24, 255), "rustlauncher-logo-oscuro.png")  # para fondos claros
print("ok")
