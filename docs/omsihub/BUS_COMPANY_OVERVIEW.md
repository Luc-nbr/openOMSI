# The Omsi-Hub bus company: an overview for another AI

This file explains what the **bus company mode** of the Omsi-Hub fork of openOMSI does, how it is built and where its code lives. Read it before working on or answering questions about the bus company.

## 1. Context

- **openOMSI** is an open-source re-implementation of the bus simulator OMSI 2, written in Rust. It reads OMSI 2's maps, buses and timetables.
- **Omsi-Hub** is Luc's fork, `github.com/Luc-nbr/openOMSI`, release branch `beta`. It adds its own launcher with:
  - four ways to play: Work shift, Tour, Free drive and **Bus company**;
  - editors for lines, liveries and depots;
  - the bus company mode with its planning program **RosterFlow**.
- **Releases:** a push to `beta` builds a release named `v<major.minor.commit>.<n>` through GitHub Actions. The game's updater installs it for players.
- **Launcher:** today it is the game-drawn Rust launcher (`crates/omsi-launcher-ui`). A rewrite into openOMSI's Tauri 2 launcher (`launcher/`) is in progress in six phases. The Rust launcher stays the default until the rewrite reaches parity.

## 2. What the bus company is

The player runs their **own transport company** on one of their installed OMSI maps. They:

- buy, lease or rent buses;
- hire and train staff;
- take on the map's timetabled lines through tenders, or draw lines of their own;
- plan who drives what;
- drive duties themselves in the game.

The rest of the company is simulated: its AI buses drive the planned tours on the map, and the economy books revenue and costs.

**Time.** The company has its own clock, with a date and the minute of that day. The player can:

- let time run;
- simulate +1 hour or "to tomorrow";
- (in Casual) press "Next day".

Every simulated day plays out its events: sick calls, lateness, breakdowns, the dispatcher's choices, incidents and the close of the day with its books. Time can also follow the real clock; a long absence is caught up, up to 31 days.

**What the player drives** counts as measured: punctuality, km and a trip score. Duties the player does not drive are simulated with the company's model.

## 3. Game modes

There is one choice per company, which can be changed at any time in the company settings. It replaced the older "Easy/Advanced mode" and "Easy/Realistic/Hard difficulty".

| | Casual | Normal | Realistic |
|---|---|---|---|
| Interface | Its own simple interface: Today, Drive a shift, 4 tiles, tips, simple pages | Full launcher, with hints | Full launcher, compact |
| Planning | Fully automatic (autopilot) | The roster repairs itself every morning; the player can step in | The player plans and repairs |
| RosterFlow | Not available | Optional add-on | Optional add-on |
| Disruptions | Rare, solved automatically | Calmer than real life | Real-life frequency |
| Economy | Generous | Close to real | Real (tight) |

## 4. The areas of the company

1. **Founding.** A wizard asks for:
   - the name, short name and colours, plus a logo;
   - the home map and the depot file;
   - the first day;
   - the game mode;
   - the buying mode.
   
   Several companies can exist; a chooser opens one.
2. **Economy and money.** Revenue comes from passengers (fares per line, patronage), concession fees and contracts. Costs are:
   - wages;
   - fuel and energy;
   - maintenance;
   - insurance;
   - depot costs;
   - leases and rents;
   - loans: banks lend against the fleet value.
   
   There are monthly books, a cash and reputation figure, and trophies earned with goals.
3. **Fleet.**
   - The **dealer** is a showroom of every installed bus model, by maker and model, at the price of its kind. Players can haggle, choose extras, sign a contract and wait for delivery.
   - A used **market** offers buses by age and km.
   - Buses can be bought, leased or rented.
   - Buses are classified by category (midibus, solo, articulated, double-decker, coach) from their files.
   - Each bus has a condition, wear, km, a value, liveries and adverts.
