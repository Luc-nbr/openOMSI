# r67-pause: status

The cloud session (branch `r67-pause-cloud`) took over the local session's snapshot (`r67-pause`, c7ef034), merged `r67-base` into it and finished the brief. It builds (`cargo build --release -p omsi-app`) and the tests pass (`cargo test --release -p omsihub-game pause_menu`, `cargo test --release -p omsi-app --lib`).

## Where it is

- `crates/omsihub-game/src/pause_menu.rs`: Omsi-Hub's pause menu, drawn in place of openOMSI's rail. It has:
  - the glass panel with the wordmark, Resume and the groups;
  - the foot with the driver and the way out;
  - the trip card and the key-hint bar;
  - the line order (`arrange`) and the keys (`keys`);
  - the gamepad (`PadKeys`).
- `crates/omsi-app/src/shell/hub.rs`: the game's side. It gathers:
  - the keys as the player has them;
  - the duty and the line's colour from the line editor's registry;
  - the stage (a triple screen's middle one);
  - the gamepad.
  An open list is drawn by openOMSI's `pause::list_page` in a sheet beside the panel.
- Picture: `cargo test --release -p omsihub-game --lib pause_menu::tests::preview_pictures -- --ignored` writes `pause-menu.png`, `pause-menu-lan.png` and `pause-menu-ultrawide.png` next to `OMSI_UI_PREVIEW` (or into `target/`). It is CPU-rasterised and needs no graphics card.

## Left open

- No blur behind the panel: only a veil, darker towards the panel.
- "End the session" has no confirmation today, and none was added.
- The card's sub-line is "Tour N · trip k of n": the timetable has no duty number.
- If a player puts `open_menu` on the pad's B button, B can close the menu and open it again in the same frame (by default Start is `open_menu`).
