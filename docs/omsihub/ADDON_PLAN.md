# Omsi-Hub as an openOMSI add-on

**Phase 1:** inventory, design and the additive foundation. Branch `r61-addon`, based on `r61-base`.

**Phase 2:** the migration (§8), done as far as openOMSI's plugin API allows today. Branch
`r62-addon2`, based on `r62-base`. What moved, what stays bundled and why: §10.

## 0. The goal

Luc's words:

> I want Omsi-Hub to work on every version of openOMSI, and to be a plugin you can switch on in
> openOMSI, an add-on. We must no longer be woven into the game code but come in as a plugin.
> When people open Omsi-Hub, they choose their openOMSI folder and it just works, and our
> features reach openOMSI through plugins.

Two further requirements from Luc:

- **The end state lands in openOMSI itself**, as a series of small pull requests to
  `openOMSI-org/openOMSI`, so that "add-ons" become a feature of openOMSI.
- **openOMSI itself only needs one thing:** a switch in its main menu that turns the add-on on.
  Switching it on downloads Omsi-Hub and enables it. Everything else lives in Omsi-Hub and
  reaches the game through the plugin API.

This document covers four things:

1. what Omsi-Hub touches in openOMSI today (§1);
2. what openOMSI's plugin system offers (§2);
3. the target architecture, and where every touch point goes (§3–§4);
4. version compatibility (§5), packaging (§6), the upstream pull requests (§7), the order of
   phase 2 (§8) and what phase 1 built (§9).

---

## 1. Inventory: where Omsi-Hub touches openOMSI

### How this was measured

- The fork's merge base with `upstream/main` is `55e0c68e`. Upstream has not been merged
  since, so every change in `55e0c68e..r61-base` under an upstream crate is Omsi-Hub's.
- `git diff upstream/main...HEAD --stat -- crates/` shows 413 files changed: +207,668 / −18,781
  lines.

| Crate | Files | + / − lines | What it is |
| --- | --- | --- | --- |
| `omsi-app` (the game) | 158 | +84,326 / −18,383 | 21 new files (+16,159), 59 upstream files edited (+4,738 / −679), 77 new locale files, `app.yml` rewritten |
| `omsi-launcher-core` | 82 | +58,203 / −27 | upstream crate; the company, RosterFlow, Play together, the editors' data |
| `omsi-launcher-api` | 76 | +31,983 / −3 | upstream crate; `src/omsihub/` holds the page's commands |
| `omsihub-game` (new) | 29 | +16,296 | the navigator's duty sheet, stop signs, intro/splash, and the editors run by the launcher service |
| `omsihub-livery` (new) | 8 | +6,018 | the livery studio's core |
| `omsi-ui` | 8 | +3,629 / −151 | accent colour, restyle, light look, `Fonts::hanken`, `Style` |
| `omsi-content` | 5 | +2,283 | `dotfont` (the display font), head-tracking keys |
| `omsi-sim` | 20 | +2,237 / −109 | `TimetableCompany`, duty composition, line buses, display font, the duplicate-id fix |
| `omsi-input` | 7 | +1,427 / −42 | Tobii, head filter, force feedback |
| `omsi-vehicle` | 2 | +338 / −3 | `.hof` writer, `depot_dir`, `is_players`, `ai_only` |
| `omsi-timetable` | 2 | +239 | TTData writers |
| `omsi-texture` | 1 | +112 | DDS writer |
| `omsi-platform` | 1 | +114 / −26 | updater pointed at the fork's releases |
| `omsi-career` | 2 | +98 / −8 | `write_trip`; a new dependency on `omsi-launcher-core` |
| `omsi-net` | 4 | +94 / −15 | `http`, `Tunnel::start_with`, `ServerInfo.date` |
| `omsi-render` | 1 | +85 / −1 | non-blocking `Readback` |
| `omsi-launcher-model` | 3 | +73 | the fleet map export |
| `omsi-scene` | 1 | +55 / −5 | `NavigationMap.stops` |
| `omsi-cfg` | 1 | +38 / −1 | about 45 `OMSI_*` flags |
| `omsi-lan` | 2 | +20 / −7 | the date in `ServerInfo`; the duty kept on the host's map |

**Untouched:** `omsi-plugin` and `omsi-plugin-ui`.

**Outside `crates/`:**

- `launcher/` (the Tauri app and its page);
- `.github/workflows/omsihub-release.yml` and `omsihub-sync.yml`;
- `scripts/omsihub-version.sh`;
- `crates/omsi-app/build.rs` (sets "Omsi-Hub" as the executable's ProductName).

### Mapping categories

Every touch point below gets one category:

- **(a)** A plugin on **today's** plugin API, or a channel openOMSI already has (its command
  line, its files). No upstream change is needed.
- **(b)** A plugin that needs a **small upstream extension point**. The extension points are
  defined in §4.
- **(c)** **Stays in the standalone Omsi-Hub app.** This includes:
  - things that do not need the running game;
  - Omsi-Hub's own branding and distribution;
  - changes that are general game improvements unrelated to Omsi-Hub. These are offered to
    openOMSI as ordinary pull requests (or dropped) and are not part of the add-on. They are
    marked **(c, upstream as a game fix)**.

### How data moves today

**Files under `~/.openomsi`:**

| File | Written by | Read by | What it holds |
| --- | --- | --- | --- |
| `settings.cfg` | both | both | settings |
| `launcher.json` | launcher | game | the OMSI 2 root |
| `launcher-duty.json` | launcher | — | the duty chosen |
| `companies/<id>.json` | launcher | — | the company |
| `companies/<id>.liveplan.json`, `<id>.drive.liveplan.json` | launcher | game | the day's plan |
| `companies/<id>.live.jsonl` | game | launcher | what happened on the live day |
| `companies/<id>.orders.jsonl` | companion | launcher | orders from the phone |
| `trips/<driver>.jsonl` | game | launcher | the driver's record |
| `personnel.json` | game | launcher | sign-on details |
| `companion-devices.json` | game | game | paired devices |
| `sessions/*.json` | game | launcher | session summaries |
| `instances/<id>.json`, `lan/<id>.json` | both | both | running games, LAN status |
| `game.log` | game | launcher | the game's log |
| `telemetry.json` | game | other programs | upstream: written while the file exists |

**Other channels:**

- **Command line.** `--root --map --bus --time --line --tour --trip`, and more, are upstream's.
  The fork adds `--company`, `--duty-leg`, `--free-line`, `--display-font` and `--editor`.
- **Environment variables.**
  - `OMSI_INSTANCE`, `OMSI_SCREEN_AT`: from the launcher.
  - `OPENOMSI_BIN`: set by the game when it starts the launcher.
  - `OMSI_COMPANION_HOST`: the companion's address.
  - `OMSI_LAUNCHER`: upstream's hook for an external launcher.
- **Inter-process communication.**
  - `openomsi --launcher-service`: JSON lines on stdin, a head and bytes on stdout.
  - The companion's HTTP server on port 47811 (and a Cloudflare tunnel).
  - Play together on TCP port 27045+.

### The touch points

