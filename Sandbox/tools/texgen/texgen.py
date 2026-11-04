#!/usr/bin/env python3
"""Sandbox texture generator — the game's visual style in code.

Style (docs/STYLE.md): chunky 32×32 pixel art, 4–6 colour ramps per
material, light from the top-left. Machines: dark riveted steel, brass,
dark wood, rubber; building blocks: white floor, stone, planks, glass.
All textures are original (generated here); redraw any PNG by hand —
the game loads assets/textures/<name>.png.

Usage:  python3 tools/texgen/texgen.py [--out assets/textures] [--sheet sheet.png]
"""

import argparse
import os

import numpy as np
from PIL import Image

import os

import numpy as np
from PIL import Image

N = 32


def hexc(h):
    h = h.lstrip("#")
    return np.array([int(h[i : i + 2], 16) for i in (0, 2, 4)], dtype=np.float32)


def pal(*hs):
    return np.stack([hexc(h) for h in hs])


# --- noise -------------------------------------------------------------------


def value_noise(seed, cells):
    """Tileable value noise on an N×N grid with `cells` lattice cells."""
    rng = np.random.default_rng(seed)
    g = rng.random((cells, cells))
    y, x = np.mgrid[0:N, 0:N] / N * cells
    x0, y0 = np.floor(x).astype(int), np.floor(y).astype(int)
    fx, fy = x - x0, y - y0
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    x1, y1 = (x0 + 1) % cells, (y0 + 1) % cells
    a, b = g[y0 % cells, x0 % cells], g[y0 % cells, x1]
    c, d = g[y1, x0 % cells], g[y1, x1]
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy


def fbm(seed, base=4, octaves=3):
    t, amp, tot = np.zeros((N, N)), 1.0, 0.0
    for o in range(octaves):
        t += value_noise(seed + o * 17, base * 2**o) * amp
        tot += amp
        amp *= 0.5
    return t / tot


def voronoi(seed, points):
    """Tileable Voronoi: (cell id, distance to the nearest border)."""
    rng = np.random.default_rng(seed)
    pts = rng.random((points, 2)) * N
    y, x = np.mgrid[0:N, 0:N] + 0.5
    best = np.full((N, N), 1e9)
    second = np.full((N, N), 1e9)
    ids = np.zeros((N, N), dtype=int)
    for i, (px, py) in enumerate(pts):
        for ox in (-N, 0, N):
            for oy in (-N, 0, N):
                d = np.hypot(x - px - ox, y - py - oy)
                closer = d < best
                second = np.where(closer, best, np.minimum(second, d))
                ids = np.where(closer, i, ids)
                best = np.where(closer, d, best)
    return ids, second - best


def shade(palette, t):
    """Quantize t∈0..1 to the palette (chunky painted look)."""
    idx = np.clip((t * len(palette)).astype(int), 0, len(palette) - 1)
    return palette[idx]


def bevel(img, k=0.10):
    """Soft block volume: light top-left, dark bottom-right edges."""
    out = img.copy()
    out[0, :] *= 1 + k
    out[:, 0] *= 1 + k * 0.6
    out[-1, :] *= 1 - k * 1.4
    out[:, -1] *= 1 - k
    out[1, :] *= 1 + k * 0.4
    out[-2, :] *= 1 - k * 0.5
    return out


def rgba(rgb, alpha=None):
    a = np.full((N, N), 255.0) if alpha is None else alpha
    return np.dstack([np.clip(rgb, 0, 255), np.clip(a, 0, 255)]).astype(np.uint8)


def lit(t, x=None, y=None):
    """Top-left light gradient inside a 0..1 field."""
    yy, xx = np.mgrid[0:N, 0:N] / (N - 1)
    return t + (0.5 - (xx + yy) / 2) * 0.25


# --- world blocks ---------------------------------------------------------------

GRASS = pal("#2e6a2b", "#3d8633", "#4f9f3b", "#66b545", "#84ca55")
DIRT = pal("#4a2e1d", "#5f3c25", "#76502f", "#8c6338", "#a37645")
STONE = pal("#474b55", "#5a5f69", "#6f757f", "#878d97", "#a1a7b0")
SAND = pal("#c2a167", "#d3b47a", "#e2c78d", "#efd9a3")
BARK = pal("#3a2414", "#4f331c", "#674426", "#7f5731")
WOOD = pal("#6a4324", "#83572e", "#9c6b39", "#b78349", "#c99a5c")
LEAF = pal("#1d4a20", "#296629", "#378334", "#4d9e41", "#6cb653")
WATER = pal("#1b5aa3", "#2771bf", "#3a8bd4", "#69aee8")
GRAVEL = pal("#4f4c4a", "#66615c", "#7d776f", "#958d82", "#a99f92")
CLAY = pal("#7f8a99", "#919cab", "#a3adba", "#b6bfca")
BED = pal("#17181b", "#24252a", "#33353b", "#45474e")


