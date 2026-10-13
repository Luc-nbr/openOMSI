#!/usr/bin/env python3
"""Writes ui/js/omsihub/company/symbols.js: every Material Symbol our game-drawn launcher has
(assets/icons/material), by its Material name, for the bus company's pages - they name the
icons the game-drawn launcher's company draws, which come from the views as they are.

    python3 launcher/tools/omsihub-company-symbols.py
"""
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
ICONS = HERE.parent.parent / "assets" / "icons" / "material"
OUT = HERE.parent / "ui" / "js" / "omsihub" / "company" / "symbols.js"

paths = {}
for svg_path in sorted(ICONS.glob("*.svg")):
    svg = svg_path.read_text()
    d = " ".join(re.findall(r'<path[^>]* d="([^"]+)"', svg))
    if d:
        paths[svg_path.stem] = d
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(
    "// Google's Material Symbols Rounded (filled, Apache-2.0): every one our game-drawn\n"
    "// launcher has (assets/icons/material), by its own name. Made by\n"
    "// launcher/tools/omsihub-company-symbols.py; viewBox 0 -960 960 960.\n"
    "export const SYMBOLS = " + json.dumps(paths, indent=1) + ";\n",
    newline="\n",
)
print(f"{len(paths)} symbols -> {OUT}")
