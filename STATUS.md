# r65-rename: status

The work stopped early because the cloud credit was running out. **Tests have not been run.** `cargo check --locked` of the Tauri shell (`launcher/src-tauri`) passes on Linux. It compiles the non-test code of `omsi-cfg`, `omsi-platform`, `omsi-launcher-core`, `omsi-launcher-api` and `omsihub-plugin`, and the shell's hand-over code. The game crate (`omsi-app`) and `omsi-guard` were not compiled, and none of the tests were.

## New names

| Program | Windows file now | Was | Cargo |
| --- | --- | --- | --- |
| The app players open (Tauri) | `Omsi-Hub.exe` | `openomsi-launcher-app.exe` | `launcher/src-tauri` `[[bin]] name = "Omsi-Hub"` (the package name stays the same, so `Cargo.lock` does not change) |
| The game | `omsi-hub-game.exe` | `openomsi.exe` | `omsi-app` `[[bin]] name = "omsi-hub-game"` (fork-local) |
| The terminal tool | `omsi-hub-cli.exe` | `openomsi-launcher.exe` | `omsi-launcher-core` `[[bin]] name = "omsi-hub-cli"` |

**The terminal tool.** Nothing starts it automatically. It offers `--cli <command>` for a terminal or a script, and otherwise passes its arguments on to the game. That is why it is named `-cli`, not `-service` (`--launcher-service` is a mode of the game itself).

**Why rename the Cargo bins instead of renaming during packaging.** Every build then produces the final names: dev builds, `build-windows.cmd`, CI and the local deploy. No packaging step can forget the rename. `build.rs` now uses `rustc-link-arg-bins`, so the link arguments do not depend on the bin name.

**The Tauri identifier stays `io.github.turbo-devv.openomsi.launcher`.** Changing it would move the WebView2 profile and localStorage, and players never see it.

## Done (committed, not compiled)

### The name table
- `crates/omsi-cfg/src/product.rs` (new): the names, the old names, `counterpart()` and `holds_game()`, with unit tests.

### Finding and starting the programs
Every lookup tries the new name first and then the old one:
- `omsi-launcher-core` `find_game`, refactored into `game_candidates`, with a test;
- `omsi-hub-cli` `main.rs`;
- `launcher_link.rs`, which finds the app as `Omsi-Hub` or `openomsi-launcher-app`;
- `recovery::staged_program`;
- `updater` `has_program`;
- `omsihub-plugin` `detect.rs` (`find_exe`, bundles, `version_from_output`), with a test;
- `omsi-cfg` `sibling_installs`;
- `hub_more.rs` and `omsi-launcher-model` `state.rs`;
- `versions.rs`, Tauri `update_relaunch` and `examples/update_e2e.rs`, which now use `updater::game_program`.

### Updater (`omsi-platform/src/updater.rs`)
- `asset_names()` gives `Omsi-Hub-<v>-<platform>` first and `openOMSI-<v>-<platform>` after. This is used by `release_of` and by `latest_from_site`.
- `program_after_update` and `game_program`: after an update, `relaunch` starts the program by its other name if the newly installed version uses that name.
- `install_place` maps a program that has been renamed to its new path.
- `install_archive` also sets aside the "other name" of each program that the archive brings. This means a downgrade to a version from before the rename works.
- Tests have been added for these changes.

### First-start migration (`omsi-platform/src/rename.rs`, new)
- The game (`main.rs`) and the app (Tauri `main.rs`) call it.
- It renames old program names to new ones in the folder of the running release install. It never touches a dev build, the staging folder or a macOS bundle.
- If a program exists under both names, the newer file wins.
- It fixes `.openomsi-files` to the new names.
- On Windows it retargets `.lnk` shortcuts on the Desktop, in the Start menu and in the taskbar/Start pins to `Omsi-Hub.exe`. This uses COM; the `windows` crate features `Win32_System_Com` and `Win32_UI_Shell` were added.
- Tests have been added.

### Starting the app directly
- When the player starts `Omsi-Hub.exe` directly (no `OPENOMSI_BIN`, no arguments, game beside it), it hands over to the game's plain start (`hand_over_to_game`). That way the start guard and the WebView2 checks still run; the game then opens the app.

### Game branding
- The window title, the FPS title, the pause menu and menu headers, the fatal dialog captions, the launcher-missing and WebView2 dialogs, and the companion PWA name all use `omsi_cfg::product::NAME`.
- The guard's heal/crash dialogs (`omsi-guard/src/dialog.rs`) say Omsi-Hub, and their download link now points to the fork's releases.
- The exe resource `ProductName`/`FileDescription` is now "Omsi-Hub".

### Tauri config
- `tauri.conf.json` now has `"mainBinaryName": "Omsi-Hub"`.

## Not done