def stone_like(seed, palette, points=7, crack=1.0):
    ids, edge = voronoi(seed, points)
    rng = np.random.default_rng(seed + 5)
    tone = rng.random(points)[ids] * 0.35 + 0.35
    t = tone + (fbm(seed, 4) - 0.5) * 0.35
    t = lit(t)
    # Each rock is lit from the top-left inside its own cell.
    y, x = np.mgrid[0:N, 0:N]
    t += np.where(edge < 1.6, -0.45 * crack, 0) + np.where((edge >= 1.6) & (edge < 2.6), 0.12, 0)
    return shade(palette, np.clip(t, 0, 0.999))




# --- palettes (see docs/STYLE.md) ----------------------------------------------

WHITE = pal("#c9ccd1", "#d6d9dd", "#e2e4e7", "#eceef0", "#f6f7f8")
STEEL = pal("#26282d", "#33363c", "#43474e", "#555a62", "#6b7079")
ANDESITE = pal("#5c615e", "#6f7570", "#848a84", "#9aa099", "#b2b7b0")
BRASS = pal("#6e4a1a", "#966827", "#bf8c36", "#dfb04f", "#f4d483")
COPPER = pal("#6a3220", "#8c4429", "#b05b33", "#cf7746", "#e8996a")
DARKWOOD = pal("#3b2414", "#4f321b", "#664225", "#7d5430", "#94673c")
RUBBER = pal("#17171a", "#202024", "#2a2a2f", "#36363c")
RED = pal("#4a1512", "#64201a", "#7f2c22", "#9a3a2c", "#b44c38")


def t_white():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.6 + (fbm(1, 8) - 0.5) * 0.12
    img = shade(WHITE, np.clip(lit(t), 0, 0.999))
    img[(x == 0) | (y == 0)] = WHITE[-1]
    img[(x == N - 1) | (y == N - 1)] = WHITE[0]
    return rgba(img)


def t_stone():
    return rgba(bevel(stone_like(11, STONE)))


def t_planks():
    y, x = np.mgrid[0:N, 0:N]
    board = y // 8
    t = 0.45 + (np.random.default_rng(91).random(4)[board] - 0.5) * 0.3
    t += np.sin(x * 0.35 + board * 2 + fbm(92, 4) * 4) * 0.12 + (fbm(93, 16) - 0.5) * 0.15
    img = shade(DARKWOOD, lit(np.clip(t + 0.1, 0, 0.999)))
    img[y % 8 == 7] = DARKWOOD[0]
    img[y % 8 == 0] = img[y % 8 == 0] * 1.12
    for b in range(4):
        for nx in (3, 28):
            img[b * 8 + 3, nx] = hexc("#8a8f98")
    return rgba(bevel(img))


def t_glass():
    img = np.full((N, N, 3), hexc("#bfe6f2"))
    a = np.zeros((N, N))
    border = np.zeros((N, N), bool)
    border[:2, :] = border[-2:, :] = border[:, :2] = border[:, -2:] = True
    img[border] = hexc("#e8fbff")
    a[border] = 255
    for i in range(6):
        img[6 + i, 20 - i] = hexc("#ffffff")
        a[6 + i, 20 - i] = 255
    return rgba(img, a)


def riveted(palette, seed, frame=4, inner=None, bolts=True):
    """Riveted frame (Create-like casing) around an inner texture."""
    y, x = np.mgrid[0:N, 0:N]
    t = lit(0.5 + (fbm(seed, 8) - 0.5) * 0.3)
    img = shade(palette, np.clip(t, 0, 0.999))
    if inner is not None:
        border = (x < frame) | (x > N - 1 - frame) | (y < frame) | (y > N - 1 - frame)
        img = np.where(border[..., None], img, inner)
        img[(x == frame) & ~border] = palette[0]
        img[(y == frame) & ~border] = palette[0]
    img[(x == 0) | (y == 0)] = palette[-1]
    img[(x == N - 1) | (y == N - 1)] = palette[0]
    if bolts:
        c = frame // 2
        for cx, cy in ((c, c), (N - 1 - c, c), (c, N - 1 - c), (N - 1 - c, N - 1 - c)):
            img[cy - 1 : cy + 1, cx - 1 : cx + 1] = BRASS[2]
            img[cy - 1, cx - 1] = BRASS[-1]
            img[cy, cx] = BRASS[0]
    return img


def t_steel():
    y, x = np.mgrid[0:N, 0:N]
    img = riveted(STEEL, 21)
    img[(x % 16 == 15) | (y % 16 == 15)] = STEEL[0]
    for cx in (8, 24):
        for cy in (8, 24):
            img[cy, cx] = STEEL[-1]
            img[cy + 1, cx + 1] = STEEL[0]
    return rgba(img)