| # | Touch point (where) | What it does for the player | Data flow | Cat. |
| --- | --- | --- | --- | --- |
| 1 | Branding: `build.rs`, window title (`app.rs:141`), title bar | "Omsi-Hub" in place of "openOMSI" | — | (c) |
| 2 | Launch of a duty: `omsi-launcher-core` `duty_args`, `instances::start` | starts the game with map, bus, time, line, tour, trip, weather, traffic, driver | command line, `OMSI_INSTANCE`, `game.log` | (a) |
| 3 | `--company` and the company live day: `company_live.rs`, `schedule.rs` `impl TimetableCompany`, `omsi-sim` `sim/company.rs` | only the company's own tours run, with its buses, livery, number, plate and `.hof`; the AI's finished trips are booked back | `<id>.liveplan.json` in, `<id>.live.jsonl` out | (b) E6 |
| 4 | Composed duties, relief starts, free drive: `--duty-leg`, `--free-line`, `duty_start.rs`, `omsi-sim` `player_plan` / `free_line_duty` / `between` | a duty made of several tours; starting or ending at a relief stop; a free drive along a route | command line | (b) E7 |
| 5 | Own lines from the line editor in the timetable: `schedule.rs` `line_buses`, `fleet.rs`, `game_lists/duty.rs` (`oo_` lines) | the player's own lines run and can be driven, with the buses chosen for them | `oo_*` TTData and `oo_vehicles.json` in the map folder | (b) E8 |
| 6 | Bus options (`--setvar` before `{init}`): `spawn.rs`, `omsi-sim` `start_vars` | the options chosen on the launcher's bus step | command line | (b) E9 |
| 7 | Display font: `--display-font`, `omsi-sim` `texttex.rs`, `omsi-content` `dotfont` | a chosen font on the destination display | command line, font files | (b) E9 |
| 8 | Player's own depot files: `situation.rs`, `omsi-vehicle` `depot_dir` / `is_players` | the depot editor's `.hof` files; never chosen by name matching | `.hof` files beside the bus | (b) E10 |
| 9 | `--editor` (`people.rs:95`) | opens the map editor once the world is loaded (from the editor hub) | command line | (b) E11 |
| 10 | Trip report card and driver's record: `trip_report.rs` (1,489 lines), `omsi-career` `write_trip` | the end-of-trip card (punctuality, grade, XP, fines); the service record | `trips/<driver>.jsonl` out | (a) |
| 11 | Drive watch: `drive_watch.rs` | red lights, speed cameras, comfort; the offence notice | (into the trip report) | (a) |
| 12 | Navigator duty board: `navigator.rs` with `omsihub_game::nav_duty` | the duty sheet under the minimap | — | (a) |
| 13 | Navigator restyle and bus stop signs: `navigator.rs` +1,132 lines, `city_map.rs` +843, `stop_signs` | Omsi-Hub's map look, H, UK and FR stop signs | — | (b) E12 |
| 14 | Navigator size, place and handle: `nav_panel.rs` | resize and move the minimap | `settings.cfg` | (c, upstream as a game fix) |
| 15 | Navigator pins: `nav_pins.rs` | own destination and waypoints on the map | — | (b) E12 |
| 16 | Signing on in the navigator: `nav_signon.rs` | personnel number and code, accepting the duty order | `personnel.json` | (a) |
| 17 | Companion server and pages: `companion/` (about 9,900 lines) | phone or tablet: sign on, duty, map, Company tab | HTTP :47811, `companion-devices.json`, `orders.jsonl` | (c) |
| 18 | Companion's live cab devices: `companion/device.rs`, `omsi-render` `Readback`, `click_mesh` | the IBIS and ticket machine as touch screens on the phone | GPU readback, mesh clicks | (b) E13 |
| 19 | IBIS filled in automatically: `player/ibis.rs` | the duty's line and destination typed in | `settings.cfg` `ibis_auto` | (a) |
| 20 | Head tracking (Tobii, filter) and force feedback: `omsi-input`, `view_camera.rs` | Tobii eye trackers, smoother tracking, MOZA wheels | `settings.cfg` | (c, upstream as a game fix) |
| 21 | Accent colour, menu restyle, loading screen: `omsi-ui` `accent` / `theme` / `kit`, `ui/menu.rs`, `ui.rs` `hub_loading` | Omsi-Hub's look inside the game | `settings.cfg` `accent` | (c) |
| 22 | Translated service messages: `game_lists*`, `duty_start.rs` | the game's own messages in six languages | `locales/*.yml` | (c, upstream as a game fix) |
| 23 | Omsi-Hub keys in `settings.rs` | the navigator, companion, IBIS, head tracking and accent settings | `settings.cfg` | (c) |
| 24 | LAN: the date in `ServerInfo`, the duty kept on the host's map (`omsi-lan`, `server.rs`) | a friend joining by code drives the host's day | LAN status | (c, upstream as a game fix) |
| 25 | Duplicate object ids across tiles (`omsi-sim` `station_tiles` and others) | timetables on maps joined from several | — | (c, upstream as a game fix) |
| 26 | Host bridge for the editors: `launcher/omsihub.rs` (`omsihub_game::host::Host`) | lends the renderer, scene and navigator roads to the editors | in-process | (c) |
| 27 | The launcher service's Omsi-Hub commands: `launcher/service.rs` (`le`, `studio`, `display_preview`, `photo`, fleet `mapview`) | line editor, livery studio, display previews, bus photos | `--launcher-service` stdin/stdout | (c) |
| 28 | Showroom and studio rendering: `launcher/showroom.rs` +468 | the 3D bus views of the studio and photos | `--launcher-service` | (c) |
| 29 | Offline file writers: `omsi-timetable` `write`, `omsi-vehicle` `.hof` writer, `omsi-texture` DDS writer, `omsi-scene` `NavigationMap.stops` | the line, depot and livery editors save into the game's formats | files in the content folder | (c) |
| 30 | `omsihub-livery` | the livery studio | liveries written as files | (c) |
| 31 | Company, RosterFlow, Play together, editors' data: `omsi-launcher-core` `company/` and other modules, `omsi-launcher-api` `src/omsihub/` | the bus company and planning | `companies/*`, TCP :27045 | (c) |
| 32 | The Tauri launcher's page: `launcher/` | Omsi-Hub's whole interface | — | (c) |
| 33 | The updater pointed at the fork: `omsi-platform` `updater.rs` | updates Omsi-Hub's copy of the game | GitHub releases | (c) |
| 34 | `omsi-net` `http`, `Tunnel::start_with` | Play together; the companion tunnel | HTTP | (c) |
| 35 | Omsi-Hub's `OMSI_*` flags in `omsi-cfg` | test and preview switches | environment | (c) |
| 36 | Locales: 77 files and a rewritten `app.yml` | every Omsi-Hub text in six languages | `locales/*.yml` | (c) |
| 37 | Release workflows: `omsihub-release.yml`, `omsihub-sync.yml`, `scripts/omsihub-version.sh` | ship Omsi-Hub with its own copy of the game; merge upstream every hour | GitHub Actions | (c) |
| 38 | Game status for the launcher: `game.log`, `instances/`, `lan/`, `sessions/` | crash and disconnect notices; session summaries | files | (a) |
| 39 | Live position and duty for the app (fleet map, companion): upstream's `telemetry.json` | where the player's bus is | `~/.openomsi/telemetry.json` | (a) |
| 40 | Opening Omsi-Hub from a bare game start: `launcher_link.rs` (`OPENOMSI_BIN`, upstream's `OMSI_LAUNCHER`) | a plain `openomsi` start opens the launcher | environment | (a) |

**Totals:**

| Category | Count | Items |
| --- | --- | --- |
| (a) plugin on today's API, or a channel that already exists | 9 | 2, 10, 11, 12, 16, 19, 38, 39, 40 |
| (b) plugin plus a small upstream extension point | 10 | 3, 4, 5, 6, 7, 8, 9, 13, 15, 18 |
| (c) stays in the standalone app | 16 | 1, 17, 21, 23, 26–37 |
| (c) general game fixes, offered upstream on their own | 5 | 14, 20, 22, 24, 25 |

That is 21 items in (c) altogether.

In (c), the line editor's headless map loading, the livery studio and the bus photos need
openOMSI's *library crates* but not the *running game*. In the add-on, they become a helper
program of Omsi-Hub's own (`omsihub-service`). It is built from openOMSI's crates at a pinned
tag, so it reads and writes the same file formats. See §3.

---

## 2. openOMSI's plugin system (our base and upstream main)

### Kinds of plugin

The game loads plugins from the `plugins` folder of every content root, except a LAN host's
sandbox (`omsi-app/src/plugins.rs`). In practice:

- `<content folder>/Plugins`, the content folder normally being the game's own folder;
- `<OMSI 2>/plugins`.

| Kind | Where it lives | How it runs |
| --- | --- | --- |
| OMSI 2 `.opl` + DLL | `plugins/*.opl` | OMSI's own `PluginStart`, `Access*` stdcalls; 32-bit DLLs run in `omsi-plugin-host`, through Wine elsewhere. openOMSI adds `AccessScriptTexture` and `AccessScriptTexturePointer`, and upstream main adds `AccessTicketRequest`. |
| Lua | `plugins/<name>.lua`, or `plugins/<name>/main.lua` with modules | Lua 5.4 (mlua, vendored), sandboxed: no `io`, no `os`, no C modules, no bytecode; 256 MB; a call over 50 ms stops the plugin. Reloaded when a `.lua` file changes. Every API function is `omsi.<name>`. |
| WebAssembly | `.oop` of kind `wasm` | wasmi interpreter, the same file on every system; one import, `openomsi.call(name, json)`; fuel limit; 256 MB. |
| Compiled `.oop` | `plugins/*.oop` | Lua or WASM built by `oopc` (openOMSI Development Tools): encrypted, optionally signed. The header carries `api_abi` and `permissions`. |

### The API surface

`crates/omsi-plugin/src/api`: one registry, the same for Lua and WASM, documented in
`docs/PLUGINS.md` and `docs/plugin-api.json`.

**Reading the game:**

- `info()`, `clock`, `position`, `speed`, `var` / `str` / `sys`, `others`;
- `bus.*`: doors, engine, gear, lights, passengers, tickets, destination, line;
- `map.*`, `traffic.*`, `people.*`;
- `duty.*`: `get`, `trips`, `stops`, `delay`, `next_stop`;
- `timetable.*`: `lines`, `stops`, `buses`;
- `game.*`: `version`, `api`, `has`, `platform`, `settings`, `stats`;
- `lan.*`.

**Changing the game** (each behind a permission):

- `set_var`, `press` / `trigger`;
- `bus.set_line`, `bus.set_destination`;
- `duty.start(line, tour, trip, stop)`, `duty.finish`, `duty.skip_*`;
- `weather`, `time`, `teleport`, `traffic` density;
- `command`, `game.action`.

**Events** (`omsi.on`):

- the plugin and the vehicle: `start`, `stop`, `frame`, `vehicle`, `key`;
- the duty: `duty`, `duty_start`, `duty_end`, `trip_start`, `trip_done` (with the driving,
  comfort and ticket ratings), `stop_arrive`, `stop_depart`, `stops_skipped`;
- driving: `crash`, `jolt`, `red_light`, `speed_limit`, `light_ahead`, `ai_collision`;
- the bus: `door` / `doors`, `passengers`, `ticket_sold`;
- time: `minute` / `hour` / `day`;
- the game: `pause` / `resume`, `menu_open` / `menu_close`;
- the panels: `ui_click`, `ui_change`, `ui_focus`;
- other plugins and the LAN: `message`, `lan_*`;
- other: `tile`, `weather`, `screenshot`, `controller_button`, and more.

**Interface:**

- `ui.set` panels: text, icon, row, bar, badge, button, checkbox, slider, input, tabs, chart,
  table, image;
- `ui.toast` cards, `ui.focus`, `message`, `game.notify`;
- a settings panel generated from `plugin.settings`.

**Data and communication:**

- `storage.*` (`storage.json`);
- `files.*`, inside the plugin's data folder only;
- `plugin.read`, from its own folder;
- `json.*`;
- `send(port, data)`: UDP to `127.0.0.1` only;
- plugin-to-plugin messages.

**Not there:**

- receiving from other programs (no UDP listen);
- opening a URL;
- any hook into the timetable's AI, the spawn of the player's bus, the navigator's drawing or
  the cab's rendered textures.

### Versioning

- `omsi_plugin::api::ABI = 1`, raised only when something is taken away or changes. A `.oop`
  whose `api_abi` is higher is refused.
- Every function and event carries `since`. `api::VERSION` is `"0.2.22"`, unchanged on upstream
  main even though releases are at 0.3.x, so `since` cannot be compared with release numbers.
- Feature detection at run time: `omsi.game.has("name")`, or, on any version, looking the
  function up in the `omsi` table.
- Lua plugins have no binary ABI; the same `main.lua` runs on every system.

### Switching a plugin on

Today a plugin is on when its file is in a `plugins` folder:

- `OMSI_NO_PLUGINS=1` switches all plugins off;
- the launcher's mod installer parks plugins it does not switch on in
  `<content>/Mods/plugins-not-enabled`;
- there was no per-plugin switch. Phase 1 adds one: PR 1 in §7.

### Upstream main compared with our base

Upstream main is 2,742 commits ahead and at version 0.3. Its plugin crate differs by
+594 / −31 lines: passenger tickets (the `passenger_ticket` event, `people.set_ticket`,
`AccessTicketRequest`) and an example. **ABI is still 1** and everything this add-on uses is
the same. The phase 1 upstream commits cherry-pick onto upstream main and pass its checks there
(§7).

---

## 3. The target architecture

```
┌──────────────────────────── Omsi-Hub (standalone app) ────────────────────────────┐
│ Tauri page (launcher/ui)        company core (omsi-launcher-core company/,         │
│ Settings ▸ Your openOMSI        RosterFlow, Play together), editors' data          │
│        │                                                                           │
│ omsihub-plugin: detect · compat · install · bridge     omsihub-service (offline:  │
│        │                                               line editor, livery studio, │
│        │   writes assignment.lua / notices.json        photos; openOMSI crates at  │
│        │   reads hello.json / reports.jsonl            a pinned tag)               │
└────────┼───────────────────────────────────────────────────────────────────────────┘
         │ files in <openOMSI content>/Plugins/omsihub/  (+ game.log, telemetry.json)
┌────────┼──────────────── openOMSI, unchanged, any version ≥ 0.2.0 ─────────────────┐
│ Plugins/omsihub/main.lua ── omsi.* plugin API ── the game                          │
│ (options ▸ Add-ons switch: download, verify, enable; PRs 1–3)                       │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

1. **Omsi-Hub becomes a standalone app.** It is the Tauri launcher plus the company core. It
   runs next to an unmodified openOMSI and does not ship a copy of the game.
2. **The player chooses the openOMSI folder.** This is done in Settings ▸ Your openOMSI, built
   in phase 1.
   - Omsi-Hub checks for the program in it:
     - `openomsi.exe` (Beta 65 on: `omsi-hub-game.exe` first, §11);
     - `openomsi`;
     - a bundle `*.app/Contents/MacOS/openomsi`, or the bundle itself.
   - It refuses an OMSI 2 folder.
   - It remembers the choice in `~/.openomsi/omsihub-addon.json`.
3. **Omsi-Hub detects the version without starting the game**, where it can:
   - the Windows ProductVersion resource;
   - the macOS `Info.plist`;
   - the release archive's folder name;
   - otherwise `openomsi --version`, once per program file.
   It then estimates compatibility (§5).
4. **Omsi-Hub installs or enables the plugin** in the game's content folder's `Plugins` (the
   same rule as `omsi_cfg::content_dir`):
   - `Plugins/omsihub/` holds `main.lua`, `hubjson.lua` and a version stamp;
   - it is updated in place;
   - switched off, it is parked in `Mods/plugins-not-enabled`;
   - once openOMSI has PR 1, its `disabled.txt` is honoured.
5. **Omsi-Hub launches openOMSI** with upstream's own command line: `--root --map --bus --time
   --line --tour --trip --weather --traffic --driver ...`.
6. **Omsi-Hub talks to the plugin through files** (`omsihub_plugin::bridge`):
   - **App to plugin:**
     - `assignment.lua` holds the duty, notices and the plugin's texts in the player's
       language. It is `require`d, so it works on every Lua plugin API, and a rewrite reloads
       the plugin while the game runs.
     - `data/notices.json` carries live notices.
   - **Plugin to app:**
     - `data/hello.json` is the handshake;
     - `data/reports.jsonl` holds `duty_start`, one `trip` per trip done (with its ratings and
       delay) and `duty_end`.
     - A game without file functions gets the same lines in `game.log`.
     - The optional live link is the assignment's `udp_port` with `omsi.send`.
7. **Offline work never needs the running game.** The editors, the livery studio and the bus
   photos move to `omsihub-service`, built from openOMSI's crates at a pinned tag. The game's
   file formats are OMSI's and stable, so a pinned reader is enough.

**The single upstream feature Luc asked for** is the options' **Add-ons** page. One switch per
catalogue entry: on, it downloads the add-on, verifies it and enables it. This is PRs 1–3,
already built as commits on this branch. The Omsi-Hub entry is PR 4 and is data only.

---

## 4. Mapping and the extension points to propose upstream

### Items in (a): today's API

| # | How |
| --- | --- |
| 2 | Omsi-Hub starts `openomsi` with upstream's own arguments. The fork-only ones wait for E6/E7. |
| 10 | Plugin: `trip_done(trip, how, driving, comfort, tickets)`, `stop_arrive` / `stop_depart` (each with its delay), `jolt`, `crash`, `red_light`, `duty.*`. The card is a `ui.set` panel. The record reaches the company as `reports.jsonl`. |
| 11 | Plugin: `red_light`, `speed_limit` and `speed()` for the camera rules; `jolt` and `crash` for comfort; the notice as `ui.toast`. Fixed cameras come from the app's own map data, given in the assignment. |
| 12 | Plugin: a `ui.set` panel anchored near the navigator, from `duty.get` / `duty.stops`. |
| 16 | Plugin: a panel with `input` fields and an accept button. The personnel data comes in the assignment. |
| 19 | Plugin: `bus.set_line` and `bus.set_destination` on `duty_start` / `trip_start`. |
| 38 | Unchanged: these are upstream's files. |
| 39 | The app reads `telemetry.json`. The plugin's `send` adds the duty events. |
| 40 | The player sets upstream's `OMSI_LAUNCHER` to Omsi-Hub, or Omsi-Hub starts the game. |

### Items in (b): the extension points

Each extension point would be its own small, generic pull request. The plugin feature-detects
it with `game.has` and degrades to today's behaviour where the game lacks it.

| Ext. | Proposed API (Lua `omsi.*`; WASM the same by name) | For | Without it |
| --- | --- | --- | --- |
| **E6** timetable provider | `timetable.keep_off(line, tour, off)`: a tour of the map's timetable is not placed today. `timetable.set_bus(line, tour, {vehicle, paint, number, plate, hof})`: the AI's bus for a tour. Event `ai_trip_done(line, tour, trip, delay, km, stops)`. The day change already comes as the `day` event. | #3 company live day | The map's own timetable runs; the company books only the player's trips. |
| **E7** duty plan | `duty.start_plan({{line, tour, first_trip, trips, from_stop}, ...})`: several legs, the trips reserved from the AI. `duty.start(line, tour, trip, stop, {until_stop = n})`: a relief end. `duty.start_free(line, route)`: a route followed, not judged. | #4 | The plugin starts the first leg with `duty.start`; the app tells the player the relief stop. |
| **E8** line vehicles | `timetable.set_line_vehicles(line, {vehicle, ...})`: the buses the AI uses for a line (the line editor's own lines). | #5 | The game picks buses as it does for any line. |
| **E9** spawn options and display font | Event `vehicle_init(name)` before the bus's `{init}`, in which `set_var` takes effect as `--setvar` does upstream. `bus.set_display_font({font, size, ...})`: a font for the destination display's text textures (with openOMSI taking `dotfont`). | #6, #7 | Options set after `{init}` with `set_var`; the bus's own font. |
| **E10** player depot files | A rule, not a function: `.hof` files the player made (for example in `<bus>/HOF/own/` or with `[player]` in the file) are never auto-matched by name. | #8 | The depot editor writes files with names that do not match by accident. |
| **E11** editor action | The map editor as a `[game]` action, so that `game.action("editor")` opens it. | #9 | The editor hub opens the game and the player presses its key. |
| **E12** navigator layer | `nav.markers({{x, y, icon, color, label}, ...})`, `nav.route_to(x, y)`, event `map_click(x, y, button)`, and the stop-sign style as a navigator setting. | #13, #15 | The pins and stop signs are not shown; the upstream navigator stays as it is. |
| **E13** cab device pictures | `bus.texture(index, width, height)`: the RGBA of one of the bus's rendered textures (the non-blocking `Readback` of `omsi-render`). `bus.click(mesh, u, v)`: a click on a cab mesh. | #18 | The companion shows the duty and the map, without the live IBIS and ticket machine. |
| (optional) **E14** listen | `net.listen(port)` and the event `udp(data)`, only on `127.0.0.1`. | Live app-to-plugin messages. | File polling every 5 s (built). |
| (optional) **E15** theme | `ui.theme({accent})`: the plugin panels' accent. | Omsi-Hub's colour on its panels. | Each panel's own `accent` key (built). |

### Items in (c)

- **#1, 21, 23, 33, 35, 36, 37** are Omsi-Hub's look, settings, texts, updates and releases.
  They stay in the app; the game's copies are reverted in phase 2.
- **#17, 26–32, 34** are the app and its helpers:
  - the companion's HTTP server moves to the app, fed by `telemetry.json` and the plugin;
  - the editors move to `omsihub-service`.
- **#14, 20, 22, 24, 25** are offered to openOMSI as ordinary bug-fix or feature pull requests.
  Examples: the minimap resize, Tobii head tracking, the LAN date, the duplicate-id fix. These
  have nothing to do with the add-on.

---

## 5. "Works on every openOMSI version"

There are three layers, each built in phase 1 (`crates/omsihub-plugin/src/compat.rs`).

### 1. Static estimate (before the first drive)

`compat::estimate(version)`:

- Every public openOMSI release (0.2.0 and later) has the plugin API this add-on is written
  against: Lua folder plugins, `duty.*`, `trip_done`, `ui.*`, `files.*`, ABI 1. The public
  history starts there.
- Below 0.2.0: **not supported**.
- Version not found: **checked at the first drive**.

### 2. Handshake (authoritative)

1. At start the plugin writes `hello.json`:
   - `game.version()`;
   - `game.api()` (the ABI);
   - for every function the add-on uses, whether the game has it. This is looked up in the
     `omsi` table, so it works even where `game.has` does not exist.
2. `compat::negotiate` then decides:
   - **unsupported** if the ABI is below 1 or the essential `duty.*` functions are missing;
   - **supported with limits** if the ABI is above 1 (the API may have changed: update
     Omsi-Hub) or an optional feature is missing;
   - **supported** otherwise.
3. A handshake from another version than the folder has now is ignored, because the game was
   updated since.

### 3. Graceful degradation per feature

The feature table is `compat::FEATURES`, and the Lua checks the same list (a test keeps them
equal).

| Feature | Needs | Without it |
| --- | --- | --- |
| duty (essential) | `duty.get`, `duty.trips` | — |
| reports | `files.append`, `files.write` | handshake and reports through `game.log` (`bridge::from_log`) |
| duty_start | `duty.start` | the player takes the line and tour in the game's menu |
| overlay | `ui.set`, `ui.toast`, `ui.remove` | notices as `omsi.message` lines |
| live_notices | `files.read`, `json.decode` | notices read at start (from `assignment.lua`) |
| live_link | `send` | results read when the drive ends |
| trip events | `trip_done` (with `game.has`) | trips detected from `duty.get().trip` changing |

### Rules for later changes

- **The bridge format is versioned** (`bridge`). Every field has `#[serde(default)]`, so old and
  new Omsi-Hub and plugin versions read each other's files.
- **Every (b) extension point** is detected with `game.has` and degrades as in the §4 table.
  This is how one Omsi-Hub works with openOMSI before and after each upstream PR lands.
- **If openOMSI raises the ABI**, Omsi-Hub keeps working ("with limits") with what the
  handshake still finds. A new Omsi-Hub release adapts the plugin, and the app updates the
  plugin in place on its next start.

---

## 6. Packaging and updates (Windows and macOS)

### What ships

Omsi-Hub ships **without the game**:

| Platform | Package |
| --- | --- |
| Windows | `Omsi-Hub-<v>-windows-x64.zip` or an installer: `Omsi-Hub.exe` (the Tauri app), `omsihub-service.exe` (the offline helper), the WebView2 bootstrapper. |
| macOS | `Omsi-Hub-<v>-macos-<arch>.dmg` with `Omsi-Hub.app`: the app and the helper in `Contents/MacOS`, signed and notarised. It is a separate bundle from `openOMSI.app`, so updating either never touches the other. |

**The plugin is compiled into the app** (`install::FILES` uses `include_str!`).

**A plugin zip is also published** as a release asset, `omsihub-plugin-<v>.zip` with its
SHA-256, for openOMSI's Add-ons catalogue (PR 4).

### Updates

| What | How |
| --- | --- |
| Omsi-Hub app | Omsi-Hub's own updater (today's `omsihub/update.rs` and `omsi-platform` updater code), pointed at app-only assets. |
| Plugin | The app checks the stamp on every start and updates the plugin in place (`install::status` reports `outdated`). A plugin the game downloaded from the catalogue is updated the same way. |
| openOMSI | Its own updater, independently. Omsi-Hub re-reads the version on start, estimates, and the next handshake confirms. |

