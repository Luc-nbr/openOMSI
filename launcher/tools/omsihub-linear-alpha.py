#!/usr/bin/env python3
"""The rgba alphas that show what our game-drawn launcher's see-through colours show.

It draws into an sRGB target and so blends in linear light: a see-through white on the night
shows much stronger than the same rgba in a browser, and its light look draws a see-through
white as see-through ink (theme.rs: overlay_alpha) that a browser shows darker. The bus
company's stylesheet (ui/omsihub/company.css: --aw-*, --ac-*, the hub's edges and lines on its
screen) and kit.js's `seen` use these:

    python3 launcher/tools/omsihub-linear-alpha.py 0.06 0.09 0.16
"""

import sys

DARK_GROUND = (9, 12, 24)
CREAM = (252, 234, 208)
INK = (22, 20, 26)
ACCENT = (245, 134, 32)


def lin(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def srgb(l):
    return 255.0 * (12.92 * l if l <= 0.0031308 else 1.055 * l ** (1 / 2.4) - 0.055)


def seen(a, top, under):
    """The browser's alpha for `top` at the game-drawn launcher's alpha `a` over `under`."""
    chans = [k for k in range(3) if abs(top[k] - under[k]) > 40]
    xs = [(srgb(a * lin(top[k]) + (1 - a) * lin(under[k])) - under[k]) / (top[k] - under[k]) for k in chans]
    return max(0.0, min(1.0, sum(xs) / len(xs)))


for a in [float(x) for x in sys.argv[1:]] or [0.06, 0.09, 0.12, 0.16]:
    white_dark = seen(a, (255, 255, 255), DARK_GROUND)
    ink_light = seen(min(0.6, 0.85 * a ** 0.5), INK, CREAM)
    accent_dark = seen(a, ACCENT, DARK_GROUND)
    accent_light = seen(a, ACCENT, CREAM)
    print(f"{a:g}: white {white_dark:.3f} dark, {ink_light:.3f} light (as ink) - accent {accent_dark:.3f} dark, {accent_light:.3f} light")