def t_steel_frame():
    """Open frame: steel border, see-through middle (alpha cutout)."""
    y, x = np.mgrid[0:N, 0:N]
    img = riveted(STEEL, 22, frame=5)
    a = np.full((N, N), 255.0)
    hole = (x > 5) & (x < N - 6) & (y > 5) & (y < N - 6)
    a[hole] = 0
    img[(x == 6) & ~hole] = STEEL[0]
    return rgba(img, a)


def t_steel_casing():
    inner = np.array(t_planks()[..., :3], dtype=np.float32) * 0.9
    return rgba(riveted(STEEL, 23, frame=4, inner=inner))


def t_brass_casing():
    inner = np.array(t_planks()[..., :3], dtype=np.float32) * 0.9
    return rgba(riveted(BRASS, 24, frame=4, inner=inner))


def plate(palette, seed):
    y, x = np.mgrid[0:N, 0:N]
    t = lit(0.5 + (fbm(seed, 4) - 0.5) * 0.25 + np.sin((x + y) * 0.25) * 0.05)
    img = shade(palette, np.clip(t, 0, 0.999))
    img[(x % 16 == 15) | (y % 16 == 15)] = palette[0]
    img[(x % 16 == 0) | (y % 16 == 0)] = palette[-2]
    return rgba(bevel(img))


def t_andesite():
    ids, edge = voronoi(301, 10)
    tone = np.random.default_rng(302).random(10)[ids] * 0.4 + 0.3
    t = lit(tone + (fbm(303, 16) - 0.5) * 0.3)
    img = shade(ANDESITE, np.clip(t, 0, 0.999))
    img[edge < 0.9] = ANDESITE[1]
    return rgba(bevel(img))


def t_shaft():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.45 + 0.3 * np.cos((x - N / 2) / N * 3.0) + (fbm(321, 16) - 0.5) * 0.15
    img = shade(ANDESITE, np.clip(t, 0, 0.999))
    img[(y % 8 == 0)] = ANDESITE[1]
    return rgba(img)


def t_cogwood():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.5 + np.sin(y * 0.5 + fbm(331, 4) * 4) * 0.15 + (fbm(332, 16) - 0.5) * 0.25
    return rgba(bevel(shade(DARKWOOD, lit(np.clip(t + 0.15, 0, 0.999))), 0.14))


def t_rubber():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.45 + (fbm(341, 8) - 0.5) * 0.2
    img = shade(RUBBER, np.clip(t, 0, 0.999))
    img[((y + (x // 8) * 4) % 8 < 2)] = hexc("#0f0f11")
    return rgba(img)


def t_red_grip():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.5 + np.sin(x * 0.9) * 0.15 + (fbm(351, 8) - 0.5) * 0.15
    return rgba(shade(RED, lit(np.clip(t, 0, 0.999))))


def t_leather():
    t = 0.5 + (fbm(361, 8) - 0.5) * 0.3
    return rgba(bevel(shade(pal("#3c2414", "#55341d", "#6e4628", "#8a5b35"), lit(np.clip(t, 0, 0.999)))))


def t_player():
    """No skin: a plain light grey with soft volume."""
    t = 0.55 + (fbm(371, 8) - 0.5) * 0.08
    return rgba(shade(pal("#9da2a8", "#aeb2b7", "#bfc2c6", "#cfd2d5"), np.clip(lit(t), 0, 0.999)))


TEXTURES = {
    # building blocks
    "white": t_white, "stone": t_stone, "planks": t_planks, "glass": t_glass,
    "steel": t_steel, "steel_frame": t_steel_frame,
    # machine materials (also used as block faces)
    "steel_casing": t_steel_casing, "brass_casing": t_brass_casing,
    "brass": lambda: plate(BRASS, 314), "copper": lambda: plate(COPPER, 315),
    "andesite": t_andesite, "shaft": t_shaft, "cogwood": t_cogwood,
    "rubber": t_rubber, "red_grip": t_red_grip, "leather": t_leather,
    "player": t_player,
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="assets/textures")
    ap.add_argument("--sheet", default=None)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    tiles = []
    for name, fn in TEXTURES.items():
        arr = fn()
        Image.fromarray(arr, "RGBA").save(os.path.join(args.out, name + ".png"))
        tiles.append(arr)
    if args.sheet:
        cols, scale = 6, 6
        rows = (len(tiles) + cols - 1) // cols
        sheet = Image.new("RGBA", (cols * N * scale, rows * N * scale), (40, 40, 48, 255))
        for i, arr in enumerate(tiles):
            im = Image.fromarray(arr, "RGBA").resize((N * scale, N * scale), Image.NEAREST)
            sheet.paste(im, ((i % cols) * N * scale, (i // cols) * N * scale), im)
        sheet.save(args.sheet)
    print(f"{len(tiles)} textures written to {args.out}")


if __name__ == "__main__":
    main()