### First run

**From Omsi-Hub:**

1. Choose your openOMSI folder.
2. Omsi-Hub shows the compatibility.
3. Switch the plugin on.
4. Drive.

**From openOMSI** (once PRs 1–4 land): Options ▸ Add-ons ▸ Omsi-Hub on. The game downloads the
plugin. Phase 2 adds a panel to the plugin that says where to get the Omsi-Hub app until it is
installed. A plugin cannot open a browser, so it shows the address. Today the plugin shows its
panel only once Omsi-Hub has written an assignment.

**The optional PR 5** lets the catalogue also name the app, so the game can fetch it as well.

### During the transition

The bundled build stays the default until the items §10 still lists as bundled have moved:
`omsihub-release.yml` keeps shipping the fork's game. The add-on mode is opt-in from Settings ▸
Your openOMSI ("Start this openOMSI for my drives"). Since phase 2 the plugin runs in both: the
launcher installs it into its own copy of the game too, so the features that moved to the
plugin left the game's code for both modes.

---

## 7. Upstream PR sequence

These are small, independent pull requests to `openOMSI-org/openOMSI`, each focused on one
change and readable line by line. **None contains Omsi-Hub code. None has been opened: Luc
decides when.**

PRs 1–3 are built here as separate commits that can be cherry-picked. Checked on a scratch
branch from `upstream/main` (`fe812289`):

