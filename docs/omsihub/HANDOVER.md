# Omsi-Hub: handover for the next AI

This document is for an AI assistant that takes over this project with no prior knowledge. Read it in full before you change anything.

## 1. What this is

**Omsi-Hub** is Luc's fork of **openOMSI**, an open-source Rust remake of the bus simulator OMSI 2. The upstream project is `openOMSI-org/openOMSI` (formerly `openOMSI-Project`), and the fork is `Luc-nbr/openOMSI` on GitHub. On top of openOMSI the fork adds:

- **The Omsi-Hub launcher:** a Tauri 2 desktop app whose UI is Preact + htm with **no build step**. It is the only launcher; the old Rust/egui launcher was removed in Beta 52.
- **The bus company mode** ("career"):
  - founding a company, the fleet, the dealer, staff, lines, tenders and concessions, contracts, finances, reputation, complaints, the agenda;
  - the day's close (the economy simulation), and a company clock that runs with real time.
- **RosterFlow:** the company's scheduling studio, made to look and work like Optibus. It has the tabs Timeplan, Vehicles (blocks), Drivers (duties) and Roster, plus Preferences/rules, Optimise and relief points.
- **Editors** (line editor, depot/HOF editor, timetable), a **WebGL 3D bus view**, a **livery studio** (Lakstudio) and bus photos.
- **Play together:** company multiplayer with a session code. It is built but **gated** off (see §8).

The owner is **Luc** (GitHub `Luc-nbr`). He tests every build himself. Testers give feedback, usually forwarded in English with screenshots. Luc writes in Dutch.

## 2. Working agreements (follow them strictly)

- **Language:** talk Dutch with Luc. Write everything else in English: code, comments, commits, docs, Trello cards.
- **Commit every change at once**, without asking, and leave nothing uncommitted at the end of a turn. Commit messages end with a `Co-Authored-By:` line for the AI.
- **Pushing is a release.** A push to `beta` triggers a public release for all players, Windows and Mac. Push only after Luc says yes ("push", "release"). Never force-push.
- **Only build, no test rounds.** Luc: "we gaan enkel bouwen, geen uitgebreide testrondes, die doe ik".
  - No review, jury or verification passes, and no UI click-through sessions.
  - Unit tests (`cargo test`) are fine.
  - **One exception:** before deploying a new launcher build, start it once in a sandbox and check that the page loads without JS errors (§5). Beta 59 once shipped a blank window.
