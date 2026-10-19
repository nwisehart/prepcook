#!/usr/bin/env python3
"""Regenerates every Prepcook brand file from the geometry below.

    python3 assets/brand/build.py            # from the repository root

Needs Python 3 with `cairosvg` and `Pillow` (`pip install cairosvg pillow`).

The mark is a tall pleated toque drawn from first principles: a flared crown
with a scalloped top edge and pleat slots cut out as holes, over a smooth
band. Nothing is traced from clip art, a logo or a font glyph. The wordmark in
the logo files is ordinary text set in the viewer's sans-serif.

Outputs (all overwritten):
  assets/brand/prepcook-mark.svg            the hat alone, ink on transparent (light backgrounds)
  assets/brand/prepcook-mark-dark.svg       the hat alone, off-white on transparent (dark backgrounds)
  assets/brand/prepcook-mark-mono.svg       single colour, inherits currentColor
  assets/brand/prepcook-app-icon.svg        white hat on the blue tile (favicons, app icons)
  assets/brand/prepcook-logo.svg            mark + "Prepcook" for light backgrounds
  assets/brand/prepcook-logo-dark.svg       mark + "Prepcook" for dark backgrounds
  packages/public/favicon.svg               = prepcook-app-icon.svg
  packages/public/favicon-16.png, favicon-32.png, favicon.ico (16/32/48)
  packages/public/apple-touch-icon.png      180 px
  packages/public/favicon-192.png, favicon-512.png
  packages/public/favicon-maskable.png      512 px with the safe-zone padding
  packages/public/prepcook-logo.svg, prepcook-logo-dark.svg
"""
from __future__ import annotations

import io
import os
import sys

try:
    import cairosvg
    from PIL import Image
except ImportError:  # pragma: no cover - a message beats a traceback here
    sys.exit("build.py needs cairosvg and Pillow: pip install cairosvg pillow")

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BRAND = os.path.join(ROOT, "assets", "brand")
PUBLIC = os.path.join(ROOT, "packages", "public")

INK = "#1F2328"        # near-black, light backgrounds
INK_DARK = "#F4F1EA"   # warm off-white, dark backgrounds
BLUE = "#3F6AA6"       # the one accent colour: chef-jacket blue
WHITE = "#FFFFFF"

# Geometry on a 64 x 64 grid.
TOP = 8.0
AMP = 1.4              # scallop height
SCALLOPS = 6
XL_TOP, XR_TOP = 11.0, 53.0
XL_BOT, XR_BOT = 15.5, 48.5
CROWN_BOTTOM = 45.0
CORNER = 2.5
PLEATS = 5
SLOT_W = 1.4
SLOT_TOP, SLOT_BOTTOM = 12.5, 41.5
BAND_TOP, BAND_BOTTOM = 47.5, 57.0
BAND_INSET = 1.0


def hat_paths() -> str:
    """The crown (one evenodd path with pleat slots as holes) and the band."""
    w = XR_TOP - XL_TOP
    sw = w / SCALLOPS
    r = (sw / 2) ** 2 / (2 * AMP) + AMP / 2
    d = [f"M{XL_BOT} {CROWN_BOTTOM - CORNER}", f"L{XL_TOP} {TOP + AMP}"]
    for i in range(SCALLOPS):
        d.append(f"A{r:.2f} {r:.2f} 0 0 1 {XL_TOP + (i + 1) * sw:.2f} {TOP + AMP:.2f}")
    d += [
        f"L{XR_BOT} {CROWN_BOTTOM - CORNER}",
        f"Q{XR_BOT} {CROWN_BOTTOM} {XR_BOT - CORNER} {CROWN_BOTTOM}",
        f"L{XL_BOT + CORNER} {CROWN_BOTTOM}",
        f"Q{XL_BOT} {CROWN_BOTTOM} {XL_BOT} {CROWN_BOTTOM - CORNER} Z",
    ]
    inner = XR_BOT - XL_BOT
    for i in range(PLEATS):
        f = (i + 1) / (PLEATS + 1)
        xb, xt, hw = XL_BOT + inner * f, XL_TOP + w * f, SLOT_W / 2
        d.append(
            f"M{xb - hw:.2f} {SLOT_BOTTOM} L{xt - hw:.2f} {SLOT_TOP} "
            f"L{xt + hw:.2f} {SLOT_TOP} L{xb + hw:.2f} {SLOT_BOTTOM} Z"
        )
    crown = f'<path fill-rule="evenodd" d="{" ".join(d)}"/>'
    band = (
        f'<rect x="{XL_BOT + BAND_INSET:.2f}" y="{BAND_TOP}" '
        f'width="{XR_BOT - XL_BOT - 2 * BAND_INSET:.2f}" height="{BAND_BOTTOM - BAND_TOP}" rx="3"/>'
    )
    return f"{crown}\n  {band}"