- PRs 1 and 2 apply without conflicts, and `cargo test -p omsi-plugin --lib` passes (27
  tests).
- PR 3's code applies without conflicts and `cargo check -p omsi-app` passes. The only
  conflict is the fork's own `tauri-addon.yml`, whose four texts go into upstream's `app.yml`.

| PR | Title | Files and size | What it adds | Why openOMSI wants it on its own |
| --- | --- | --- | --- | --- |
| 1 | **Plugins: switch a plugin off without removing it** (commit `Plugins: switch a plugin off without removing it`) | `crates/omsi-plugin/src/switch.rs` (new, 90 lines, about 40 of them tests); `crates/omsi-plugin/src/lib.rs` (+18 / −6 in `Plugins::load`); `docs/PLUGINS.md` (+4) | `plugins/disabled.txt`: one plugin name per line (case-insensitive, `#` comments), covering `.lua`, folder, `.oop` and `.opl` plugins. `switch::set` edits the list and keeps the other lines. | Players switch a misbehaving plugin off without deleting it or its data. Any manager, menu or launcher gets one plain-text format to write. |
| 2 | **Plugins: add-ons from a catalogue, checked and unpacked** (commit `Plugins: add-ons from a catalogue, checked and unpacked`) | `crates/omsi-plugin/src/addons.rs` (new, 229 lines, about 90 of them tests); `crates/omsi-plugin/Cargo.toml` (+3: `sha2`, `zip`, both already workspace dependencies); `src/lib.rs` (+1); `Cargo.lock` (+2) | `parse_catalogue`: `[{id, name, description, homepage, version, url (https), sha256}]`, bad entries skipped with a log line. `install`: SHA-256 checked, paths kept inside `plugins/<id>/`, **no programs or libraries** (only Lua, `.oop`, data, pictures, sounds, fonts), unpacked beside the folder first, the plugin's `data/` kept. `enabled` and `set_enabled` use PR 1's list. | Safe, verifiable installation of sandboxed plugins. It is the base for any "get plugins" feature, independent of who lists them. |
| 3 | **Options: an Add-ons page to switch catalogue add-ons on** (commit `Options: an Add-ons page that downloads and switches add-ons`) | `crates/omsi-app/src/addons.rs` (new, about 110 lines); `src/game_lists/options.rs` (+7: one arm each in `toggle_now` and `toggle_set`); `src/game_lists/menu_pages.rs` (+5: the page, shown only when the catalogue lists something); `src/lib.rs` (+1); `assets/addons.json` (new: `[]`); 4 texts for `app.yml` | One switch per catalogue entry. **Switched on the first time:** downloaded in the background (`ureq`, 64 MB limit), checked and installed by PR 2; the row shows "Downloading..." or the error. **Off:** `disabled.txt`. Takes effect at the next start. | The main-menu switch Luc asked for, generic: the openOMSI organisation decides what the catalogue lists (its own example plugins, community plugins). The catalogue is empty in this PR, so nothing changes for players until something is listed. |
| 4 | **Add-ons: list Omsi-Hub** | `assets/addons.json` (+10: one entry) | The Omsi-Hub plugin's release zip, its SHA-256, homepage and description. | Data only, no code. Players can get the bus company add-on from the game. |
| 5 (optional) | **Add-ons: a companion program** | `crates/omsi-plugin/src/addons.rs` (+40: an optional `program` per platform with its own SHA-256); `crates/omsi-app/src/addons.rs` (+40: an "Open <name>" row that downloads, verifies and starts it, through the existing `launcher_link` code) | The add-on's desktop app, for example the Omsi-Hub app, fetched and opened from the game. | Add-ons that are more than a plugin (planners, editors) become one click. This is the step reviewers may question, as it runs downloaded programs; without it the plugin's panel shows the download address instead. |
| 6+ | **E6–E13**, one PR each, in the order of §8 | `crates/omsi-plugin/src/api/fns/*.rs` (a few `def!` entries), the matching `PluginIo` method in `io.rs`, its implementation in `omsi-app/src/plugin_io/*`, `docs/PLUGINS.md`. Typically +80 to +250 lines each. | For example E7's `duty.start_plan`, built on what `--duty-leg` already does in the fork's `omsi-sim`. | Each is a plugin capability any plugin can use: career mods, dispatch tools, timetable tools, accessibility. |

