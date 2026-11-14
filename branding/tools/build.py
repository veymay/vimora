#!/usr/bin/env python3
"""Builds every Vimora logo file from `branding/source/vimora-master.svg`.

Needs: python3 + Pillow, and `resvg` (cargo install resvg) on PATH or RESVG=/path/to/resvg.
Run from anywhere:  python3 branding/tools/build.py

Writes SVG + PNG of every variant under `branding/`, the app icons (PNG sizes + ICO), the
monochrome tray icons, and mirrors what the builds use into `assets/brand/`.
"""
import os, re, shutil, struct, subprocess, sys
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
B = ROOT / "branding"
RESVG = os.environ.get("RESVG", "resvg")
GREEN = "#7DBA28"
INK = "#1D1F1B"   # the wordmark on light backgrounds
BLACK, WHITE = "#000000", "#FFFFFF"

master = (B / "source" / "vimora-master.svg").read_text()
paths = re.findall(r'<path transform="([^"]*)" d="([^"]*)"/>', master)
# The whole shape with the inside filled (used behind the mark on dark surfaces).
sil_paths = re.findall(r'<path transform="([^"]*)" d="([^"]*)"/>',
                       (B / "source" / "vimora-silhouette.svg").read_text())
# Tight box of the emblem in the master's coordinates.
EX0, EY0, EX1, EY1 = 356, 171, 2152, 2348
EW, EH = EX1 - EX0, EY1 - EY0
WORD_D = (B / "tools" / "wordmark-vimora-daysone.d").read_text().strip()
# Advance width and capital height of the word, at the size the path was made for.
WORD_ADV, WORD_CAP = [float(x) for x in (B / "tools" / "wordmark-vimora-daysone.txt").read_text().split()]


def emblem(fill, stroke=0):
    """The mark in one colour. `stroke` thickens it a little (tiny tray sizes)."""
    inner = "\n".join(f'    <path transform="{t}" d="{d}"/>' for t, d in paths)
    extra = f' stroke="{fill}" stroke-width="{stroke}" stroke-linejoin="round"' if stroke else ""
    return f'  <g fill="{fill}" fill-rule="evenodd"{extra}>\n{inner}\n  </g>\n'


def solid(green=GREEN, white=WHITE):
    """The mark as on white paper: green shapes, the face and the hat band filled white.
    This is the one drawn inside the programs, so it reads on dark surfaces too."""
    back = "\n".join(f'    <path transform="{t}" d="{d}"/>' for t, d in sil_paths)
    return f'  <g fill="{white}">\n{back}\n  </g>\n' + emblem(green)


def tile(radius=0.225):
    """App icon: a rounded dark square (like a desktop app's tile) with the green mark."""
    side = max(EW, EH) / 0.68
    cx, cy = EX0 + EW / 2, EY0 + EH / 2
    x, y = cx - side / 2, cy - side / 2
    r = side * radius
    grad = ('  <defs>\n    <linearGradient id="t" x1="0" y1="0" x2="0" y2="1">\n'
            '      <stop offset="0" stop-color="#2B2E27"/>\n'
            '      <stop offset="1" stop-color="#12140F"/>\n    </linearGradient>\n  </defs>\n')
    body = (grad + f'  <rect x="{x:g}" y="{y:g}" width="{side:g}" height="{side:g}" '
            f'rx="{r:g}" fill="url(#t)"/>\n' + emblem(GREEN))
    return (x, y, side, side), body


def svg(vb, body, title="Vimora"):
    x, y, w, h = vb
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x:g} {y:g} {w:g} {h:g}">\n'
            f'  <title>{title}</title>\n{body}</svg>\n')


def pad_box(x, y, w, h, k=0.06):
    p = max(w, h) * k
    return (x - p, y - p, w + 2 * p, h + 2 * p)


def square_box(k=0.0):
    side = max(EW, EH) * (1 + k)
    cx, cy = EX0 + EW / 2, EY0 + EH / 2
    return (cx - side / 2, cy - side / 2, side, side)


def wordmark(fill, scale, x, baseline):
    return (f'  <path fill="{fill}" transform="translate({x:g} {baseline:g}) scale({scale:g})" '
            f'd="{WORD_D}"/>\n')


def horizontal(emblem_fill, text_fill):
    cap = 0.34 * EH
    s = cap / WORD_CAP
    gap = 0.09 * EH
    tx = EX1 + gap
    base = EY0 + EH / 2 + cap / 2
    w = EW + gap + WORD_ADV * s
    vb = pad_box(EX0, EY0, w, EH)
    return svg(vb, emblem(emblem_fill) + wordmark(text_fill, s, tx, base))


def vertical(emblem_fill, text_fill):
    s = (EW * 1.25) / WORD_ADV
    cap = WORD_CAP * s
    gap = 0.07 * EH
    tw = WORD_ADV * s
    tx = EX0 + EW / 2 - tw / 2
    base = EY1 + gap + cap
    vb = pad_box(EX0 + EW / 2 - tw / 2, EY0, tw, base - EY0)
    return svg(vb, emblem(emblem_fill) + wordmark(text_fill, s, tx, base))