def mark_svg(fill: str) -> str:
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="Prepcook">\n'
        f'<g fill="{fill}">\n  {hat_paths()}\n</g>\n</svg>\n'
    )


def app_icon_svg() -> str:
    """White hat, scaled to leave a margin, on a rounded blue tile."""
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="Prepcook">\n'
        f'<rect width="64" height="64" rx="14" fill="{BLUE}"/>\n'
        f'<g fill="{WHITE}" transform="translate(32 32.5) scale(0.74) translate(-32 -32)">\n  {hat_paths()}\n</g>\n'
        "</svg>\n"
    )


def logo_svg(fill: str) -> str:
    """The mark beside the word "Prepcook". 265 x 64; the mark fills the height,
    and the width leaves room for the word at the mark's 40 px type size (about
    182 px of bold sans-serif from x=70, with a margin on the right)."""
    font = "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif"
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 265 64" role="img" aria-label="Prepcook">\n'
        f'<g fill="{fill}">\n  {hat_paths()}\n'
        f'  <text x="70" y="49" font-family="{font}" font-size="40" font-weight="700" letter-spacing="-0.5">Prepcook</text>\n'
        "</g>\n</svg>\n"
    )


def png(svg: str, size: int, pad: float = 0.0) -> Image.Image:
    """Rasterise `svg` at `size`; `pad` (0..0.5) is the fraction of the side left
    as margin on each edge, for maskable icons."""
    inner = int(round(size * (1 - 2 * pad)))
    raw = cairosvg.svg2png(bytestring=svg.encode(), output_width=inner, output_height=inner)
    im = Image.open(io.BytesIO(raw)).convert("RGBA")
    if inner == size:
        return im
    out = Image.new("RGBA", (size, size), BLUE)
    out.alpha_composite(im, ((size - inner) // 2, (size - inner) // 2))
    return out


def write(path: str, text: str) -> None:
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    print("wrote", os.path.relpath(path, ROOT))


def main() -> None:
    os.makedirs(BRAND, exist_ok=True)
    os.makedirs(PUBLIC, exist_ok=True)
    mark, mark_dark, mono, app = mark_svg(INK), mark_svg(INK_DARK), mark_svg("currentColor"), app_icon_svg()
    write(os.path.join(BRAND, "prepcook-mark.svg"), mark)
    write(os.path.join(BRAND, "prepcook-mark-dark.svg"), mark_dark)
    write(os.path.join(BRAND, "prepcook-mark-mono.svg"), mono)
    write(os.path.join(BRAND, "prepcook-app-icon.svg"), app)
    write(os.path.join(BRAND, "prepcook-logo.svg"), logo_svg(INK))
    write(os.path.join(BRAND, "prepcook-logo-dark.svg"), logo_svg(INK_DARK))

    write(os.path.join(PUBLIC, "favicon.svg"), app)
    write(os.path.join(PUBLIC, "prepcook-logo.svg"), logo_svg(INK))
    write(os.path.join(PUBLIC, "prepcook-logo-dark.svg"), logo_svg(INK_DARK))
    for size, name in ((16, "favicon-16.png"), (32, "favicon-32.png"), (180, "apple-touch-icon.png"),
                       (192, "favicon-192.png"), (512, "favicon-512.png")):
        png(app, size).save(os.path.join(PUBLIC, name))
        print("wrote", os.path.relpath(os.path.join(PUBLIC, name), ROOT))
    png(app, 512, pad=0.1).save(os.path.join(PUBLIC, "favicon-maskable.png"))
    print("wrote packages/public/favicon-maskable.png")
    # One .ico holding 16, 32 and 48 px frames, each rendered at its own size.
    # Pillow keeps only frames no larger than the base image, so start from 48.
    frames = [png(app, s) for s in (48, 32, 16)]
    frames[0].save(os.path.join(PUBLIC, "favicon.ico"), format="ICO",
                   sizes=[(48, 48), (32, 32), (16, 16)], append_images=frames[1:])
    print("wrote packages/public/favicon.ico")


if __name__ == "__main__":
    main()
