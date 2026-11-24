#!/usr/bin/env python3
"""SoftPresenter's sample slide backgrounds, drawn from nothing but numbers.

No photographs, stock pictures or outside generators go in: every picture is light added to a dark field in this
script, then a fine grain on top (as the brand guide has it: light in a dark room, and grain). Running it again draws
the same pictures (the grain is seeded).

    python3 make_backgrounds.py [folder]      # needs numpy and Pillow; the motion background also needs ffmpeg

Each still is 1920 x 1080. The words of a slide sit in the built-in theme's boxes (the main line from y 170 to 570,
the second language from 610 to 930), so the light stays low there: see contrast.txt, which this script writes.
"""
import math
import os
import subprocess
import sys

import numpy as np
from PIL import Image

W, H = 1920, 1080
ARGS = [a for a in sys.argv[1:] if not a.startswith('--')]
OUT = ARGS[0] if ARGS else os.path.dirname(os.path.abspath(__file__))
X = np.arange(W, dtype=np.float32)[None, :]
Y = np.arange(H, dtype=np.float32)[:, None]


# ── light is added in linear light, then turned into sRGB ─────────────────────────────────────────────────────────
def lin(hexcol):
    c = np.array([int(hexcol[i:i + 2], 16) / 255 for i in (1, 3, 5)], dtype=np.float32)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(img):
    img = np.clip(img, 0, 1)
    return np.where(img <= 0.0031308, img * 12.92, 1.055 * np.power(img, 1 / 2.4) - 0.055) * 255


def field(colour):
    return np.broadcast_to(lin(colour), (H, W, 3)).astype(np.float32).copy()


def glow(cx, cy, rx, ry, colour, strength):
    """A soft round light, centred at (cx, cy), falling off over rx by ry."""
    g = np.exp(-((X - cx) ** 2) / (2 * rx * rx)) * np.exp(-((Y - cy) ** 2) / (2 * ry * ry))
    return g[..., None] * lin(colour) * strength


def vgrad(top, bottom, curve=1.0):
    t = (Y / (H - 1)) ** curve
    return (lin(top) * (1 - t)[..., None] + lin(bottom) * t[..., None]) * np.ones((1, W, 1), np.float32)


def beam(x0, y0, angle, width, colour, strength, fade):
    """A soft beam slanting up to the right from (x0, y0), `angle` degrees from level, fading along its length."""
    a = math.radians(angle)
    dx, dy = math.cos(a), -math.sin(a)
    along = (X - x0) * dx + (Y - y0) * dy
    across = (X - x0) * (-dy) + (Y - y0) * dx
    g = np.exp(-(across ** 2) / (2 * width * width)) * np.exp(-np.clip(along, 0, None) / fade) * (along > -width)
    return g[..., None] * lin(colour) * strength


def vignette(img, amount):
    r = np.sqrt(((X - W / 2) / (W / 2)) ** 2 + ((Y - H / 2) / (H / 2)) ** 2)
    return img * (1 - amount * np.clip(r - 0.35, 0, None) ** 1.6)[..., None]


def grain(shape, sigma, seed):
    return np.random.default_rng(seed).normal(0, sigma, shape[:2]).astype(np.float32)[..., None]


def finish(img, seed, sigma=3.0):
    """sRGB, then the grain: the same speck in every channel, as film grain is."""
    out = to_srgb(img) + grain(img.shape, sigma, seed)
    return np.clip(np.round(out), 0, 255).astype(np.uint8)


