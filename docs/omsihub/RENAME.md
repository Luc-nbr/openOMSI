# Omsi-Hub's program names (Beta 65)

Luc, Beta 65: everything a player sees is called Omsi-Hub. This reverses phase 2's step 8
(ADDON_PLAN.md §8), which gave the game openOMSI's branding back. The rename is fork-local;
`ADDON_PLAN.md` §11 lists it as a deliberate difference from upstream.

## The names

| Program | Windows | macOS (inside `Omsi-Hub.app/Contents/MacOS`) | Linux | Was |
| --- | --- | --- | --- | --- |
| The app the player opens (Tauri, `launcher/src-tauri`) | `Omsi-Hub.exe` | `Omsi-Hub` | `Omsi-Hub` | `openomsi-launcher-app` |
| The game (`crates/omsi-app`) | `omsi-hub-game.exe` | `omsi-hub-game` (the bundle's executable) | `omsi-hub-game` | `openomsi` |
| The terminal tool (`crates/omsi-launcher-core`) | `omsi-hub-cli.exe` | `omsi-hub-cli` | `omsi-hub-cli` | `openomsi-launcher` |

No two names differ only in case, so they can share a folder on Windows and on a
case-insensitive Mac disk.

`crates/omsi-cfg/src/product.rs` holds the names, the old names and the display name
(`NAME = "Omsi-Hub"`). The window title, the dialogs, the Discord presence, the updater's
messages and the exe resource (`ProductName`/`FileDescription`) all take the name from there.

**The terminal tool.** Nothing starts it by itself. `--cli <command>` runs one of the
launcher's commands for a terminal or a script, and any other arguments are passed on to the
game. That is why it is called `-cli`. `--launcher-service` is a mode of the game, not of this
tool.

**Why the Cargo `[[bin]]` names changed instead of renaming at packaging.** With the new bin
names, every build produces the final names: a dev build, `scripts/build-*.{cmd,sh}`, CI and
the local deploy into `dist\windows`. No packaging step can forget the rename. The crates keep
their package names, so `Cargo.lock` does not change. `crates/omsi-app/build.rs` uses
`rustc-link-arg-bins`, so its link arguments do not depend on the bin name.

**The Tauri identifier stays `io.github.turbo-devv.openomsi.launcher`.** The identifier names
the WebView2 profile folder (the page's `localStorage`: the welcome, the chosen tabs, the
company's local state) and the Tauri window state. A new identifier would start every player
with an empty profile, and the player never sees it. For the same reason the Mac bundle's
`CFBundleIdentifier` stays `io.github.turbo-devv.openomsi`, and the data folder stays
`~/.openomsi`.

## Finding and starting the programs

Everything that looks for a program tries the new name first and then the old one:

- the game: `omsi-launcher-core::find_game` (`game_candidates`), the plugin's
  `omsihub-plugin/src/detect.rs` (`find_exe`, the bundle), `omsi-cfg`'s `sibling_installs`,
  `updater::game_program`, `recovery::staged_program`, and the server's `start.cmd`/`start.sh`;
- the app: the game's `launcher_link::find_app_launcher` (a plain start of the game opens it).

A player who chose a folder of openOMSI's own (Settings, "Your openOMSI") keeps working,
because its `openomsi.exe` is found by the old name.

A plain start of `Omsi-Hub.exe` (no arguments, not started by the game) hands over to the
game's plain start (`hand_over_to_game` in `launcher/src-tauri/src/main.rs`). The start guard
(an update to finish, a version to go back to) and the WebView2 check then run as before, and
the game opens the app again.

## Releases

Each release carries two archives per platform:

- `Omsi-Hub-<v>-windows-x64.zip`, `Omsi-Hub-<v>-macos-<arch>.zip` (and the `.dmg`): the
  programs by their new names. Builds after the rename install these (`updater::asset_names`
  tries `Omsi-Hub-<v>-…` first).
- `openOMSI-<v>-windows-x64.zip`, `openOMSI-<v>-macos-<arch>.zip`: the same files with the
  three programs renamed back to `openomsi.exe`, `openomsi-launcher-app.exe` and
  `openomsi-launcher.exe` (on the Mac the binaries inside the bundle, its
  `CFBundleExecutable` set to `openomsi`, and the bundle signed ad hoc again). These are for
  the builds before the rename: they know only this name, check for `openomsi` in the
  archive, and start the program by its old name after the update.

`.github/workflows/omsihub-release.yml` builds and uploads both.

## How an old install moves over

### Windows

1. The old app looks for an update. It knows only `openOMSI-<v>-windows-x64.zip`, so it
   downloads the compatibility zip.
2. It self-tests the new game as `openomsi.exe --self-test`, sets its own files aside as
   `*.old-update`, puts the new ones in under the old names, and starts
   `openomsi-launcher-app.exe` (or `openomsi.exe`). That is the new code under the old name.
3. At its first start the new code (`omsi_platform::rename::migrate_install`, called first
   thing by the game and by the app):
   - renames `openomsi.exe` to `omsi-hub-game.exe`, `openomsi-launcher-app.exe` to
     `Omsi-Hub.exe` and `openomsi-launcher.exe` to `omsi-hub-cli.exe`. Windows lets a running
     program be renamed, so it renames itself too;
   - when a program is there by both names, keeps the newer file under the new name;
   - writes the new names into the update's file list (`.openomsi-files`);
   - points the player's shortcuts (`.lnk` on the Desktop, in the Start menu, and the pinned
     ones on the taskbar and in Start) at `Omsi-Hub.exe`. A shortcut to the old terminal tool
     points to `omsi-hub-cli.exe`.
4. At the next healthy start the `*.old-update` files go, as after any update.
5. From then on the updater takes `Omsi-Hub-<v>-windows-x64.zip`.

`migrate_install` only touches the folder of the running release install. It never touches a
development build (`target/…`), the update's staging folder or a macOS bundle. A file that
cannot be deleted yet is set aside as `*.renamed-old` and removed at a later start.

**Going back to an older version.** The versions page installs a release from before the
rename: the new updater finds only `openOMSI-<v>-…` and installs it. `install_archive` also
sets aside the new name of each program that the archive brings by its old name, so the
folder never holds both. `program_after_update` then starts the program by whichever name the
installed version uses.

### macOS

The old app installs `openOMSI-<v>-macos-<arch>.zip`. The updater replaces the running bundle
whatever it is called, so the bundle keeps its folder name (`openOMSI.app`). Inside it are the
new binaries under their old names, and the new code runs with the fallbacks. The next update
installs `Omsi-Hub-<v>-macos-<arch>.zip`, whose binaries carry the new names, and
`program_after_update` finds them. The Dock and the menu bar already said Omsi-Hub. A new
install from the `.dmg` is `Omsi-Hub.app`.

### Linux

There is no fork release for Linux. A self-built install of the old names is renamed by
`migrate_install` as on Windows, without the shortcuts.

## The local deploy into `dist\windows`

Remove `openomsi.exe`, `openomsi-launcher.exe` and `openomsi-launcher-app.exe` (and their
`.vorige` copies). Copy in `omsi-hub-game.exe` and `omsi-hub-cli.exe` from `target\release`,
and `Omsi-Hub.exe` from `launcher\src-tauri\target\release`. Leaving the old files in place
also works: the first start of the new app renames them, as for a player.

## Left as it is

- The data folder `~/.openomsi`, the content folder's name, the Tauri identifier and the Mac
  bundle identifier (see above).
- Texts that mean the upstream project: "built on openOMSI", the add-on's "Your openOMSI"
  tab, credits, and the GreenTeaSpeak openOMSI plugin.
- The Discord application. Discord shows the name registered with the application ID
  (`omsi_platform::discord::DEFAULT_APP_ID`), not one the program sends. Showing "Omsi-Hub"
  there needs a Discord application of that name and its ID in `DEFAULT_APP_ID`. The
  presence's own text (`large_text`) already says Omsi-Hub.
- The game-drawn texts in upstream's `crates/omsi-app/locales/app.yml` that still say openOMSI.
  The ones Omsi-Hub's launcher shows are renamed through `crates/omsihub-locales/locales/rename.yml`.
- upstream's Xcode app (`apple/`, `scripts/build-apple.sh`) keeps its names apart from the
  embedded programs.
