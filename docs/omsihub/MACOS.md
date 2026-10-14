# Omsi-Hub on a Mac

Omsi-Hub (openOMSI with the Omsi-Hub interface) runs on macOS 11 or newer, on Apple silicon
(M1 and later) and on Intel Macs. The Mac build cannot be made on a Windows PC: GitHub Actions
makes it (`.github/workflows/omsihub-release.yml`, job `mac`), as a **test build** - kept with
the run for 14 days, not published as a release.

## 1. Start a Mac build

Either of these, on GitHub (github.com/Luc-nbr/openOMSI):

- **Push to the branch `mac-test`.** Every push builds it (`git push origin <commit>:mac-test`).
- **By hand:** *Actions* > *Omsi-Hub release* > *Run workflow*, choose the branch, tick
  **"Only the Mac test build of this branch"**, *Run workflow*.

Neither publishes a release or touches `beta`. The run builds for Apple silicon and for Intel
(about 30-40 minutes). When it is green, open the run: under **Artifacts** are

- `omsihub-macos-arm64` - Apple silicon (M1, M2, M3, M4 ...)
- `omsihub-macos-x64` - Intel Macs

each a zip with `Omsi-Hub-<version>-macos-<arch>.dmg` (to install) and
`openOMSI-<version>-macos-<arch>.zip` (the same app as the updater's release file would be).
Which Mac you have: Apple menu > *About This Mac* > *Chip* (Apple M...) or *Processor* (Intel).

## 2. Install and open it (signed ad hoc, no Apple Developer account)

The app is signed ad hoc (free, no Apple Developer account), not notarised: macOS does not know
who made it and asks before it opens the first time.

1. Unzip the artifact and open the `.dmg`. Drag **openOMSI.app** onto **Applications** (or
   into any folder you can write to - not the `.dmg` itself, or it cannot update or keep mods).
2. In Applications, **right-click** (or Ctrl-click) **openOMSI.app** > **Open**, then **Open**
   again in the dialog. From then on it opens with a double-click. (macOS 15 and newer: if
   there is no Open button, go to *System Settings* > *Privacy & Security*, scroll down to
   "openOMSI was blocked" and choose **Open Anyway**.)
3. If macOS says the app **"is damaged and can't be opened"**, the download's quarantine
   flag is in the way. In Terminal:

   ```
   xattr -dr com.apple.quarantine /Applications/openOMSI.app
   ```

   then open it again.

The app shows itself as **Omsi-Hub** (Dock, menu bar); its file stays `openOMSI.app`.

## 3. OMSI 2's content

OMSI 2 runs on Windows only, so on a Mac there is no Steam OMSI 2 to find. Copy it over:

1. On a Windows PC with OMSI 2: Steam > right-click **OMSI 2** > *Manage* > *Browse local
   files* - that is the OMSI 2 folder (with `Omsi.exe`, `maps` and `Vehicles`).
2. Copy the **whole folder** to the Mac (a USB stick, a network share), for example to your
   home folder as `~/OMSI 2`.
3. Start Omsi-Hub. The welcome asks where OMSI 2 is: **Browse** to the copied folder (or type
   its path) and choose **Use this folder**. Later: *Settings* > *Setup*.

Omsi-Hub only reads that folder. Mods go into the content folder beside `openOMSI.app`
(the *Mods* page, or *Setup* > *Open the content folder*), as on Windows.

## 4. Test on your Mac

Tick them off:

1. The opening plays: the Omsi-Hub logo draws itself (ring, bus, line, stops, "Omsi-Hub"),
   then the start comes up. A click skips it.
2. The bar shows the Omsi-Hub logo and the version; the window title says Omsi-Hub.
3. The start: the four ways to drive, the service record, the links - nothing overlapping.
4. Every drive step up to the bus step: Profile, Mode, Map, Day & weather, Duty (and Start
   point for a free drive), Bus - the bus's 3D view turns smoothly. (No need to start a drive;
   do so only if you want to try the game itself.)
5. The bus company: open a company (or found one), its pages, the dealer, a dialog.
6. RosterFlow (Planning page with the add-on on):
   - drag a trip in **Vehicles** onto another bus;
   - in **Drivers**, drag a trip onto another duty and onto *New duty*, **Alt**-drag a whole
     piece, **Shift**-click and **Cmd**-click to choose more trips;
   - **Cmd + scroll** over the Gantt zooms it (not the whole launcher);
   - **Cmd + Z** / **Cmd + Shift + Z** undo and redo.
7. The editors: line editor, timetable, depot editor, livery studio.
8. Light and dark: the moon button in the bar; both looks read well.
9. Window size: make the window small and large; **Cmd + scroll** outside RosterFlow and
   **Cmd + / Cmd - / Cmd 0** change the Launcher size (a short note says the size);
   Settings > General > Launcher size. Nothing runs off the window or overlaps.

Tell Luc what did not work, with `~/.openomsi/launcher.log` (Finder: *Go* > *Go to Folder* >
`~/.openomsi`).

## 5. What was not tested

This PC cannot build or run the Mac app, so before the first run on GitHub:

- **Checked:** the launcher's Rust code compiles for macOS (Apple silicon): `cargo check
  --target aarch64-apple-darwin` of `omsi-launcher-core`, `omsi-launcher-api` and the Tauri app
  (`launcher/src-tauri`) - no errors, no warnings.
- **Not checked here:** `omsihub-game` and the game program (`omsi-app`) for macOS - their
  audio (cpal's CoreAudio) needs Apple's SDK to compile; upstream builds them on macOS in
  `release.yml`, and our job builds them the same way (`scripts/build-macos.sh`). Our code in
  them has no Windows-only parts left (the livery studio's fonts are looked up in the Mac's
  font folders; fonts it does not find are left out of its list).
- **Not run:** the app itself on a Mac - the opening, WebKit's drawing of the pages (Safari's
  engine, not WebView2), the dialogs to choose a folder, the game's window, Metal, the
  updater on macOS (a test build is never updated: its version has a "-").

The logo's wordmark "Omsi-" is set in Switzer (Indian Type Foundry, Fontshare) as outlines;
the font is not in the repository.
