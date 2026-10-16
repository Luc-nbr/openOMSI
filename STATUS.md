# r67-overlay-cloud: status

The in-drive overlays restored inside the Omsi-Hub plugin, with the bus company's parts.
Built on the local session's snapshot (`r67-overlay`, bd6f8893), which sits on `beta`
(a6eff38d), not on `r67-base`: merge it into r67-base/beta as usual. The pause menu
(`app_impl/menus.rs`) is untouched.

Done and tested (`cargo test --release -p omsihub-plugin -p omsi-plugin -p omsi-launcher-core
-p omsi-launcher-api`: 910 passed; `cargo test -p omsi-plugin-ui`; `cargo check -p omsi-app`):

- plugin UI version 3 (`crates/omsi-plugin`, `crates/omsi-plugin-ui`, `omsi-app`): the
  navigator's board (`anchor = "navigator"`, `ui.dock`), `box`, `stops`, bar `segments`, badge
  `weight`, panel `font`, table `aligns` and coloured cells, panel `grab` (clicks without F10),
  `game.settings().display_units`; docs/PLUGINS.md and docs/plugin-api.json updated;
- the duty board back in the navigator as Omsi-Hub's own was (the navigator's stop list makes
  way for it, so nothing is shown twice); the trip card and the watch's toasts in Omsi-Hub's
  look; the stray bar (the LAN chat's input box, opened by a click on the old top-left board)
  is gone with the board's move;
- the company's parts (coach, duty strip, service control messages, fare box with the load
  aboard, contract), switchable in the company panel (F10): docs/omsihub/OVERLAYS.md;
- every new text in the six locales (crates/omsi-app/locales/tauri-addon.yml).

To check on Windows: see the report in the session (what Luc should look at in the game).
