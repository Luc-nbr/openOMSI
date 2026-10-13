#!/usr/bin/env python3
"""Writes ui/js/omsihub/symbols.js: the Material Symbols Omsi-Hub's look of the page uses, by
their Material names (as our game-drawn launcher names them), from the game's own copies in
assets/icons/material. Upstream's ui/js/symbols.js (tools/symbols.py) stays as it is.

    python3 launcher/tools/omsihub-symbols.py
"""
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
ICONS = HERE.parent.parent / "assets" / "icons" / "material"
OUT = HERE.parent / "ui" / "js" / "omsihub" / "symbols.js"

NAMES = [
    "add", "arrow_back", "autorenew", "badge", "calendar_month", "check", "chevron_left",
    "chevron_right", "close", "cloud", "construction", "dark_mode", "delete", "description",
    "directions_bus", "directions_walk", "expand_less", "expand_more", "extension", "folder_open",
    "foggy", "garage", "grid_view", "groups", "help", "history", "home", "info", "keyboard",
    "language", "light_mode", "location_on", "lock", "logout", "map", "near_me", "open_in_new",
    "palette", "partly_cloudy_day", "person", "photo_camera", "play_arrow", "public", "rainy",
    "refresh", "remove", "restart_alt", "route", "schedule", "search", "settings",
    "sports_esports", "star", "tune", "view_list", "warning", "wb_sunny", "weather_snowy",
]

paths = {}
for name in NAMES:
    svg = (ICONS / f"{name}.svg").read_text()
    paths[name] = " ".join(re.findall(r'<path[^>]* d="([^"]+)"', svg))
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(
    "// Google's Material Symbols Rounded (filled, Apache-2.0) by their own names, as our\n"
    "// game-drawn launcher draws them (assets/icons/material). Made by\n"
    "// launcher/tools/omsihub-symbols.py; viewBox 0 -960 960 960.\n"
    "export const SYMBOLS = " + json.dumps(paths, indent=1) + ";\n",
    newline="\n",
)
print(f"{len(paths)} symbols -> {OUT}")