def write(path, text):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def png(svg_path, out, width, height=None):
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    args = [RESVG, "-w", str(width)]
    if height:
        args += ["-h", str(height)]
    subprocess.run(args + [str(svg_path), str(out)], check=True)


def both(svg_path, png_path, width):
    png(svg_path, png_path, width)


def main():
    # --- primary and monochrome emblems -----------------------------------------------
    vb = pad_box(EX0, EY0, EW, EH, 0.0)
    vb = square_box(0.12)
    for name, fill, d in [
        ("vimora", GREEN, "primary"),
        ("vimora-black", BLACK, "monochrome"),
        ("vimora-white", WHITE, "monochrome"),
        ("vimora-green", GREEN, "monochrome"),
    ]:
        p = B / "logo" / d / f"{name}.svg"
        write(p, svg(vb, emblem(fill)))
        png(p, p.with_suffix(".png"), 1024)

    p = B / "logo" / "primary" / "vimora-solid.svg"
    write(p, svg(square_box(0.12), solid()))
    png(p, p.with_suffix(".png"), 1024)

    # --- lockups -------------------------------------------------------------------------
    for kind, fn in [("horizontal", horizontal), ("vertical", vertical)]:
        for name, e, t in [
            (f"vimora-{kind}", GREEN, INK),             # for light backgrounds
            (f"vimora-{kind}-on-dark", GREEN, WHITE),   # for dark backgrounds
            (f"vimora-{kind}-black", BLACK, BLACK),
            (f"vimora-{kind}-white", WHITE, WHITE),
        ]:
            p = B / "logo" / kind / f"{name}.svg"
            write(p, fn(e, t))
            png(p, p.with_suffix(".png"), 2048 if kind == "horizontal" else 1024)

    # --- app icon: the emblem on a transparent square, optically centred ----------------
    icon_box, icon_body = tile()
    app_svg = B / "app-icons" / "vimora-app-icon.svg"
    write(app_svg, svg(icon_box, icon_body))
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256, 512, 1024]
    for s in sizes:
        png(app_svg, B / "app-icons" / "png" / f"vimora-{s}.png", s)
    # Windows ICO: one PNG-compressed image per size (each rendered from the vector).
    ico_sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    write_ico(B / "app-icons" / "windows" / "vimora.ico",
              [B / "app-icons" / "png" / f"vimora-{s}.png" for s in ico_sizes], ico_sizes)

    # --- system tray: truly one colour, transparent ----------------------------------------
    # light panel -> black icon, dark panel -> white icon.
    tray_box = square_box(0.04)
    for folder, fill in [("light", BLACK), ("dark", WHITE)]:
        p = B / "system-tray" / folder / f"vimora-tray-{folder}.svg"
        write(p, svg(tray_box, emblem(fill, stroke=22)))
        for s in [16, 20, 24, 32, 48, 64]:
            png(p, B / "system-tray" / folder / f"vimora-tray-{folder}-{s}.png", s)

    # --- what the builds read: assets/brand ---------------------------------------------------
    dst = ROOT / "assets" / "brand"
    for f in dst.glob("vimora*"):
        f.unlink()
    shutil.copy(app_svg, dst / "vimora.svg")
    shutil.copy(B / "app-icons" / "windows" / "vimora.ico", dst / "vimora.ico")
    for s in [16, 24, 32, 48, 64, 128, 256, 512]:
        shutil.copy(B / "app-icons" / "png" / f"vimora-{s}.png", dst / f"vimora-{s}.png")
    # The two tray icons (64 px; the program picks one by the system theme).
    shutil.copy(B / "system-tray" / "light" / "vimora-tray-light-32.png", dst / "tray-black-32.png")
    shutil.copy(B / "system-tray" / "dark" / "vimora-tray-dark-32.png", dst / "tray-white-32.png")
    # The emblem the programs draw inside their windows (one size, scaled by the GPU).
    solid_svg = B / "logo" / "primary" / "vimora-solid.svg"
    for s_ in [32, 48, 64, 128, 256, 512]:
        png(solid_svg, dst / f"logo-{s_}.png", s_)
    print("done")


def write_ico(out, pngs, sizes):
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    datas = [Path(p).read_bytes() for p in pngs]
    head = struct.pack("<HHH", 0, 1, len(datas))
    offset = 6 + 16 * len(datas)
    entries = b""
    for s, data in zip(sizes, datas):
        entries += struct.pack("<BBBBHHII", 0 if s >= 256 else s, 0 if s >= 256 else s,
                               0, 0, 1, 32, len(data), offset)
        offset += len(data)
    out.write_bytes(head + entries + b"".join(datas))


if __name__ == "__main__":
    sys.exit(main())
