#!/usr/bin/env python3
"""Aetherwright texture generator — the game's visual style in code.

Style (docs/STYLE_GUIDE.md):
* world blocks — saturated, chunky hand-painted pixels, soft volume
  (lit top-left, darker bottom-right), 4–6 colour ramps per material;
* machines — andesite / brass / copper / wood with frames, bolts, rivets;
* magic — dark rune stone, gold trims, glowing crystals.

All textures are original (generated); nothing is copied from other games.

Usage:  python3 tools/texgen/texgen.py [--out assets/textures] [--sheet out.png]
Writes blocks/<name>.png and materials/<name>.png (32×32). Re-run after
changing palettes; the game hot-reloads block textures.
"""

import argparse
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


def t_stone():
    return rgba(bevel(stone_like(11, STONE)))


def t_dirt():
    t = fbm(21, 3) * 0.6 + fbm(22, 8) * 0.4
    img = shade(DIRT, lit(t))
    rng = np.random.default_rng(23)
    for _ in range(14):  # pebbles
        x, y = rng.integers(0, N - 2, 2)
        img[y : y + 2, x : x + 2] = DIRT[4] if rng.random() < 0.6 else DIRT[0]
    return rgba(bevel(img))


def t_grass():
    t = fbm(31, 4) * 0.55 + fbm(32, 16) * 0.45
    img = shade(GRASS, lit(t))
    rng = np.random.default_rng(33)
    for _ in range(70):  # blade strokes
        x, y = rng.integers(0, N), rng.integers(0, N - 3)
        c = GRASS[4] if rng.random() < 0.5 else GRASS[1]
        img[y : y + 3, x] = c
    for _ in range(4):  # tiny flowers
        x, y = rng.integers(1, N - 1, 2)
        img[y, x] = hexc("#f3e37a") if rng.random() < 0.5 else hexc("#ffffff")
    return rgba(bevel(img, 0.06))


def t_grass_side():
    img = np.array(t_dirt()[..., :3], dtype=np.float32)
    top = np.array(t_grass()[..., :3], dtype=np.float32)
    rng = np.random.default_rng(41)
    depth = 6 + (value_noise(42, 8)[0] * 6).astype(int)
    for x in range(N):
        d = depth[x] + rng.integers(0, 2)
        img[:d, x] = top[:d, x]
        img[d, x] = GRASS[0]  # dark lip under the overhang
        img[min(d + 1, N - 1), x] *= 0.8
    return rgba(bevel(img))


def t_sand():
    t = fbm(51, 4) * 0.5 + (np.sin(np.mgrid[0:N, 0:N][0] * 0.6 + fbm(52, 4) * 5) * 0.5 + 0.5) * 0.3 + fbm(53, 16) * 0.2
    return rgba(bevel(shade(SAND, lit(t)), 0.06))


def t_log_side():
    x = np.mgrid[0:N, 0:N][1]
    t = 0.5 + 0.35 * np.sin(x * 0.9 + fbm(61, 2) * 6) + (fbm(62, 16) - 0.5) * 0.4
    img = shade(BARK, np.clip(t, 0, 0.999))
    for gx in (5, 13, 22, 28):  # deep grooves
        img[:, gx] = BARK[0]
    return rgba(bevel(img))


def t_log_top():
    y, x = np.mgrid[0:N, 0:N] + 0.5
    r = np.hypot(x - N / 2, y - N / 2)
    t = 0.55 + 0.25 * np.sin(r * 1.3) + (fbm(71, 8) - 0.5) * 0.2
    img = shade(WOOD, np.clip(t, 0, 0.999))
    img[r > N / 2 - 3] = shade(BARK, np.clip(fbm(72, 8), 0, 0.999))[r > N / 2 - 3]
    img[(r > N / 2 - 4) & (r <= N / 2 - 3)] = WOOD[0]
    return rgba(bevel(img))


def t_leaves():
    ids, edge = voronoi(81, 18)
    rng = np.random.default_rng(82)
    tone = rng.random(18)[ids] * 0.5 + 0.3
    t = lit(tone + (1 - np.clip(edge / 4, 0, 1)) * -0.25)
    img = shade(LEAF, np.clip(t, 0, 0.999))
    img[edge < 1.0] = LEAF[0] * 0.8
    return rgba(bevel(img, 0.05))