4. **Staff.** There are roles: drivers, mechanics, researchers, cleaners, service staff and ticket inspectors. Each role has its own labour market, with new applicants every week.
   - Hiring and dismissing with notice.
   - Wages by experience.
   - Satisfaction, stress, illness and holidays.
   - **Licences and type training:** a driver may only drive buses they are qualified for.
   - **Training courses:** punctuality, eco driving, customer service and more.
   - **Working-time rules:** a duty is at most 9½ h, a working day at most 10 h, with 20 min between pieces of work, 11 h rest overnight and 45 min for a transfer.
   - **Agency drivers** fill gaps, up to 2 + 1 per 3 drivers employed.
5. **Lines.**
   - **Map lines** are won through **concessions**: a tender with fictitious rival bidders and an auction, where the bid matters. Concessions run for a term and can be renewed.
   - **Own lines** are drawn in the line editor. The editor supports circular routes and separate return-trip times, shows what a line costs and brings, and the player confirms and pays.
   - Each line has a fare.
   - Special trips (depot runs, empty runs, other operators' traffic) are not lines to run.
6. **The day and the dispatcher.** Each day, the tours of the company's lines (`network::tours_of_day`) get buses and drivers from the weekly roster (`plan`). In the morning, sick calls, late drivers and buses that did not start create open places. The dispatcher's choices are:
   - a free colleague (standby first);
   - an agency driver;
   - the player themselves;
   - a spare bus of the line;
   - a rental bus;
   - dropping the trips, with the contract penalty.
   
   Every driver who can't take a duty shows the exact reason. "Close the day" settles the day.
7. **Standard planning.** This is the planning without RosterFlow: a Gantt of the day's tours per line, where drivers and buses are given by tap or drag. Its tools are Fill the roster (which also repairs broken places), Clear the day, and Repeat Mon–Fri / weekend.
8. **RosterFlow ("Bus Scheduling & Crew Rostering").** This is the advanced planning add-on, modelled on the professional scheduling software Optibus. It is a whole-window application of its own, with its own Back button. Its tabs follow the order of the work:
   - **Timeplan:** a line's running times per time band, and its timetable grid with trips made by headway. Hand edits show in orange. After saving, the line is UNSYNCED until Refresh.
   - **Vehicles:** trips are chained into **blocks**, one bus's day. A Gantt shows arrow-shaped trip chips in the line's colour, depot boxes and deadheads. Trips are dragged to star-rated drop zones that show the cost difference.
   - **Drivers:** blocks are cut into **duties**, one driver's working day. Cuts happen only at termini or the depot. Duties can be joined, split, built by hand and locked.
   - **Roster:** which named driver drives which duty on which day.
   - **Around the tabs:**
     - KPI tiles: peak vehicles (PVR), duties, paid hours, deadhead km and cost;
     - OPTIMIZE: currently the game's heuristic blocking and runcutting as a proposal to accept or keep;
     - a validation panel with Global, Duty and Vehicle issues;
     - Preferences: hard and soft rules;
     - lock, pin, undo/redo and shortcuts;
     - SAVE and DISCARD: all edits are a draft until SAVE;
     - printing the bus graph and duty graph as PDF;
     - a guided tour with lessons.
   - **New lines with RosterFlow on:** the line editor makes the route only. The timetable, blocks and duties are then built in RosterFlow, and the line stays "No timetable yet" until a timetable is saved.
   - Today a bus keeps to one line, so interlining is not possible yet.
9. **Driving a company duty.** The company duty launcher opens the duty exactly as planned: date, sign-on time, line, trips, bus, depot and map. None of it can be changed. If something blocks the start, the reason is shown and the player is sent back to the planning. In Casual, "Drive a shift" offers 3–4 of today's duties as cards.
10. **Depot and workshop.** The depot is the yard and its buildings, with capacity. The workshop has jobs, repairs and services, and a store of spare parts. Buses break down with wear and age, and maintenance prevents it.
11. **Contracts and goals.** These include service contracts with penalties and company goals with rewards.
12. **Research.** Trophies buy upgrades and unlock larger bus sizes.
13. **Advertising.** Advertisers offer contracts for the rear, side panels or a full wrap, at a monthly fee per bus. These unlock by level, and the price follows passengers, reputation and the game mode.
14. **Levels and career.** The company earns experience and levels that unlock features. The player has a separate driver career, which belongs to the driver and goes beyond the company.
15. **Other parts:**
    - the inbox, where things are told as messages;
    - incidents during trips;
    - rankings against fictitious companies on the map;
    - the fleet map, which shows where every bus of a tour is at the company's current time;
    - a phone companion "Company" tab.

## 5. Where the code lives

- **Core logic** is in `crates/omsi-launcher-core/src/company/`: about 32 000 lines of plain functions over plain data, without any UI. Everything is unit-tested. The key modules are:
  - `model` (the saved company);
  - `store` (`~/.openomsi/companies/<id>.json`);
  - `clock`, `day` and `plan` (the roster, the dispatcher, `repair_day`);
  - `network` (tours of a day);
  - `staff`, `licences`, `training`;
  - `market`, `dealer`;
  - `economy`, `finance`, `fares`;
  - `concessions` and `auction`, `contracts`;
  - `depot` and `parts`;
  - `research`, `adverts`, `levels`, `career`, `inbox`, `incidents`;
  - `autopilot` (Casual's automation; `rosterflow(c)` checks the add-on);
  - `ownline`, `livery`, `fleetmap`, `duty_launch`;
  - for RosterFlow: `studio`, `rosterflow`, `timeplan`, `rosterpdf`;
  - `rng`: seeded draws, so the same company on the same day draws the same numbers.
- **UI (Rust launcher)** is in `crates/omsi-launcher-ui/src/company/`:
  - one file per page;
  - `casual/` for Casual's own interface;
  - `planning/` for the standard planning, plus `rosterflow.rs`, `rf_gantt.rs`, `rf_panels.rs`, `rf_prefs.rs`, `rf_print.rs`, `rf_brand.rs`, `rf_tutorial.rs`, `rf_roster.rs`, `timeplan.rs` and `duty_launch.rs`.
  - The line editor is `crates/omsi-launcher-ui/src/lineeditor.rs`.
- **Tauri launcher (in progress)** is `launcher/ui/js/omsihub/` and `launcher/ui/omsihub.css`. Its commands go through `crates/omsi-launcher-api/src/omsihub/` into the same core.
- **Texts** are in `crates/omsi-app/locales/*.yml`. English is the key, with translations in nl, de, fr, ru, uk and pl. Tests fail when a text is missing.

## 6. Rules when changing it

- Old saves must keep loading: use `#[serde(default)]` and `store::migrate`.
- Keep logic in the core, with tests. The UI only calls it.
- New texts must be translated in all languages of the locale file.
- Runtime switches (`OMSI_*` environment variables) must be registered in `crates/omsi-cfg/src/flags.rs` and `docs/DEBUG_FLAGS.md`, which a test checks.
- **Housekeeping:**
  - code, comments, commits and docs are written in English;
  - every change is committed at once;
  - pushing to `beta` is a release, so it needs Luc's yes.

## 7. Status and plans (October 2026)

- **Done:**
  - the core economy, fleet, staff, lines, day and clock;
  - the dispatcher with full cover choices;
  - game modes and the Casual interface;
  - RosterFlow (Timeplan, Vehicles, Drivers, Roster) as an add-on;
  - line creation through RosterFlow;
  - the company duty launcher.
- **Planned, on the Trello board "OpenOmsi":**
  - a real optimiser;
  - rotating rosters and fairness;
  - a dispatcher with planned-versus-actual;
  - punctuality control;
  - a driver app on the phone;
  - passenger information;
  - interlining;
  - grouping and renaming tours;
  - hired planners that run RosterFlow for a wage;
  - realistic bus prices from the 3D model;
  - an applicant refresh;
  - enabling and disabling vehicle models;
  - balance (wages against revenue);
  - a realistic advertising market and the company's own marketing campaigns;
  - the multiplayer company (a server, roles, open duties);
  - the Tauri 2 rewrite of the whole launcher.
