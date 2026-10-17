"""Images de l'installateur NSIS (bandeau latéral 164×314 et en-tête 150×57,
BMP 24 bits) et texte de licence affiché pendant l'installation.

    python scripts/make_installer_assets.py

Nécessite Pillow. Les fichiers produits sont versionnés dans
src-tauri/installer/ : il n'est utile de relancer ce script qu'en cas de
changement d'identité visuelle ou de licence.
"""
import os

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "src-tauri", "installer")
ICON = os.path.join(ROOT, "src-tauri", "icons", "128x128@2x.png")
FONTS = os.path.join(os.environ.get("WINDIR", "C:\\Windows"), "Fonts")

BG = (14, 15, 18)
ACCENT = (124, 120, 255)
ACCENT2 = (184, 108, 255)
TEXT = (236, 236, 241)
MUTED = (150, 152, 166)


def font(name, size):
    try:
        return ImageFont.truetype(os.path.join(FONTS, name), size)
    except OSError:
        return ImageFont.load_default()


def glow(img, cx, cy, r, color, strength):
    """Halo radial discret (dégradé vers le fond)."""
    w, h = img.size
    px = img.load()
    for y in range(h):
        for x in range(w):
            d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5 / r
            if d < 1:
                k = (1 - d) ** 2 * strength
                p = px[x, y]
                px[x, y] = tuple(int(p[i] + (color[i] - p[i]) * k) for i in range(3))


def icon(size):
    im = Image.open(ICON).convert("RGBA")
    return im.resize((size, size), Image.LANCZOS)


def sidebar():
    img = Image.new("RGB", (164, 314), BG)
    glow(img, 30, 40, 190, ACCENT, 0.32)
    glow(img, 150, 300, 170, ACCENT2, 0.18)
    ic = icon(64)
    img.paste(ic, (24, 40), ic)
    d = ImageDraw.Draw(img)
    d.text((24, 122), "Aura PDF", font=font("segoeuib.ttf", 22), fill=TEXT)
    d.text((24, 152), "Lire, annoter, signer.", font=font("segoeui.ttf", 12), fill=MUTED)
    d.text((24, 170), "Libre, gratuit,", font=font("segoeui.ttf", 12), fill=MUTED)
    d.text((24, 186), "hors ligne.", font=font("segoeui.ttf", 12), fill=MUTED)
    d.rounded_rectangle((24, 274, 84, 278), radius=2, fill=ACCENT)
    img.save(os.path.join(OUT, "sidebar.bmp"))


def header():
    img = Image.new("RGB", (150, 57), (255, 255, 255))
    ic = icon(36)
    img.paste(ic, (104, 10), ic)
    img.save(os.path.join(OUT, "header.bmp"))


def licence():
    with open(os.path.join(ROOT, "LICENSE"), encoding="utf-8") as f:
        mit = f.read().strip()
    text = (
        "Aura PDF est un logiciel libre et gratuit, distribué sous licence MIT.\r\n"
        "Vous pouvez l'utiliser, le copier, le modifier et le redistribuer librement,\r\n"
        "y compris à des fins commerciales.\r\n\r\n"
        "Aura PDF fonctionne entièrement hors ligne : vos documents ne quittent pas\r\n"
        "votre ordinateur (seul l'horodatage qualifié, s'il est activé, envoie\r\n"
        "l'empreinte d'un document à l'autorité choisie).\r\n\r\n"
        "Composants tiers : PDFium (BSD-3), Tesseract (Apache 2.0), données tessdata\r\n"
        "(Apache 2.0), polices et bibliothèques listées dans le fichier\r\n"
        "THIRD-PARTY-LICENSES.md installé avec l'application.\r\n\r\n"
        "Texte de la licence (en anglais, seul texte faisant foi) :\r\n\r\n"
        + mit.replace("\r\n", "\n").replace("\n", "\r\n")
        + "\r\n"
    )
    # UTF-8 sans BOM : Tauri lit le fichier comme du texte et ajoute lui-même
    # le BOM attendu par NSIS (un fichier UTF-16 s'afficherait illisible).
    with open(os.path.join(OUT, "LICENCE.txt"), "wb") as f:
        f.write(text.encode("utf-8"))


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    sidebar()
    header()
    licence()
    print("Images et licence de l'installateur écrites dans", OUT)