def t_planks():
    y, x = np.mgrid[0:N, 0:N]
    board = y // 8
    t = 0.45 + (np.random.default_rng(91).random(4)[board] - 0.5) * 0.3
    t += np.sin(x * 0.35 + board * 2 + fbm(92, 4) * 4) * 0.12 + (fbm(93, 16) - 0.5) * 0.15
    img = shade(WOOD, lit(np.clip(t, 0, 0.999)))
    img[y % 8 == 7] = WOOD[0]
    img[y % 8 == 0] = img[y % 8 == 0] * 1.12
    for b in range(4):  # nails
        for nx in (3, 28):
            img[b * 8 + 3, nx] = hexc("#3a3a3e")
    return rgba(bevel(img))


def t_bricks():
    y, x = np.mgrid[0:N, 0:N]
    row = y // 8
    off = (row % 2) * 8
    bx = (x + off) // 16
    rng = np.random.default_rng(101)
    tone = rng.random((4, 3))[row, bx % 3] * 0.35 + 0.4
    t = tone + (fbm(102, 8) - 0.5) * 0.25 + ((7 - y % 8) / 7) * 0.12
    img = shade(STONE, np.clip(t, 0, 0.999))
    mortar = (y % 8 == 7) | ((x + off) % 16 == 15)
    img[mortar] = STONE[0]
    return rgba(bevel(img))


def t_glass():
    img = np.full((N, N, 3), hexc("#bfe6f2"))
    a = np.full((N, N), 60.0)
    border = np.zeros((N, N), bool)
    border[:2, :] = border[-2:, :] = border[:, :2] = border[:, -2:] = True
    img[border] = hexc("#e8fbff")
    a[border] = 220
    for i in range(6):  # glints
        img[6 + i, 20 - i] = hexc("#ffffff")
        a[6 + i, 20 - i] = 200
    return rgba(img, a)


def t_metal():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.55 + (fbm(111, 4) - 0.5) * 0.2 + (x - y) / N * 0.15
    img = shade(pal("#4d5560", "#606a76", "#76818e", "#8e99a6", "#a9b3bf"), np.clip(t, 0, 0.999))
    seam = (x % 16 == 15) | (y % 16 == 15)
    img[seam] = hexc("#3b424b")
    for cx in (3, 12, 19, 28):
        for cy in (3, 12, 19, 28):
            img[cy, cx] = hexc("#c8d2dc")
            img[cy + 1, cx + 1] = hexc("#353b43")
    return rgba(bevel(img))


def ore(seed, gem):
    """Stone with a few chunky nuggets (lit top-left, dark outline)."""
    img = stone_like(seed, STONE, points=6)
    y, x = np.mgrid[0:N, 0:N] + 0.5
    rng = np.random.default_rng(seed + 2)
    mask = np.zeros((N, N), bool)
    for _ in range(6):
        cx, cy = rng.uniform(4, N - 4, 2)
        r = rng.uniform(2.0, 3.4)
        d = np.hypot(x - cx, (y - cy) * 1.15)
        blob = d < r
        t = np.clip(0.75 - ((x - cx) + (y - cy)) / (2.5 * r) * 0.5 - d / r * 0.2, 0, 0.999)
        img = np.where(blob[..., None], shade(gem, t), img)
        mask |= blob
    m = mask.astype(np.uint8)
    ring = (np.roll(m, 1, 0) | np.roll(m, -1, 0) | np.roll(m, 1, 1) | np.roll(m, -1, 1)).astype(bool) & ~mask
    img[ring] = STONE[0] * 0.75
    return rgba(bevel(img))


def crystal(seed, palette, base_palette=STONE):
    img = stone_like(seed, base_palette, points=6)
    y, x = np.mgrid[0:N, 0:N]
    rng = np.random.default_rng(seed)
    mask = np.zeros((N, N), bool)
    for _ in range(4):  # shards
        cx, cy, h, w = rng.integers(6, 26), rng.integers(10, 30), rng.integers(10, 20), rng.integers(3, 6)
        shard = (np.abs(x - cx) < w * (1 - (cy - y) / h)) & (y <= cy) & (y > cy - h)
        mask |= shard
        t = 0.3 + (x - cx + w) / (2 * w) * 0.6
        img = np.where(shard[..., None], shade(palette, np.clip(t, 0, 0.999)), img)
        img[shard & (x == cx)] = palette[-1]
    return rgba(bevel(img)), mask


