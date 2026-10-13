#!/usr/bin/env python3
"""Writes ui/js/omsihub/editors/symbols.js: the Material Symbols Omsi-Hub's editors in the page
use (`js/omsihub/editors`), by their Material names, from the game's own copies in
assets/icons/material - every icon our game-drawn launcher's editors name (a string literal of
theirs that is the name of one of those icons), and the ones the page's editors name besides.
Kept apart from omsihub-symbols.py's list so that the two parts do not write the same file.

    python3 launcher/tools/omsihub-editor-symbols.py
"""
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
ICONS = REPO / "assets" / "icons" / "material"
OUT = HERE.parent / "ui" / "js" / "omsihub" / "editors" / "symbols.js"
UI = REPO / "crates" / "omsi-launcher-ui" / "src"

# the game-drawn launcher's editors, whose icons the page's draw too
SOURCES = [
    UI / "editor_hub.rs", UI / "timetable.rs", UI / "hof.rs", UI / "lineeditor.rs",
    UI / "depoteditor.rs", UI / "ownlines.rs", UI / "busclass.rs",
] + sorted((UI / "lineeditor").glob("*.rs")) + sorted((UI / "livery").glob("*.rs"))
# what the page's editors use besides
MORE = ["content_copy", "open_with", "departure_board", "livery_fill", "my_location", "zoom_in", "zoom_out"]

# the game's own drawn icons (the livery studio's tools): stroked, in a 24-point box
CUSTOM = REPO / "assets" / "icons" / "custom"

have = {p.stem for p in ICONS.glob("*.svg")}
custom = {p.stem for p in CUSTOM.glob("*.svg")}
names = set(n for n in MORE if n in have)
drawn = set()


def take(lit):
    if lit in have:
        names.add(lit)
    elif lit in custom:
        drawn.add(lit)


for src in SOURCES:
    if src.exists():
        for lit in re.findall(r'"([a-z0-9_]+)"', src.read_text(encoding="utf-8")):
            take(lit)
for js in (HERE.parent / "ui" / "js" / "omsihub" / "editors").rglob("*.js"):
    if js.name != "symbols.js":
        for lit in re.findall(r"""(?:icon|name)[=:]\s*["']([a-z0-9_]+)["']""", js.read_text(encoding="utf-8")):
            take(lit)

paths = {}
for name in sorted(names):
    svg = (ICONS / f"{name}.svg").read_text(encoding="utf-8")
    paths[name] = " ".join(re.findall(r'<path[^>]* d="([^"]+)"', svg))
strokes = {}
for name in sorted(drawn):
    svg = (CUSTOM / f"{name}.svg").read_text(encoding="utf-8")
    strokes[name] = re.findall(r'<path[^>]* d="([^"]+)"', svg)
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(
    "// Google's Material Symbols Rounded (filled, Apache-2.0) by their own names, as our\n"
    "// game-drawn launcher's editors draw them (assets/icons/material), and the game's own\n"
    "// stroked icons (assets/icons/custom: the livery studio's tools, a 24-point box). Made by\n"
    "// launcher/tools/omsihub-editor-symbols.py; viewBox 0 -960 960 960.\n"
    "export const SYMBOLS = " + json.dumps(paths, indent=1) + ";\n\n"
    "export const STROKED = " + json.dumps(strokes, indent=1) + ";\n",
    encoding="utf-8",
    newline="\n",
)
print(f"{len(paths)} symbols, {len(strokes)} stroked -> {OUT}")