- **Trello** (the board "Omsi-Hub Addon for Open-Omsi", https://trello.com/b/MdRsC151) is public and hundreds of people watch it.
  - Ask Luc before any Trello write: a card, comment, move or label.
  - Never put testers' names on it; write "tester".
  - No local paths or internal notes on cards.
  - Lists are phases: Inbox · new ideas, Inbox -Bugs, Phase 1 … Phase 7, Later, Done.
  - Labels: red bug, blue feature, purple RosterFlow/planning, yellow small improvement, orange needs a decision, green done.
- **Never touch Luc's real data or game:**
  - never start the game, and never click Start/Drive in a launcher;
  - never kill processes you didn't start;
  - never write to `C:\Users\lucru\.openomsi`, the OMSI 2 folder or `C:\OMSI Career` (the old Electron app). Reading them is fine.
  - Use sandbox homes (§5).
- **Speed:** Luc likes parallel builder agents (4–5 at once) in separate git worktrees, each on its own area, with the main session merging. Keep briefs complete, since builders know nothing.
- **Switzer font:** never commit Switzer font files (licence). The "Omsi-" logo is committed as SVG paths only.

## 3. Repositories, branches, worktrees

| Path | What |
|---|---|
| `C:\openOMSI` | main clone (branch `main` = upstream mirror); `dist\windows\` is where Luc runs builds from |
| `C:\oo-wt\beta` | worktree of **`beta`**, the fork's main branch. Merge everything here; it has no `target\` of its own |
| `C:\oo-wt\samen`, `fonts`, `route`, `hof`, `vertaal` | worktrees for builders. Each has a warm `target\`, except `hof`, which uses `C:\oo-wt\font\target`. Make a new branch from `beta` per task (`git checkout -b rNN-topic beta`) |
| `C:\OMSI Career` | the old Electron app ("OMSI Enhancer"), reference only |

Remotes: `origin` = Luc-nbr/openOMSI, `upstream` = openOMSI-org/openOMSI. The workflow `omsihub-sync.yml` merges upstream `main` into `beta` on a schedule.

## 4. Code map

**Rust crates** (`crates/`):
- `omsi-app`: the game.
- `omsi-launcher-core` (lib name `omsi_launcher_lib`): the company core lives in `src/company/`. Key files:
  - `model.rs`: Company, Vehicle, Employee, …
  - `day.rs`: the day's close
  - `clock.rs`: the live day, breakdowns, spares
  - `plan.rs`: roster and dispatcher
  - `network.rs`: tours of a day
  - `studio.rs`: blocks and duties
  - `rosterflow.rs` and `rosterflow/`: prefs, issues, Optimise, signing, relief points
  - `plandays.rs`: copy a plan to days
  - `lateness.rs`: the single delay model, causes and penalty tiers
  - `control.rs`: live service control
  - `reputation.rs`, `complaints.rs`, `holidays.rs`, `agency.rs`, `agenda.rs`
  - `registrations.rs`: UK plate ages
  - `market.rs`, `dealer.rs`, `economy.rs`: prices and values by age
  - `concessions.rs`: tenders, 70 % price / 30 % quality
  - `together/`: the company multiplayer server and client
- `omsi-launcher-api`: launcher commands. `src/omsihub/` holds `company.rs` and `company/` (pages: dialogs, people, money, ops, reputation, …), `rosterflow.rs` and `rosterflow/` (roster, days, breaks, relief, timeplan…), `editors/`, `bus3d/`, `lak/`, and `mod.rs` with the **locale table**.
- `omsihub-game`, `omsihub-livery`: line editor, livery engine.
- `omsi-lan` / `omsi-net`: openOMSI's multiplayer (session codes, gateway, Cloudflare tunnel).

**Launcher UI:** `launcher/ui/js/omsihub/…` (company/, rosterflow/, editors/, bus3d/), CSS in `launcher/ui/omsihub/`. The Tauri shell is `launcher/src-tauri` (its own Cargo workspace; build with `--locked`).

**Translations:** six languages (en, de, fr, nl, pl, ru, uk as the tests define). Each feature has its own YAML in `crates/omsi-app/locales/*.yml`, registered in `omsihub/mod.rs`. The API tests fail if a page text is untranslated.

**Old saves must keep loading:** put `#[serde(default)]` on every new field.

Docs: `docs/omsihub/BUS_COMPANY_OVERVIEW.md`, `TAURI_PARITY.md`, `MACOS.md`, and `docs/DEBUG_FLAGS.md` for `OMSI_*` flags.

## 5. Building, testing and deploying a local "Beta"

Local test builds are named **Beta N** with version `0.2.48-betaN`. The last was **Beta 60**; the next is **Beta 61**. The updater ignores `-beta` versions.

```bash
# tests (warm target)
cd /c/oo-wt/beta && CARGO_TARGET_DIR=/c/oo-wt/samen/target cargo test --release -p omsi-launcher-core -p omsi-launcher-api
# build
export OPENOMSI_VERSION=0.2.48-beta61
CARGO_TARGET_DIR=/c/oo-wt/samen/target cargo build --release --target x86_64-pc-windows-msvc -p omsi-app -p omsi-launcher-core
cd launcher/src-tauri && CARGO_TARGET_DIR=/c/oo-wt/samen/launcher/src-tauri/target cargo build --release --locked --target x86_64-pc-windows-msvc
```

Build notes:
- This PC has **no cmake**, so a cold target fails on `openxr-sys`. Always use a warm target (`samen\target`).
- `node --check` misses some errors that WebView2 rejects, such as a raw line break inside a quoted string.

**Smoke test before deploying (mandatory):** start the new `openomsi-launcher-app.exe` with a sandbox home (`HOME`/`USERPROFILE`/`APPDATA`/`LOCALAPPDATA` pointing to a scratch folder) and `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9391`. Connect over CDP (`/json`, then `Runtime.enable`, `Page.reload`) and make sure there is no `Runtime.exceptionThrown`. Then stop that process.

**Deploy** to `C:\openOMSI\dist\windows` with backups, then verify:
- for each of `openomsi.exe`, `openomsi-launcher.exe` and `openomsi-launcher-app.exe`: move `X.vorige` to `X.vorige2`, move `X` to `X.vorige`, copy in the new `X`;
- check the sha256 matches the build, and `grep -c 0.2.48-betaN` finds the version string (`openomsi-launcher.exe` has none);
- renaming a running exe works on Windows;
- never put a Beta beside the exe under another name.

## 6. Releasing (only after Luc's yes)

1. `git -C C:/openOMSI push origin beta`. That triggers `.github/workflows/omsihub-release.yml`:
   - **job `release`** (Windows) runs the workspace tests and publishes `v0.2.48.N` (N counts the fork's commits) with `openOMSI-<v>-windows-x64.zip`;
   - **job `mac`** builds arm64 and x64 (`scripts/build-macos.sh`, ad-hoc signed);
   - **job `mac-publish`** attaches `Omsi-Hub-<v>-macos-*.dmg` and the updater zips to the same release.
2. Watch it: `gh run list -R Luc-nbr/openOMSI`, then `gh run view <id>`. If a Mac runner flakes ("lost communication"), run `gh run rerun <id> --failed`.
3. If a push starts no run, start it by hand: `gh workflow run omsihub-release.yml -R Luc-nbr/openOMSI --ref beta`. Do **not** tick "mac" for a release; that input means "Mac test build only, no release".
4. **GitHub disabled Actions on this fork once** "due to the scale of GitHub Actions usage". Luc re-enabled them on the Actions page. To avoid it, reduce the hourly sync and switch off the unused upstream workflows (Apple, Website, PR test builds, PR checks, Issue triage, Security audit). Ask Luc first; he hasn't decided yet.

The latest public release is **v0.2.48.397**, which equals Beta 57. Beta 58–60 are local only.

## 7. State at handover (2026-10-15)

`beta` is at `8810d521` and holds Beta 60.

**Recently built, Beta 54–60:**
- tester feedback rounds on RosterFlow (Trello #95–#131, mostly in Done);
- relief points along a route and mid-trip cuts;
- break labels and driving-period bands;
- copying a plan to other days, plus "Copy to…";
- unassigning;
- agency drivers: the whole duty, booking ahead, no limit;
- holiday requests and the holiday planner;
- the minimum duty length;
- Optimise fixed;
- colour by trip ID;
- per-duty sign-on, sign-off and break points;
- the agenda;
- breakdown notices with spares;
- vehicle age from UK registration plates (`.org` lists in the bus folders), or typed by the player, with prices and values by age;
- the lateness model: minutes, causes and penalties (≤5 min = 10 %, rising linearly, ≥30 min = 100 %), plus why late / why dropped and advice;
- the Operations page "Service control" (short-turn, out of service, layover, spare swap, cover a late driver);
- tenders at 70 % price / 30 % quality;
- milder reputation with daily/weekly caps and recovery, plus a reputation page;
- passenger complaints with scripted dialogue;
- company multiplayer, gated.

**Open points and known doubts:**
- **Balancing:** the lateness weights (peak, driver, bus), the reputation numbers, the accident rate and the holiday morale effects are builders' guesses. Tune them only on Luc's feedback.
- **The game side:**
  - during a running game, a broken-down bus keeps driving in the game (its day plan is fixed at game start);
  - the game overlay doesn't receive company notices.
- **Copying a plan:** only the switcher's 7 days can be targets, and mid-trip reliefs are not carried into copies.
- **Timetables:** saving them per school day, holiday or single date is not built (Trello #100, partly done).
- **Agency undo:** Roster-tab undo doesn't cover agency bookings made on the staff page.
- **Multiplayer:**
  - never tested between two real players;
  - booking is per tour (a player driving part of a tour gets the whole tour);
  - re-enable it with `OMSI_TOGETHER=1` for testing.
- **Luc's platform idea, not started:** later, Omsi-Hub becomes a separate platform or add-on outside openOMSI's code. Users would switch it on as an add-on in the official openOMSI launcher, and it would launch openOMSI and provide the navigator and overlay via a Lua plugin. This needs an add-on extension point upstream. Don't start without Luc.

## 8. How to take a request from Luc

1. **Understand it.** Testers' feedback often arrives as screenshots plus one line. Find the code and the real cause first.
2. **Small fix:** do it yourself in `C:\oo-wt\beta`, commit, build, smoke-test, deploy, report.
3. **Larger work:** split it by area and start builders in worktrees. Give each a branch from `beta` and a complete brief (area, what to build, hard rules from §2, the "only build" rule, report format). Merge each branch into `beta` as it finishes; resolve conflicts keeping both sides. Watch out for tab indices in `api company.rs` (`TABS`, `ICONS`, `DRAWN`, `RAIL`) and the locale table in `omsihub/mod.rs`. Run the tests, build the next Beta, smoke-test, deploy, and report in Dutch: what changed, what to watch for, what's open.
4. **Trello:** only when Luc asks (e.g. "maak kaarten", "zet op done").
5. **Release:** only when Luc says so.