def t_ancient():
    img = stone_like(131, pal("#1c2826", "#283836", "#364a47", "#4a625d", "#5f7a74"), points=5)
    y, x = np.mgrid[0:N, 0:N]
    glyph = np.zeros((N, N), bool)
    glyph[8:24, 15:17] = True
    glyph[8:10, 10:22] = True
    glyph[15:17, 11:21] = True
    glyph[22:24, 10:22] = True
    img[glyph] = hexc("#5fe8c5")
    return rgba(bevel(img))


def t_gravel():
    ids, edge = voronoi(141, 26)
    rng = np.random.default_rng(142)
    tone = rng.random(26)[ids]
    img = shade(GRAVEL, np.clip(lit(tone * 0.8 + 0.1), 0, 0.999))
    img[edge < 1.2] = GRAVEL[0] * 0.75
    return rgba(bevel(img))


def t_bedrock():
    return rgba(bevel(stone_like(151, BED, points=9, crack=1.3)))


def t_tall_grass():
    img = np.zeros((N, N, 3))
    a = np.zeros((N, N))
    rng = np.random.default_rng(161)
    for _ in range(18):
        x = rng.integers(2, N - 2)
        h = rng.integers(10, 30)
        lean = rng.uniform(-0.25, 0.25)
        for k in range(h):
            yy = N - 1 - k
            xx = int(round(x + lean * k))
            if 0 <= xx < N:
                img[yy, xx] = GRASS[min(4, 1 + k * 4 // h)]
                a[yy, xx] = 255
    return rgba(img, a)


def t_berry():
    rgbl = t_leaves()
    img = rgbl[..., :3].astype(np.float32)
    y, x = np.mgrid[0:N, 0:N] + 0.5
    a = np.where(np.hypot(x - N / 2, (y - N * 0.6) * 1.2) < N * 0.48, 255.0, 0.0)
    rng = np.random.default_rng(171)
    for _ in range(9):
        bx, by = rng.integers(4, N - 5), rng.integers(8, N - 5)
        img[by : by + 3, bx : bx + 3] = hexc("#c4223a")
        img[by, bx] = hexc("#ff8a96")
        img[by + 2, bx + 2] = hexc("#7a1022")
    return rgba(img, a)


def t_clay():
    t = fbm(181, 3) * 0.7 + fbm(182, 12) * 0.3
    return rgba(bevel(shade(CLAY, lit(t)), 0.06))


def t_moss():
    img = stone_like(191, STONE)
    m = fbm(192, 4) > 0.5
    moss = shade(GRASS, np.clip(lit(fbm(193, 8)), 0, 0.999))
    img = np.where(m[..., None], moss * 0.92, img)
    return rgba(bevel(img))


def t_water():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.5 + 0.12 * np.sin((x + y * 0.5) * 2 * np.pi / N * 2 + fbm(202, 2) * 3) + (fbm(201, 4) - 0.5) * 0.45
    img = shade(WATER, np.clip(t, 0, 0.999))
    return rgba(img, np.full((N, N), 190.0))


# --- machines (Create-like palette, original drawings) ------------------------

ANDESITE = pal("#5c615e", "#6f7570", "#848a84", "#9aa099", "#b2b7b0")
BRASS = pal("#7a5520", "#a3742c", "#c9963b", "#e6b955", "#f7d88a")
COPPER = pal("#6a3220", "#8c4429", "#b05b33", "#cf7746", "#e8996a")
COGWOOD = pal("#5e3b1f", "#7a4f29", "#956434", "#b07a42", "#c79559")


def t_andesite():
    ids, edge = voronoi(301, 10)
    tone = np.random.default_rng(302).random(10)[ids] * 0.4 + 0.3
    t = lit(tone + (fbm(303, 16) - 0.5) * 0.3)
    img = shade(ANDESITE, np.clip(t, 0, 0.999))
    img[edge < 0.9] = ANDESITE[1]
    return rgba(bevel(img))


def casing(frame_pal, seed, inner="wood"):
    y, x = np.mgrid[0:N, 0:N]
    inner_img = np.array(t_planks()[..., :3], dtype=np.float32) * 0.85
    t = lit(0.5 + (fbm(seed, 8) - 0.5) * 0.3)
    frame = shade(frame_pal, np.clip(t, 0, 0.999))
    border = (x < 4) | (x > N - 5) | (y < 4) | (y > N - 5)
    img = np.where(border[..., None], frame, inner_img)
    # frame bevel + inner shadow
    img[(x == 4) & ~((y < 4) | (y > N - 5))] = frame_pal[0]
    img[(y == 4) & ~((x < 4) | (x > N - 5))] = frame_pal[0]
    img[(x == 0) | (y == 0)] = frame_pal[-1]
    img[(x == N - 1) | (y == N - 1)] = frame_pal[0]
    for cx, cy in ((2, 2), (N - 3, 2), (2, N - 3), (N - 3, N - 3)):  # bolts
        img[cy - 1 : cy + 1, cx - 1 : cx + 1] = hexc("#2e2e30")
        img[cy - 1, cx - 1] = hexc("#d9d9d0")
    return rgba(img)


def plate(palette, seed, rivets=True):
    y, x = np.mgrid[0:N, 0:N]
    t = lit(0.5 + (fbm(seed, 4) - 0.5) * 0.25 + np.sin((x + y) * 0.25) * 0.05)
    img = shade(palette, np.clip(t, 0, 0.999))
    img[(x % 16 == 15) | (y % 16 == 15)] = palette[0]
    img[(x % 16 == 0) | (y % 16 == 0)] = palette[-2]
    if rivets:
        for cx in (3, 12, 19, 28):
            for cy in (3, 12, 19, 28):
                img[cy, cx] = palette[-1]
                img[cy + 1, cx + 1] = palette[0]
    return rgba(bevel(img))


def t_shaft():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.45 + 0.3 * np.cos((x - N / 2) / N * 3.0) + (fbm(321, 16) - 0.5) * 0.2
    img = shade(ANDESITE, np.clip(t, 0, 0.999))
    img[(y % 8 == 0)] = ANDESITE[1]
    return rgba(img)


def t_cogwood():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.5 + np.sin(y * 0.5 + fbm(331, 4) * 4) * 0.15 + (fbm(332, 16) - 0.5) * 0.25
    img = shade(COGWOOD, lit(np.clip(t, 0, 0.999)))
    return rgba(bevel(img, 0.14))


# --- magic (Thaumcraft-like mood, original drawings) ---------------------------

ARCANE = pal("#15141c", "#211f2b", "#2e2b3b", "#3e3a4f", "#524c66")
GOLD = pal("#6b4a12", "#9c6f1c", "#cf9c2c", "#efc451", "#fff0a8")
CRYSTAL = pal("#1a4e78", "#2a7cb8", "#45aee0", "#8be0fa", "#d9fbff")
FLUX = pal("#2a0f3a", "#4d1a6a", "#7a2fa0", "#b05ad6", "#e5a8ff")


def t_arcane_stone():
    y, x = np.mgrid[0:N, 0:N]
    slab = (x // 16) + (y // 16) * 2
    tone = np.random.default_rng(401).random(4)[slab] * 0.3 + 0.35
    t = lit(tone + (fbm(402, 8) - 0.5) * 0.3)
    img = shade(ARCANE, np.clip(t, 0, 0.999))
    img[(x % 16 == 15) | (y % 16 == 15)] = ARCANE[0]
    img[(x % 16 == 0) | (y % 16 == 0)] = ARCANE[-1]
    return rgba(img)


def t_arcane_rune():
    img = np.array(t_arcane_stone()[..., :3], dtype=np.float32)
    glow = np.zeros((N, N), bool)
    y, x = np.mgrid[0:N, 0:N] + 0.5
    r = np.hypot(x - N / 2, y - N / 2)
    glow |= (np.abs(r - 11) < 0.8)
    glow |= (np.abs(x - N / 2) < 0.8) & (r < 11)
    glow |= (np.abs(y - N / 2 - 3) < 0.8) & (r < 8)
    for k in range(6):
        ang = k * np.pi / 3
        gx, gy = N / 2 + np.cos(ang) * 11, N / 2 + np.sin(ang) * 11
        glow |= np.hypot(x - gx, y - gy) < 1.6
    img[glow] = CRYSTAL[3]
    return rgba(img)


def t_arcane_rune_glow():
    """Emissive mask for arcane_rune (black = no glow)."""
    base = t_arcane_rune()[..., :3]
    m = np.all(base == CRYSTAL[3].astype(np.uint8), axis=2)
    img = np.zeros((N, N, 3))
    img[m] = CRYSTAL[3]
    return rgba(img)


def t_gold_trim():
    y, x = np.mgrid[0:N, 0:N]
    t = lit(0.55 + 0.25 * np.sin(y * 0.4) + (fbm(411, 8) - 0.5) * 0.2)
    img = shade(GOLD, np.clip(t, 0, 0.999))
    pattern = ((x + y) % 8 == 0) | ((x - y) % 8 == 0)
    img[pattern & (y > 6) & (y < N - 7)] = GOLD[1]
    img[(y < 3) | (y > N - 4)] = GOLD[3]
    img[(y == 3) | (y == N - 4)] = GOLD[0]
    return rgba(img)


def t_crystal():
    y, x = np.mgrid[0:N, 0:N]
    facet = ((x + y * 2) // 9 + (x - y) // 11) % 4
    t = 0.35 + facet * 0.15 + (fbm(421, 8) - 0.5) * 0.15
    img = shade(CRYSTAL, np.clip(t, 0, 0.999))
    edges = ((x + y * 2) % 9 == 0) | ((x - y) % 11 == 0)
    img[edges] = CRYSTAL[4]
    a = np.where(edges, 230.0, 120.0)
    return rgba(img, a)


def t_flux():
    t = fbm(431, 4) * 0.7 + fbm(432, 16) * 0.3
    img = shade(FLUX, np.clip(t, 0, 0.999))
    rng = np.random.default_rng(433)
    for _ in range(12):
        x, y = rng.integers(0, N, 2)
        img[y, x] = hexc("#ffffff")
    return rgba(img)


def t_greatwood():
    img = np.array(t_planks()[..., :3], dtype=np.float32)
    return rgba(img * np.array([0.55, 0.45, 0.5]))


BLOCKS = {
    "grass": t_grass, "grass_side": t_grass_side, "dirt": t_dirt, "stone": t_stone,
    "sand": t_sand, "water": t_water, "log_side": t_log_side, "log_top": t_log_top,
    "leaves": t_leaves, "iron_ore": lambda: ore(121, pal("#8a5a3a", "#b07a52", "#d6a27a", "#f0cba8")),
    "copper_ore": lambda: ore(123, pal("#7c3a1f", "#b5592f", "#3f9c86", "#78d1b6")),
    "coal_ore": lambda: ore(125, pal("#141416", "#222226", "#34343a", "#4a4a52")),
    "aether": lambda: crystal(127, CRYSTAL)[0], "ancient": t_ancient, "planks": t_planks,
    "bricks": t_bricks, "glass": t_glass, "metal": t_metal, "gravel": t_gravel,
    "bedrock": t_bedrock, "tall_grass": t_tall_grass, "berry": t_berry, "clay": t_clay,
    "moss": t_moss, "agitated_aether": lambda: crystal(129, FLUX)[0],
}

def t_rubber():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.45 + (fbm(341, 8) - 0.5) * 0.2
    img = shade(pal("#1a1a1d", "#242428", "#2f2f34", "#3b3b41"), np.clip(t, 0, 0.999))
    tread = ((y + (x // 8) * 4) % 8 < 2)
    img[tread] = hexc("#121214")
    return rgba(img)


# --- characters (Hytale-like: chunky, readable faces) -------------------------

SKIN = pal("#b9785a", "#cf8e6b", "#e2a67f", "#f0be98")
HAIR = pal("#3a2414", "#55331b", "#704524", "#8c5a30")
TUNIC = pal("#1f4d4f", "#2a6668", "#378284", "#4c9fa0", "#6cbcb8")
PANTS = pal("#3a2b20", "#4b382a", "#5e4635", "#715541")
LEATHER = pal("#3c2414", "#55341d", "#6e4628", "#8a5b35")


def t_skin():
    return rgba(shade(SKIN, np.clip(lit(0.55 + (fbm(501, 8) - 0.5) * 0.15), 0, 0.999)))


def t_face():
    img = np.array(t_skin()[..., :3], dtype=np.float32)
    # Eyes (white, iris, pupil), brows, nose shade and mouth.
    for ex in (8, 19):
        img[14:18, ex : ex + 5] = hexc("#f4f1ec")
        img[14:18, ex + 2 : ex + 4] = hexc("#3b7bb5")
        img[15:17, ex + 2 : ex + 3] = hexc("#14212e")
        img[11:13, ex - 1 : ex + 5] = HAIR[0]
    img[19:21, 15:17] = SKIN[0]
    img[24:25, 12:20] = hexc("#8a4a3a")
    img[25:26, 13:19] = SKIN[1]
    return rgba(img)


def t_hair():
    y, x = np.mgrid[0:N, 0:N]
    t = 0.5 + np.sin(x * 0.9 + fbm(511, 4) * 4) * 0.25 + (fbm(512, 16) - 0.5) * 0.25
    return rgba(bevel(shade(HAIR, np.clip(lit(t), 0, 0.999)), 0.12))


def t_tunic():
    y, x = np.mgrid[0:N, 0:N]
    t = lit(0.5 + (fbm(521, 8) - 0.5) * 0.25)
    img = shade(TUNIC, np.clip(t, 0, 0.999))
    img[(x == 15) & (y % 3 != 0)] = TUNIC[0]  # stitching
    img[(y == 2) | (y == N - 3)] = TUNIC[4]
    return rgba(bevel(img, 0.12))


def t_pants():
    t = lit(0.5 + (fbm(531, 8) - 0.5) * 0.3)
    img = shade(PANTS, np.clip(t, 0, 0.999))
    return rgba(bevel(img, 0.12))


def t_leather():
    y, x = np.mgrid[0:N, 0:N]
    t = lit(0.5 + (fbm(541, 8) - 0.5) * 0.35)
    img = shade(LEATHER, np.clip(t, 0, 0.999))
    img[(y % 16 == 0)] = LEATHER[0]
    img[(y % 16 == 1) & (x % 4 == 0)] = hexc("#d9b26b")  # stitches
    return rgba(bevel(img, 0.14))


def t_hollow_shell():
    img = stone_like(551, pal("#120c18", "#1e1428", "#2b1d3a", "#3c2a50", "#523a6a"), points=7, crack=1.2)
    ids, edge = voronoi(552, 7)
    img[edge < 0.8] = FLUX[3]  # glowing seams
    return rgba(img)


def t_hollow_glow():
    base = t_hollow_shell()[..., :3]
    m = np.all(base == FLUX[3].astype(np.uint8), axis=2)
    img = np.zeros((N, N, 3))
    img[m] = FLUX[3]
    return rgba(img)


MATERIALS = {
    "char_skin": t_skin, "char_face": t_face, "char_hair": t_hair, "char_tunic": t_tunic,
    "char_pants": t_pants, "char_leather": t_leather, "hollow_shell": t_hollow_shell,
    "hollow_glow": t_hollow_glow,
    "rubber": t_rubber,
    "andesite": t_andesite, "andesite_casing": lambda: casing(ANDESITE, 311),
    "brass_casing": lambda: casing(BRASS, 312), "copper_casing": lambda: casing(COPPER, 313),
    "brass": lambda: plate(BRASS, 314), "copper": lambda: plate(COPPER, 315),
    "shaft": t_shaft, "cogwood": t_cogwood,
    "arcane_stone": t_arcane_stone, "arcane_rune": t_arcane_rune,
    "arcane_rune_glow": t_arcane_rune_glow, "gold_trim": t_gold_trim,
    "crystal": t_crystal, "flux": t_flux, "greatwood": t_greatwood,
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="assets/textures")
    ap.add_argument("--sheet", default=None, help="write a contact sheet PNG")
    ap.add_argument("--only", default=None, help="comma-separated names")
    args = ap.parse_args()
    only = set(args.only.split(",")) if args.only else None
    tiles = []
    for folder, table in (("blocks", BLOCKS), ("materials", MATERIALS)):
        os.makedirs(os.path.join(args.out, folder), exist_ok=True)
        for name, fn in table.items():
            if only and name not in only:
                continue
            arr = fn()
            Image.fromarray(arr, "RGBA").save(os.path.join(args.out, folder, name + ".png"))
            tiles.append((name, arr))
    if args.sheet:
        cols = 8
        scale = 6
        rows = (len(tiles) + cols - 1) // cols
        sheet = Image.new("RGBA", (cols * N * scale, rows * N * scale), (40, 40, 48, 255))
        for i, (_, arr) in enumerate(tiles):
            im = Image.fromarray(arr, "RGBA").resize((N * scale, N * scale), Image.NEAREST)
            sheet.paste(im, ((i % cols) * N * scale, (i // cols) * N * scale), im)
        sheet.save(args.sheet)
    print(f"{len(tiles)} textures written to {args.out}")


if __name__ == "__main__":
    main()