### 1. Compile and run the tests
- `cargo test --release -p omsi-launcher-core -p omsi-launcher-api -p omsihub-plugin`, plus `-p omsi-cfg -p omsi-platform`.
- `cargo check --target x86_64-pc-windows-msvc -p omsi-platform`: the `rename::shortcuts` COM code has never been compiled. Check the `windows` 0.62 signatures of `GetPath`, `GetIconLocation`, `Save` and `CoTaskMemFree`.

### 2. Scripts
These still copy the old names and must be updated: `scripts/build-windows.cmd`, `build-windows-cross.sh`, `build-linux.sh`, `build-macos.sh` (bundle `Omsi-Hub.app`, `MacOS/omsi-hub-game`, `Omsi-Hub`, `omsi-hub-cli`, `Info.plist` `CFBundleExecutable` `omsi-hub-game`), `build-apple.sh`, `scripts/server/start.cmd` (should try `omsi-hub-game.exe`, then `openomsi.exe`) and `profile-th-wald.ps1`.

**These are now broken, because the cargo outputs have new names.**

### 3. The release workflow (`.github/workflows/omsihub-release.yml`)
The plan:

**Windows:**
- `dist/windows` with the new names becomes `Omsi-Hub-<v>-windows-x64.zip`.
- Also build a compatibility zip, `openOMSI-<v>-windows-x64.zip`. It holds the same files, with the three exes renamed back to `openomsi.exe`, `openomsi-launcher-app.exe` and `openomsi-launcher.exe`.
- Upload both zips.

**Mac:**
- `Omsi-Hub.app` goes into `Omsi-Hub-<v>-macos-<arch>.dmg` and `Omsi-Hub-<v>-macos-<arch>.zip`.
- The compatibility zip `openOMSI-<v>-macos-<arch>.zip` is a copy of the bundle with the binaries renamed to `openomsi`, `openomsi-launcher-app` and `openomsi-launcher`. Its `CFBundleExecutable` is `openomsi`, and it must be signed ad hoc again.
- `mac-publish` must check for and upload both zip names.
- The release notes still say "Start `openomsi.exe`".

**`release.yml` line 155** (upstream; also runs on pull requests) copies `dist/windows/openomsi.exe` and must be changed.

### 4. Docs
- `docs/omsihub/RENAME.md` (referenced from the code, not written yet): it should say how an old install moves over, as described below.
- `ADDON_PLAN.md` needs a "Fork differences" entry for the bin names and the branding.
- `README`, `BUILDING`, `USER_GUIDE`, `HANDOVER` and `launcher/README.md` still name the old files.

### 5. Texts in the app (launcher UI)
None of these have been changed yet, and they need all 7 locales:
- `index.html` `<title>`;
- `settings.js` "This is openOMSI {version}";
- the `versions.js` texts and `data.js` "openOMSI {version} is installed";
- the `setup.js` header;
- `displayfont.js` and `hof.js` "openOMSI's folder";
- `state.rs` `root_problem` texts.

Keep the add-on mode texts ("Your openOMSI" …) and "built on openOMSI".

### 6. Game-drawn texts in `crates/omsi-app/locales/app.yml` that say openOMSI
These have not been changed (there are many; it is upstream's table). Discord's `large_text` and the Discord app name are still openOMSI.

### 7. Bundle folder name on existing Macs
Existing Mac installs keep the folder name `openOMSI.app`; their Dock and menu bar already show Omsi-Hub. New installs from the dmg get `Omsi-Hub.app`.

## How an old install moves over (designed; workflow part not built)
1. The old app checks for an update. It knows only `openOMSI-<v>-windows-x64.zip`, so it downloads the compatibility zip, which uses the old names.
2. It self-tests `openomsi.exe --self-test` (the new game), swaps the files and relaunches `openomsi-launcher-app.exe` or `openomsi.exe` (the new code under the old name).
3. On that first start the new code (`rename::migrate_install`):
   - renames `openomsi.exe` to `omsi-hub-game.exe`, `openomsi-launcher-app.exe` to `Omsi-Hub.exe` and `openomsi-launcher.exe` to `omsi-hub-cli.exe`;
   - fixes `.openomsi-files`;
   - retargets the shortcuts.

   The `*.old-update` files go at the next healthy start, as before.
4. From then on the new updater takes `Omsi-Hub-<v>-*.zip`.

**Mac.** The old app installs the compatibility zip with the old names inside the bundle, and the new code runs with the fallbacks. The next update installs the proper bundle; `program_after_update` finds the renamed binaries.

## Local deploy into `dist\windows` (main session)
1. Remove `openomsi.exe`, `openomsi-launcher.exe` and `openomsi-launcher-app.exe`, plus their `.vorige` copies.
2. Copy in `omsi-hub-game.exe`, `omsi-hub-cli.exe` (from `target\...\release`) and `Omsi-Hub.exe` (from `launcher\src-tauri\target\...\release`).

Alternatively, leave the old files in place: the first start of the new app migrates them.