### Preparing each PR

- **The commit.**
  - PRs 1 and 2: `git cherry-pick` the commit onto a branch from `upstream/main` (checked
    clean).
  - PR 3: cherry-pick the commit and drop `crates/omsi-app/locales/tauri-addon.yml` from it.
    Its four game texts (`Add-ons` exists already) go into upstream's `app.yml`.
- **The tests.** Run `cargo test -p omsi-plugin`. For PR 3, also run `cargo check -p omsi-app`.
- **The description.** Name the feature and say why it helps players on its own (the last
  column of the table above). Do not mention Omsi-Hub until PR 4.

---

## 8. Phase 2: the migration order

Every step keeps every build working:

1. Add the plugin side first, behind its compat feature.
2. Switch the default once it is proven.
3. Delete the in-game hook in a separate, later commit.

The bundled build keeps working until the last step.

1. **Launch through the chosen folder.** No game code changes.
   - `omsi-launcher-core` `find_game` gets the chosen add-on folder as its first candidate, when
     the add-on mode is on (a setting; bundled stays the default).
   - The company's duty start calls `addon_assign`.
   - After the drive, `addon_reports` books the trips into the company, as
     `store::append_live` does today.
2. **Trip report card and driver's record → plugin** (item #10).
   - Add the card as a `ui.set` panel from `trip_done`, `stop_arrive` and `stop_depart`, with
     the grade computed by the app's `company::career::evaluate` and given back in the
     assignment.
   - Then remove `trip_report.rs`'s hooks and `omsi-career::write_trip`. That also drops
     `omsi-career`'s dependency on `omsi-launcher-core`.