def smoothstep(e0, e1, v):
    t = np.clip((v - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def ridge(seed, base, amp, waves):
    """A line of hills: a few slow waves added together."""
    rng = np.random.default_rng(seed)
    y = np.full(W, base, np.float32)
    for k, (length, a) in enumerate(waves):
        y += amp * a * np.sin(2 * math.pi * np.arange(W) / length + rng.uniform(0, 2 * math.pi))
    return y


# ── the stills ────────────────────────────────────────────────────────────────────────────────────────────────────
def still():
    """The plainest: a warm charcoal with the faintest light from above. For when only the words matter."""
    img = field('#111113') + glow(W * 0.5, -120, 1000, 600, '#3a3a40', 1.0)
    return vignette(img, 0.35)


def first_light():
    """The brand's beam, in a warm white instead of orange (orange means "on screen" in the app): one soft beam and
    one fainter beside it, slanting up to the right at about 50 degrees across the whole picture, kept faint where
    the words sit."""
    img = field('#09090a')
    img += beam(560, 1300, 50, 190, '#e3d6c0', 0.085, 2600)
    img += beam(980, 1300, 50, 70, '#e3d6c0', 0.035, 2400)
    img += glow(1700, 40, 600, 300, '#3a352e', 0.25)
    return vignette(img, 0.30)


def night_blue():
    """A deep blue with a soft light from the upper left."""
    img = vgrad('#0d1626', '#070b14', 0.9)
    img += glow(420, 60, 820, 520, '#2b4a74', 0.55)
    img += glow(1650, 980, 700, 380, '#0f1d33', 0.6)
    return vignette(img, 0.35)


def evening_hills():
    """Dusk over three lines of hills, flat shapes in blue greys, the nearest darkest. The hills stay low and dark,
    so the second language reads over them."""
    sky = vgrad('#121522', '#3a2f3d', 1.6)
    sky += glow(W * 0.62, 820, 900, 170, '#5a4552', 0.45)
    img = sky
    layers = [
        (ridge(11, 800, 26, [(1900, 1.0), (740, 0.45), (310, 0.15)]), '#262836'),
        (ridge(12, 862, 30, [(1500, 1.0), (620, 0.5), (240, 0.12)]), '#1a1b26'),
        (ridge(13, 950, 34, [(2300, 1.0), (880, 0.4), (360, 0.15)]), '#0f1017'),
    ]
    for line, colour in layers:
        cover = smoothstep(-1.0, 1.0, Y - line[None, :])[..., None]
        img = img * (1 - cover) + lin(colour) * cover
    return img


def olive():
    """A deep green grey with light from the upper right, like a window in a quiet room."""
    img = vgrad('#101611', '#090c0a', 0.8)
    img += glow(1500, 0, 900, 560, '#3b4b37', 0.55)
    return vignette(img, 0.4)


def room_out_of_focus():
    """A stand-in for a camera behind a livestream's lower third: a church room with the stage far out of focus. Two
    rows of stage lights on their bars become large soft discs, a warm wash lights where someone would stand, and
    the front rows' heads are dark shapes along the bottom."""
    rng = np.random.default_rng(21)
    img = vgrad('#15130f', '#0a0908', 1.0)
    img += glow(W * 0.52, 560, 520, 300, '#6a5238', 0.55)           # the stage, lit where someone stands
    img += glow(W * 0.5, 420, 1200, 360, '#2c2723', 0.55)           # the wall behind it
    img += glow(W * 0.5, 760, 1400, 90, '#3a2f24', 0.35)            # the stage's edge, catching light
    lights = [(160, 70, 0.30), (520, 64, 0.08), (760, 72, 0.22), (1010, 60, 0.06), (1240, 76, 0.26), (1610, 66, 0.10), (1820, 70, 0.18),
              (330, 46, 0.05), (900, 44, 0.12), (1420, 48, 0.05), (1700, 42, 0.07)]
    for k, (cx, r, bright) in enumerate(lights):
        cy = (96 if r > 55 else 250) + rng.uniform(-18, 18)
        cx += rng.uniform(-20, 20)
        d = np.sqrt((X - cx) ** 2 + (Y - cy) ** 2)
        disc = smoothstep(r + 7, r - 7, d) * (0.78 + 0.22 * smoothstep(r * 0.4, r, d))
        colour = '#c9d4ea' if k in (3, 9) else '#f4dcb4'
        img += disc[..., None] * lin(colour) * bright
        img += glow(cx, cy, r * 2.2, r * 2.2, colour, bright * 0.25)       # the haze round each light
    heads = [(80, 1010, 150), (330, 1050, 170), (600, 1000, 140), (880, 1060, 180), (1150, 1015, 150), (1430, 1045, 170), (1720, 1005, 150)]
    for cx, cy, r in heads:                                         # the front rows, out of focus
        d = np.sqrt(((X - cx) / 1.0) ** 2 + ((Y - cy) / 1.15) ** 2)
        img *= (1 - 0.72 * smoothstep(r + 70, r - 40, d))[..., None]
    return vignette(img, 0.3)


STILLS = [
    ('Still', still, 1),
    ('First Light', first_light, 2),
    ('Night Blue', night_blue, 3),
    ('Evening Hills', evening_hills, 4),
    ('Olive', olive, 5),
    ('Room Out of Focus', room_out_of_focus, 6),
]


# ── the motion background: two soft lights drifting on closed paths, so the last frame leads into the first ──────
def drifting_light(path, seconds=16, fps=30):
    frames = seconds * fps
    base = field('#0b0b0c')
    grains = [grain((H, W), 1.6, 100 + k) for k in range(16)]          # 16 divides 480: the grain loops too
    gx = lambda cx, rx: np.exp(-((np.arange(W, dtype=np.float32) - cx) ** 2) / (2 * rx * rx))
    gy = lambda cy, ry: np.exp(-((np.arange(H, dtype=np.float32) - cy) ** 2) / (2 * ry * ry))
    cmd = ['ffmpeg', '-y', '-loglevel', 'error', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-s', f'{W}x{H}', '-r', str(fps),
           '-i', '-', '-c:v', 'libx264', '-preset', 'slow', '-crf', '20', '-tune', 'grain', '-pix_fmt', 'yuv420p',
           '-movflags', '+faststart', '-an', path]
    enc = subprocess.Popen(cmd, stdin=subprocess.PIPE)
    warm, cool = lin('#d8ccb8'), lin('#6f88b0')
    brightest = []
    for f in range(frames):
        t = 2 * math.pi * f / frames                                   # whole turns only, so frame N is frame 0
        ax, ay = W * (0.38 + 0.16 * math.cos(t)), H * (0.30 + 0.10 * math.sin(2 * t))
        bx, by = W * (0.66 + 0.14 * math.cos(t + 2.2)), H * (0.70 + 0.10 * math.sin(t + 0.7))
        a = np.outer(gy(ay, 300), gx(ax, 520))[..., None] * warm * (0.075 + 0.015 * math.sin(t))
        b = np.outer(gy(by, 260), gx(bx, 480))[..., None] * cool * (0.065 + 0.015 * math.cos(t))
        img = vignette(base + a + b, 0.3)
        out = np.clip(np.round(to_srgb(img) + grains[f % 16]), 0, 255).astype(np.uint8)
        enc.stdin.write(out.tobytes())
        if f % 30 == 0:
            brightest.append(out)
    enc.stdin.close()
    enc.wait()
    return brightest


def contrast_report(pics):
    """The brightest of each text box (the 99th percentile, so a stray speck of grain doesn't count), and how far
    white and the second language's colour stand out from it."""
    def lum(srgb):
        c = srgb / 255
        c = np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)
        return 0.2126 * c[..., 0] + 0.7152 * c[..., 1] + 0.0722 * c[..., 2]
    second = np.array([0xd2, 0xce, 0xc6], np.float32)
    rows = ['picture             | main box: brightest  white | second box: brightest  white  #d2cec6',
            '--------------------|---------------------------- |------------------------------------------']
    for name, px in pics:
        L = lum(px.astype(np.float32))
        top = float(np.percentile(L[170:570, 120:1800], 99))
        bot = float(np.percentile(L[610:930, 120:1800], 99))
        ratio = lambda a, b: (max(a, b) + 0.05) / (min(a, b) + 0.05)
        ls = float(lum(second))
        rows.append(f'{name:<20}| {top:.3f}              {ratio(1, top):5.1f} | {bot:.3f}                {ratio(1, bot):5.1f}  {ratio(ls, bot):5.1f}')
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    os.makedirs(OUT, exist_ok=True)
    made = []
    for name, draw, seed in STILLS:
        px = finish(draw(), seed)
        Image.fromarray(px).save(os.path.join(OUT, f'{name}.jpg'), quality=93, subsampling=0, optimize=True)
        made.append((name, px))
        print(name)
    with open(os.path.join(OUT, 'contrast.txt'), 'w') as f:
        f.write(contrast_report(made))
    if '--stills' not in sys.argv:
        frames = drifting_light(os.path.join(OUT, 'Drifting Light.mp4'))
        print('Drifting Light')
        lines = contrast_report([(f'Drifting Light {k}s', px) for k, px in enumerate(frames)]).splitlines()[2:]
        worst = min(lines, key=lambda l: float(l.split()[-1]))
        with open(os.path.join(OUT, 'contrast.txt'), 'a') as f:
            f.write(f'{worst}   (of its {len(frames)} seconds, the one where the second language stands out least)\n')
