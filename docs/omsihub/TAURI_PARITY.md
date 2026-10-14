# The desktop launcher and the game-drawn one it replaced

Trello "Tauri 6/6": every page, step and dialog of the launcher Omsi-Hub's game drew itself
(`crates/omsi-launcher-ui`, "Rust" below) and how far the desktop launcher (`launcher/`,
"Tauri" below) has it.

**Since Beta 52 (openOMSI 0.2.48) the Rust launcher is gone** - as openOMSI's own game-drawn
launcher is - and the desktop launcher is the only one: a plain start opens it, and where it is
missing or its window cannot open (no WebView2) the game says so and offers Microsoft's
installer (upstream's `launcher_link`). There is no `launcher_ui` setting, no `openomsi
--launcher` and no `OMSI_CLASSIC_LAUNCHER` any more. The "Rust" column below is history: what
the desktop launcher was checked against (the files are in git, `git show
399e6ef7:crates/omsi-launcher-ui/src/<file>`, Beta 51; many comments of the page and of
`omsi-launcher-api`'s `omsihub` name them as where a page came from). What the game program
still does for the desktop launcher - the line editor kept headless, the livery studio drawn as
pictures, a text on a bus's display, the bus photos - is the crate `crates/omsihub-game`, run by
the launcher service (`openomsi --launcher-service`).

## How it was checked

* **Pictures.** The same page of the same sandbox home, Rust left and Tauri right
  (`OMSI_LAUNCHER_PAGE`; `OMSI_LAUNCHER_INPUT` for the clicks the Rust launcher needs, a script
  over the webview's remote debugging for the Tauri page's): every page below dark at 1440 x 820,
  the frame, the settings, the editors and a page of each part of the company again light, dark
  at 1920 x 1080 and dark with "Launcher size" at 125 %.
* **The self-check.** `OPENOMSI_LAUNCHER_SELFTEST=full` of every phase together, in homes with
  a company of each mode, dark and light: no failure, no script error.
* **End to end.** The hand-overs, RosterFlow's Timeplan and Roster, the planning's sheet of a
  line's trips, the game controllers' set-up and Save and the update check were driven through
  the page's commands.
* **Old saves.** Two companies saved by older Betas (still with `difficulty` in place of
  `game_mode`) open, their clock runs (playing, +1 h, to tomorrow - after the decisions it
  waits for are answered, as in the Rust launcher), and they are saved and read again.

Status: **same** - the pictures agree (but for a pixel or two and the browser's type);
**upstream's** - openOMSI's own page of its desktop launcher in our look, with every setting
and action the Rust page has (laid out as upstream lays it out); **fixed** - a difference found
in phase 6 and made the same; **open** - not there, and why.

## The frame

| Rust | Tauri | Status |
| --- | --- | --- |
| Top bar: the brand, the steps as a route, the icons, the flags, the language (`flow.rs`) | `bar.js` | same |
| The sheet, its glass and the accent's glow round the start's pane (`flow::glass_pane`) | `omsihub.css` | fixed: the dark look's glow and its falloff |
| Edges, hairlines, a switch's track, a slider's rail: white blended in linear light | `omsihub.css` | fixed: as strong as the Rust launcher draws them, in both looks |
| An icon the atlas lacks: its room left empty | `bits.js` | fixed (it was the info sign) |
| Light and dark, the switch's circle (`theme_switch.rs`) | `bar.js` | same |
| Accent colour: the presets and a picker for any other (`accent_pick.rs`) | the bar's palette (`bar.js`); the Settings page's choice | same under the bar; the Settings page lists the presets only |
| "Launcher size" (`launcher_scale`), Ctrl + wheel | `hub.js` `scale()` | same |
| The welcome: language, interface, animations, game folder, driver, ready (`welcome.rs`) | `welcome.js` | same; since Beta 52 without the interface's step (new or classic launcher): there is one launcher |
| The tour, 22 stops (`tour.rs`) | `tour.js` | same |
| The bus company's tour, 28 stops | `tour.js` `COMPANY_STOPS` | same (phase 6) |
| The update: Check now, the dialog, installed and started again, "Updated to Beta N" (`update.rs`) | `update.js`, `omsihub/update.rs` | same (phase 6); upstream's own updates page (`updates.js`) stays off in our look: ours reads this fork's releases |
| The opening: the logo drawn (`intro.rs`) | `intro.js` (the logo's parts: `logo.js`) | same since Beta 53 (it went with the game-drawn launcher in Beta 52; the logo reads Omsi-Hub) |
| Bus across the screen between pages (`transition.rs`, `page_bus`, off by default) | - | open: not ported - off by default, and only motion |
| The phone and tablet launcher (`mobile.rs`, `phone.rs`) | upstream's `js/mobile` | upstream's: phones keep it |

## The drive

| Rust | Tauri | Status |
| --- | --- | --- |
| Mode: Work shift, Tour, Free drive, Bus company; the record and the ways (`flow.rs`) | `home.js` | same |
| Profile step | `steps/profile.js` | same |
| Map: the tiles and the list (`mapchoice.rs`) | `steps/map.js` | same |
| Start point (free drive, `freedrive.rs`) | `steps/start.js` | same |
| Day & weather, season (`daytime.rs`, `season.rs`) | `steps/day.js`, `steps/day-more.js` | same |
| Duty: shift length, the list, the route on the map, the shift sheet (`shiftsheet.rs`, `nav_duty.rs`) | `steps/duty.js`, `steps/duty-more.js`, `steps/mapbehind.js` | same; the map's scale bar in phase 6 |
| Bus: the classes, the tiles, the showroom, its options, display fonts, photos, depot files (`busclass.rs`, `buspick.rs`, `showroom.rs`, `busoptions.rs`, `displayfont.rs`, `busphoto.rs`, `hof.rs`) | `steps/bus.js`, `steps/bus-more.js`, `steps/busoptions.js`, `steps/displayfont.js`, `editors/hof.js`, `photos.js` | same; a bus without a photo is photographed by the game program (phase 6) |
| Own lines on the bus step (`ownlines.rs`) | `steps/more.js` | same |

## openOMSI's own pages

| Rust | Tauri | Status |
| --- | --- | --- |
| Settings: Graphics, Driving, Camera, Sound, Gameplay, General (`pages/settings.rs`) | upstream's `pages/settings.js` with `settings-extra.js` | (Beta 52: no "Launcher" row; speeds in upstream's Units, Omsi-Hub's `speed_unit=mph` taken over as British; upstream's drive layout and launcher size rows left out in our look) fixed: rows 40 high with the field 55 % wide, a switch's name the whole width, a slider's name, track and value on one line, a lone button the row's width, the head tracking's own rows under its switch, the General tab's order, Reset navigator; kept beside them: a Dark mode switch (the Rust launcher has the bar's moon only) and upstream's support package (the Rust launcher has it on Setup) |
| Controls: the keyboard (`pages/controls.rs`, `keybind_picker.rs`) | upstream's `pages/controls.js` | upstream's |
| Controls: the game controllers - devices, axes, buttons, the set-up, Save | upstream's `pages/controllers.js` (0.2.47) | upstream's: since Beta 52 openOMSI's own page (devices, axes, buttons, the step-by-step set-up, the force feedback's test) in place of ours (`pads.js`, `hub_service::pads`, gone) |
| Multiplayer, host (`multiplayer.rs`, `host.rs`) | upstream's `pages/multiplayer.js` | upstream's |
| Service record (`pages/profile.rs`) | upstream's `pages/profile.js` | upstream's |
| Sessions (`pages/sessions.rs`) | upstream's `pages/games.js` | upstream's |
| Mods (`pages/mods.rs`) | upstream's `pages/mods.js` | upstream's |
| Tutorials | upstream's `pages/tutorials.js` | upstream's |
| Setup, the support package (`pages/setup.rs`) | upstream's `pages/setup.js` | upstream's |

## The editors

| Rust | Tauri | Status |
| --- | --- | --- |
| The editor's page (`editor_hub.rs`) | `editors/hub.js` | same |
| Line editor: the map, stops, displays, timetable, kind, the table (`lineeditor.rs`) | `editors/lines/` | fixed: a line without a depot group gets the map's first, as the Rust panel sets it |
| Depot editor (`depoteditor.rs`) | `editors/depots.js` | same |
| Livery studio (`livery/`) | `editors/livery.js` | same |
| Timetable (`timetable.rs`) | `editors/timetable.js` | same |
| Map objects: the game's object editor | `editors/hub.js` | same |

## The bus company

| Rust | Tauri | Status |
| --- | --- | --- |
| How do you want to play, the companies, the founding wizard (`chooser.rs`, `wizard.rs`) | `company/chooser.js` | same; the wizard's logo in phase 6 |
| The bar: the mark, the mode, the status, the clock, the tools (`mod.rs` `status`, `clock::head`) | `company/company.js` | fixed: a narrow bar (a small window, a large "Launcher size") leaves out the day and time, then the status's figures, instead of pushing Back off the window |
| Start screen (`home.rs`) | `company/home.js` | same |
| Overview | `company/overview.js` | fixed: the last card is not cut off on a low window |
| Fleet, the dealer and its sheets, advertising (`fleet.rs`, `dealer.rs`, `adverts.rs`) | `company/fleet.js`, `company/dealer.js` | same |
| Staff: employees, hiring, licences, work schedule, settings (`people/`) | `company/people.js` | same |
| Lines | `company/lines.js` | same |
| Finances, the bank, the fares (`money.rs`, `bank.rs`, `fares.rs`) | `company/money.js` | same |
| Planning; a line's trips and timetable (`planning.rs`, `planning/plan_edit.rs`) | `company/planning.js`, `company/planedit.js` | same; the plan-edit sheet in phase 6 |
| The duty launcher (`planning/duty_launch.rs`) | `company/launch.js` | fixed: the route as the Rust launcher draws it, grey roads on the night; the duty's whole route in phase 6 |
| Career, the workshop's job (`career.rs`, `repair_game.rs`) | `company/career.js`, `company/repair.js` | same; the job in phase 6 |
| Depot, Concessions, Map, Research, Workshop, Contracts | `company/*.js` | same |
| Inbox | `company/inbox.js` | fixed: the bars in the soft accent (`accent_2`) |
| Signing a contract (`signing.rs`) | `company/contracts.js` | same |
| The company's settings, the time, the day's report, a decision (`settings.rs`, `clock.rs`) | `company/dialogs.js` | same; the first day is the browser's date field |
| The company's tutorial (`tutorial.rs`) | `tour.js` | same |
| Casual: home, buses, shop, lines, new line, drivers, money, company, shifts (`casual/`) | `company/casual.js` | same |
| RosterFlow: vehicles, timeplan, roster, the help, print, preferences, the lessons (`planning/rf_*.rs`, `planning/timeplan.rs`) | `rosterflow/` | same |

## Nothing is handed over any more

Until Beta 51 a few things still went to the game-drawn launcher: `kit.js` `handOver` when the
editors' module was missing from the page, the welcome's classic interface, upstream's Setup
page's "The game's own launcher - Open it", and the game itself where the Tauri app was missing
or its window could not open. Since Beta 52 none of them is there: the editors are a plain
import of the company's kit, the welcome has no interface step, upstream took its Setup row out,
and the game gives upstream's message (and WebView2 rescue) instead.

## Upstream's desktop launcher pages (openOMSI 0.2.47)

| Upstream | In our look | |
| --- | --- | --- |
| Model test (`pages/modeltest.js`) | on the start's links, in our page sheet | reachable |
| Inside views of the bus (`preview.js`, the showroom's `PreviewView`) | the bus step's preview (upstream's `BusPreview`): Outside, Passenger, Driver | reachable |
| The launcher rests while a game runs (`launcher_rest`) | our shell rests too | reachable |
| Game controllers with a set-up (`pages/controllers.js`) | Controls | reachable (ours gone) |
| A game that ended on an error says so (`crash.js`) | over every page of ours | reachable |
| The window grows with its size (`fit_zoom`) | left out: our look keeps the system's size, a larger window shows more (Luc) | not used |
| A classic Drive page in steps (`drive-classic.js`, `launcher_layout`) | left out: our drive's steps are the drive | not used |
| Updates and versions (`updates.js`, `versions.js`) | left out: our updater reads this fork's releases ("Beta N") | not used |

## Keeping upstream merges easy

Our code lives in files and folders of its own: `launcher/ui/js/omsihub/`, `launcher/ui/omsihub*.css`,
the `omsihub` module of `omsi-launcher-api`, `crates/omsihub-game`, the Omsi-Hub tables in
`crates/omsi-app/locales`, and this folder. Upstream's files carry only small hooks, each marked
`Omsi-Hub:` (one `mod` or `use` line, a hand-over at the end of a `match`, a field added to a
struct literal) - conflicts there are expected at every merge and are resolved by putting the
hook back on upstream's new version. Everything else stays upstream's: take theirs. The larger
exceptions are upstream's interface kit and theme (`omsi-ui`'s `kit.rs`, `theme.rs`, which carry
our look - the light look, the accent, glass), the navigator (the duty board, the pins, the
sign-on page) and the simulation's duty (composed duties, free drives along a line, the bus
company's tours): merge those hunk by hunk.

Two checks of upstream's catch what a merge forgets:

* `frame_stages::access_tests` - every field of the `App` belongs to a group, and a frame stage
  reaches only the groups it declares (functions are followed by name: a free function of ours
  called like one of upstream's, `frame(app)`, is mixed up with it). Our fields go into the
  group lists there with an `Omsi-Hub:` line; our phone companion runs in the integrations stage.
* `omsi-launcher-api`'s `every_text_of_our_page_is_translated` - upstream drops the languages of
  texts it no longer shows from `app.yml`; the ones our page still shows go into
  `tauri-omsihub.yml` (0.2.47: "Continue last game", "Update now" ...).

## Open

* The bus across the screen between pages (`page_bus`).
* macOS: this fork's release job builds and packs the desktop launcher for Windows only.