3. **Drive watch → plugin** (#11): `red_light`, `speed_limit`, `jolt`. Then remove
   `drive_watch.rs` and its 3 hooks.
4. **Sign-on and duty board → plugin panels** (#12, #16). Then remove `nav_signon.rs` and the
   board from `navigator.rs`.
5. **IBIS auto-fill → plugin** (#19). Then remove the `ibis_auto` hooks.
6. **Companion → app** (#17), fed by `telemetry.json` and the plugin's live link. The cab device
   pictures (#18) stay fork-only until E13 is upstream.
7. **Editors, livery studio, photos → `omsihub-service`** (#26–30). This is a helper binary in
   the Omsi-Hub app built from openOMSI's crates at a pinned tag. Then remove
   `launcher/omsihub.rs` and the service's Omsi-Hub commands from `omsi-app`.
8. **Look, settings, updater, flags, branding** (#1, 21, 23, 33, 35): revert them in the game.
   Omsi-Hub's look lives in its app.
9. **Upstream PRs 1–4**: Luc opens them when he decides. Meanwhile the fork carries them as
   they are.
10. **Extension points, one PR each, then the plugin feature, then the hook removed**:
    - E7 duty plan (#4);
    - E9 spawn options and font (#6, #7);
    - E6 timetable provider (#3);
    - E8 line vehicles (#5);
    - E10 / E11 (#8, #9);
    - E12 navigator (#13, #15);
    - E13 device pictures (#18).
    Until each lands, the feature stays in the bundled build.
11. **Releases.** The release ships the app and the plugin only (§6). When no item is left in
    the bundled build, retire `omsihub-sync.yml` and the fork's copy of the game.

The general game fixes (#14, 20, 22, 24, 25) are offered upstream whenever convenient. They do
not block any step.

---

## 9. What phase 1 built

All of it is additive. No existing game code was moved or deleted.

### Commits

1. **`Plugins: switch a plugin off without removing it`** (upstream PR 1).
2. **`Plugins: add-ons from a catalogue, checked and unpacked`** (upstream PR 2).
3. **`Omsi-Hub add-on: the in-game plugin on openOMSI's plugin API`**: `crates/omsihub-plugin`.
   - `lua/main.lua` and `lua/hubjson.lua`, the plugin itself:
     - reads the assigned duty;
     - offers it on a panel (F10 for the mouse) or takes it by itself (`auto_start`);
     - reports `duty_start`, each `trip` with its ratings and delay, and `duty_end`;
     - shows the company's notices as cards, and new ones while driving.
   - `bridge`: the files, with `ReportReader` reading reports incrementally and the `game.log`
     fallback.
   - `compat`: estimate, handshake, the feature table.
   - `detect`: the folder, the program, the version (PE resource, `Info.plist`, folder name,
     `--version` output) and the plugins folder.
   - `install`: install, update in place, park, remove, `disabled.txt`.
   - `version`: `MAJOR.MINOR.COMMIT[-prN]`.
   - Tests: the Lua runs in `omsi-plugin`'s real runtime with a fake bus and duty. A second
     test runs it with an "older openOMSI" (no `files`, `ui`, `game` or `json`) to check the
     fallbacks.
4. **`Launcher: choose your openOMSI folder`**:
   - Settings ▸ Your openOMSI (`launcher/ui/js/omsihub/addon.js`):
     - pick a folder;
     - validate it;
     - show the version and where it was read from, the compatibility (supported, with limits,
       not supported, checked at the first drive), the notes and each feature;
     - switch the plugin on or off, update it;
     - forget the folder.
   - The `addon_*` commands (`omsi-launcher-api/src/omsihub/addon.rs`): `addon_state`,
     `addon_set_folder`, `addon_clear`, `addon_enable`, `addon_assign`, `addon_reports`.
   - Translations in `crates/omsi-app/locales/tauri-addon.yml` (de, fr, nl, pl, ru, uk).
   - The bundled mode is unchanged and stays the default.
5. **`Options: an Add-ons page that downloads and switches add-ons`** (upstream PR 3).
6. **This document.**

### Shared files touched (each change is minimal)

- `Cargo.toml`: workspace member `crates/omsihub-plugin`.
- `Cargo.lock`.
- `crates/omsi-plugin/src/lib.rs`, `crates/omsi-plugin/Cargo.toml`, `docs/PLUGINS.md`: PRs 1
  and 2.
- `crates/omsi-launcher-api/Cargo.toml` (+2) and `crates/omsi-launcher-api/src/omsihub/mod.rs`
  (+4: the module, the `addon_` route, the YAML in `TABLES`).
- `launcher/ui/js/omsihub/settings-extra.js` (+11: the tab) and `launcher/ui/omsihub.css`
  (+5).
- `crates/omsi-app/src/lib.rs` (+1), `crates/omsi-app/src/game_lists/options.rs` (+7),
  `crates/omsi-app/src/game_lists/menu_pages.rs` (+5): PR 3.
- New: `assets/addons.json`, `crates/omsi-app/locales/tauri-addon.yml`.

---

## 10. What phase 2 did

Branch `r62-addon2`, based on `r62-base`. Every commit keeps every build working; the full
tests (`cargo test --release -p omsi-launcher-core -p omsi-launcher-api -p omsi-plugin -p
omsihub-plugin`, `cargo test -p omsi-app`) passed at each step. No pull request was opened.

### How Omsi-Hub reaches the game now

- **The plugin is in every game Omsi-Hub starts.** In the add-on mode (Settings ▸ Your
  openOMSI ▸ "Start this openOMSI for my drives") that is the chosen openOMSI; otherwise it is
  Omsi-Hub's own copy, into which the launcher puts the plugin (and keeps it up to date) by
  itself. So the features that moved to the plugin moved for both, and the game's own hooks
  could go.
- **Every start gives the plugin an assignment** (`addon::assign_duty`): the company's duty or
  the player's own drive, the driver, the map and bus, the driver's pass when signing on is
  asked, the duty board and IBIS settings, the display font, the editor hub's "open the map
  editor". Its texts are written beside it in the player's language (`texts.lua`). An
  assignment older than half a day is not taken (a game started on its own later).
- **What the plugin reports is booked by the page's poll** (`addon::book`): each trip's record
  (Omsi-Hub's `TripRun`) goes into the driver's record (`trips/<driver>.jsonl`), which the
  company's day takes in as before.
- **A plain openOMSI gets only upstream's arguments:** `duty_args` leaves out the launcher's
  own (`OWN_ARGS`: `--company`, `--duty-leg`, `--free-line`) for a game that is not an
  Omsi-Hub build.

### The plugin (crates/omsihub-plugin/lua)

| File | What it does | On openOMSI's API |
| --- | --- | --- |
| `main.lua` | assignment, handshake, reports, notices, the company's panel | `duty.*`, `files.*`, `ui.*`, events |
| `trip.lua` | each trip followed stop by stop into the driver's record | `stop_arrive`/`stop_depart`, `duty.trip_stops`, `jolt`, `crash`, `passenger_board`, `bus.odometer` |
| `career.lua` | the trip judged and the fines, as the company does (a test runs it against `company::career`) | - |
| `card.lua` | the end-of-trip card | `ui.set` (tabs, table, bar) |
| `watch.lua` | red lights, speed cameras, comfort | `red_light`, `light_ahead`, `map.lane`, `speed` |
| `board.lua` | the duty board | `duty.get`, `duty.stops`, `duty.trips`, `info().clock` |
| `signon.lua` | personnel number and code, the duty order | `ui.set` inputs |

Every feature is detected; a game without it keeps working with less, and the compatibility
page lists it (`compat::FEATURES`: now also `ibis`, `display_font`, `editor`).

### Status of every inventory item (§1)

| # | Item | Status |
| --- | --- | --- |
| 1 | Branding | **Reverted in the game:** window title, title bar and the executable's ProductName are openOMSI's (`1d23db4b`). Omsi-Hub's name stays in its app. |
| 2 | Launch of a duty | **Moved:** the chosen folder in the add-on mode; upstream's arguments only for a plain openOMSI (`414c9233`). |
| 3 | `--company`, the company live day | **Still bundled.** It needs E6 (a timetable provider: keep a tour off the map, give a tour its bus, report the AI's trips), which reaches into `Schedule::choose` and the AI's dispatch - a deeper change than one extension point commit could carry safely untested in a running game. In the add-on mode the map's timetable runs and the company books only the player's trips. |
| 4 | Composed duties, relief starts, free drive along a line | **Still bundled.** The game picks the entry point from the legs before the world opens (`place_on_duty`), before any plugin runs; E7 alone (a plan started by a plugin) would start a relief duty from the wrong place. Needs E7 plus a "where to start" hint in the launch. |
| 5 | Own lines (`oo_*`, `oo_vehicles.json`) | **Still bundled** (E8 not built). The line editor's files work in any openOMSI as TTData; only the buses chosen for the line need E8. |
| 6 | Bus options | **Works with upstream:** `--setvar` is upstream's argument. |
| 7 | Display font | **Moved:** extension point `bus.set_display_font` (`e01f710d`), the plugin puts it on (`ba603547`), `--display-font` gone (`5cb4d96d`). The font support itself (`omsi-content` `dotfont`, `omsi-sim` `texttex`) is the fork's game code the upstream PR has to bring along. |
| 8 | Player's own depot files | **Still bundled** (E10 is a rule of the game, `omsi-vehicle` `depot_dir`/`is_players`): offered upstream as a game fix. |
| 9 | `--editor` | **Moved:** `game.action("object_editor")` (`84ddf492`), the plugin opens it (`ba603547`), `--editor` gone (`5cb4d96d`). |
| 10 | Trip report card and driver's record | **Moved** (`e2872daa`, `7992ba38`); `omsi-career::write_trip` and its dependency on `omsi-launcher-core` gone (`fbbd18c6`). |
| 11 | Drive watch | **Moved** (`43ad9c08`, `33d6e1b5`). The plugin's cameras sit by the lane's index (`map.lane`) rather than its map identity, so they are not in the same places as the game's were. |
| 12 | Navigator duty board | **Moved** (`233f454e`, `514a8fee`): the plugin's board panel; the small navigator has upstream's schedule again, the city map's duty sheet is gone. |
| 13 | Navigator restyle, bus stop signs | **Still bundled** (E12 not built). |
| 14 | Navigator size, place, handle | **Still bundled**, offered upstream as a game fix. |
| 15 | Navigator pins | **Still bundled** (E12 not built). |
| 16 | Signing on | **Moved** (`233f454e`, `514a8fee`): the plugin's company panel; the game never asks any more. `nav_signon.rs` stays as the city map's page of the companion (phones and tablets, the duty menu, the break) until #17 moves. |
| 17 | Companion server and pages | **Still bundled.** Its pages show the bus's devices (#18) and its duty menu works the game's own duty; split from the game they need E13 and a live link the plugin API does not have yet (E14). The companion's page in the navigator stays with it. |
| 18 | Companion's cab devices | **Still bundled** (E13 not built). |
| 19 | IBIS filled in automatically | **Moved:** extension point `duty.set_auto_ibis` / `duty.type_ibis` (`69e67d6a`, called E16 here), the plugin uses it (`1d562025`), the game's switch gone (`95cd2225`). The player's `ibis_auto` stays as the API's state: the upstream PR adds that field and its check in `set_duty_destination`. |
| 20 | Head tracking, force feedback | **Still in the game**, offered upstream as a game fix. |
| 21 | Accent colour, menu restyle, loading screen | **Still bundled.** `omsi-ui`'s accent and kit are what the fork's navigator, pins and companion page draw with; reverting them comes with E12 and #17. |
| 22 | Translated service messages | **Still in the game**, offered upstream as a game fix. |
| 23 | Omsi-Hub keys in `settings.rs` | **Partly reverted:** `nav_signon`, `nav_board`, `ibis_auto` are gone from the game (Omsi-Hub keeps them and gives them to the plugin). `accent`, `stop_style` (#13, #21), `nav_scale`/`nav_rect` (#14), head tracking (#20) and the companion's (#17) stay with their features. |
| 24 | LAN date and host's duty | **Still in the game**, offered upstream as a game fix. |
| 25 | Duplicate object ids | **Still in the game**, offered upstream as a game fix. |
| 26 | Host bridge for the editors (`launcher/omsihub.rs`) | **Still bundled.** `omsihub-service` needs the showroom (`launcher/showroom.rs`), the navigator's road builder (`navigator/roads.rs`), the weather setup and the spawn helpers, which live in the game program, not in a library crate. Moving them to library crates is an upstream refactor of its own. |
| 27 | The launcher service's Omsi-Hub commands | **Still bundled**, as #26. |
| 28 | Showroom and studio rendering | **Still bundled**, as #26. |
| 29 | Offline file writers | Library crates; stay where they are (Omsi-Hub uses them). |
| 30 | `omsihub-livery` | Omsi-Hub's crate; only the game's studio (#27) uses it from the game side. |
| 31 | Company, RosterFlow, Play together | Omsi-Hub's code in the launcher crates; untouched (other builders work there). |
| 32 | The Tauri launcher's page | Omsi-Hub's; untouched apart from the add-on page. |
| 33 | Updater pointed at the fork | **Still bundled:** the fork's own game updates from the fork's releases until the bundled build is retired (§8 step 11). |
| 34 | `omsi-net` `http`, `Tunnel::start_with` | Used by the companion (#17) and Play together; stay. |
| 35 | `OMSI_*` flags | **Still bundled** with the features they switch. |
| 36 | Locales | **Moved in part:** 60 of the fork's 80 tables are the launcher page's only and are in `crates/omsihub-locales` now (`3101232e`); the game is built without them. The 20 left are used by features still in the game (the companion, the editors' studio, the navigator), and `app.yml` still holds Omsi-Hub texts the game shows. |
| 37 | Release workflows | Untouched (the bundled build still ships). |
| 38 | Game status for the launcher | Upstream's files; unchanged. |
| 39 | Live position and duty | Upstream's `telemetry.json`; unchanged. |
| 40 | Opening Omsi-Hub from a bare start | Unchanged (upstream's `OMSI_LAUNCHER`). |

### The extension points (the future upstream pull requests)

Each is one commit, generic (no Omsi-Hub in it), with the plugin API's tests and the
regenerated docs:

| Commit | API | Plan's name | Note for the upstream PR |
| --- | --- | --- | --- |
| `69e67d6a` | `duty.set_auto_ibis(on)`, `duty.type_ibis()` | E16 (new) | brings `Player::ibis_auto` and its check in `set_duty_destination` (3 lines) |
| `84ddf492` | `game.action("object_editor")` | E11 | applies as it is |
| `e01f710d` | `bus.set_display_font(font)` | E9 (the display font part) | needs the fork's display font support (`omsi-content` `dotfont`, `omsi-sim` `texttex::apply_display_font`) |
| (phase 1) | `Plugins: switch a plugin off without removing it` | PR 1 | |
| (phase 1) | `Plugins: add-ons from a catalogue, checked and unpacked` | PR 2 | |
| (phase 1) | `Options: an Add-ons page that downloads and switches add-ons` | PR 3 | |

Not built: E6 (timetable provider), E7 (duty plan), E8 (line vehicles), E10 (depot rule), E12
(navigator layer), E13 (cab device pictures), and the bus-option event of E9 (`--setvar` is
upstream's, so nothing waits on it).

### The direction of the dependencies

`crates/omsihub-plugin/tests/deps.rs` reads every crate's `Cargo.toml` and fails when a crate
of the game (`omsi-*` outside the launcher crates) depends on an Omsi-Hub crate (`omsihub-*`)
or on the launcher crates beyond the edges upstream has itself (`omsi-app` →
`omsi-launcher-core/-api/-model`, `omsi-platform` → `omsi-launcher-core`), and when an
Omsi-Hub crate depends on the game program. What is left is listed with its reason
(`STILL_BUNDLED`): `omsi-app` → `omsihub-game` (the navigator's palette and duty state for the
companion's page, the stop signs, the loading screen, the launcher service's editors). The
dependency `omsi-career` → `omsi-launcher-core` is gone.

The launcher crates (`omsi-launcher-core`, `-api`, `-model`) are openOMSI's own launcher, which
upstream's game links too; Omsi-Hub's code inside them (`company/`, `omsihub/`) is the fork's
and has to move to Omsi-Hub crates of its own before Omsi-Hub can leave (below).

### Touches in the other builders' areas

Company's and Omsi-Hub's Play together, the RosterFlow relay, `plandays.rs` and RosterFlow's
copy UI were left alone, with two exceptions forced by the move of the locales (step 11):

- Their translation tables (`company-together*.yml`, `rosterflow*.yml`, `tauri-rosterflow.yml`)
  moved unchanged from `crates/omsi-app/locales` to `crates/omsihub-locales/locales`.
- `crates/omsi-launcher-api/src/omsihub/together.rs`: one line in a test's failure message
  names the table's new path.

### What is left before Omsi-Hub moves to its own repository

1. **E6, E7, E8, E12, E13** built and in openOMSI, then #3, #4, #5, #13, #15 and #18 to the
   plugin and their hooks out of the game.
2. **The companion (#17) into the Omsi-Hub app**, fed by `telemetry.json` and a live link from
   the plugin (E14), with E13 for the cab's devices; then `companion/` and `nav_signon.rs` out
   of the game.
3. **`omsihub-service` (#26-28):** upstream moves the showroom, the road builder and the spawn
   helpers into library crates (or Omsi-Hub keeps its own copies); then `launcher/omsihub.rs`,
   the service's Omsi-Hub commands and the `omsi-app` → `omsihub-game` dependency go.
4. **The look (#21) and the settings left (#23)** reverted once nothing of Omsi-Hub draws in the
   game; the Omsi-Hub texts in `app.yml` and the 20 tables left move to `omsihub-locales`.
5. **Omsi-Hub's code out of the launcher crates:** `omsi-launcher-core`'s `company/` (and the
   other Omsi-Hub modules there) and `omsi-launcher-api`'s `src/omsihub/` into Omsi-Hub crates
   that depend on openOMSI's library crates and on the launcher crates' public API only.
6. **Releases (§8 step 11):** the app and the plugin only; `omsihub-sync.yml` and the fork's
   copy of the game retired.

## 11. Fork differences

Deliberate differences from upstream openOMSI that stay until Omsi-Hub moves to its own
repository. Each is marked "Omsi-Hub (fork-local)" in the code.

- **The program names and the branding (Beta 65, docs/omsihub/RENAME.md).** Luc reversed step 8
  of §8 for this distribution: everything a player sees is called Omsi-Hub.
  - The Cargo bins are `omsi-hub-game` (`crates/omsi-app`, upstream's `openomsi`) and
    `omsi-hub-cli` (`crates/omsi-launcher-core`, upstream's `openomsi-launcher`). The Tauri
    app's bin is `Omsi-Hub` (upstream's `openomsi-launcher-app`).
  - `crates/omsi-cfg/src/product.rs` holds the names, the old names and the display name. A
    merge of upstream meets the rename there and in the few places that use it.
  - Everything that looks for a program falls back to the old name, so a folder of openOMSI's
    own still works as the chosen game (§3, step 2).
