# Company UI redesign — status (branch `r65-company-ui`)

The work stopped early because the cloud credit ran low. Everything below is committed and pushed.
`cargo test --release -p omsi-launcher-core -p omsi-launcher-api -p omsihub-locales` passes, and `node --check` passes on every changed JS file.
**Nothing was run in a browser or the launcher.** The layout and behaviour are untested.

## Done
- **Sidebar** (`company.js` `Side`, CSS `co-nav-*`): replaces the row of page icons. It has:
  - the company's mark and name;
  - one chip, e.g. "Sandbox · Realistic", that opens the settings;
  - the start screen;
  - **Pinned**: a star on each page, saved per company in localStorage `co-pins:<id>`;
  - the pages grouped under OVERVIEW / OPERATIONS / PEOPLE / BUSINESS / COMPANY, each with its name and badge;
  - Settings under Company.
- **Groups** come from the API: each rail item carries `group` (`RAIL_HEADS` in `company.rs`, aligned with `RAIL`; a test checks the lengths).
  - Overview: Dashboard (0), Agenda (14), Inbox (13).
  - Operations: Planning (5), Operations (15), Lines (3), Map (9), Fleet (1).
  - People: Staff (2).
  - Business: Finances (4), Concessions (8), Contracts (12), Reputation (16), Career (6).
  - Company: Depot (7), Workshop (11), Research (10), Multiplayer (18, only while Play together is on), Sandbox (17, only for a sandbox company), Settings (opens the dialog).
  - Casual shows its own pages without headings.
- **Collapse and slide away:**
  - The foot of the sidebar folds it to an icon rail with tooltips, or hides it completely (×).
  - **Ctrl+B** slides it away and brings it back; a menu button in the top bar also brings it back.
  - The state is remembered (localStorage `co-nav`, `co-nav-back`).
  - It starts as a rail when the stage is narrower than 1200 px (small window or large UI scale), and folds when the window goes below 1000 px.
- **Slim top bar** (`Top`, CSS `co-top-*`, still class `co-bar`):
  - the page title, or Casual's crumbs;
  - a search button (Ctrl K);
  - cash and reputation (trophies are now in the cash's tooltip);
  - date and time, with the lead or label as a small hint;
  - ONE split button, Simulate to tomorrow ▾, whose menu holds ×60, ×600, +1 h and the time dialog;
  - a bell with a badge (unread inbox + agenda "needs you", from the new `co_view` "bell");
  - the avatar menu: Account (the unchanged `OnlineButton`, re-placed), Multiplayer, Company settings, Light/Dark mode, Help/tour;
  - Back.
  - A joined player gets the remote clock, no bell, and settings only with the `settings` permission.
  - Casual gets its clock with Next day.
- **Dashboard** (overview page): the six KPI cards, the chart, and a **Needs you** card where each item has its button. Its sources (`overview.rs` `needs_you`):
  - the alerts: no lines, buses or drivers ("Hire staff"), uncovered tours, unplanned lines, low cash, service due, unhappy staff;
  - bus contracts rated orange or red;
  - service control waiting (breakdowns, late buses and late drivers);
  - complaints to answer;
  - the agenda's "needs you" entries for the coming week.
- **Command palette** (`cmdk.js`, CSS `co-cmdk-*`): opened with Ctrl+K or the search button.
  - It does a fuzzy search over Recent / Pages / Actions, with icons, grey descriptions and key hints, and works with the arrows, Enter and Esc.
  - It offers only the sidebar's pages, so it follows a joined player's permissions; the actions are offered only to the boss, and Casual gets fewer.
- **Tour**: the selectors now point at the sidebar, the mode chip and the avatar menu.
- **Locales**: `company-shell.yml`, in all 6 languages, listed alphabetically in `TABLES`.

## Not done
- The tour **texts** still describe the old bar: coNav mentions "the icons on the right", coStatus says "Cash, trophies, reputation", and coDone says "this question mark". They need new texts and translations. The tour's `LAYOUT` was not bumped, so returning players won't see the new shell's tour.
- There is no separate "RosterFlow" sidebar item; Planning opens RosterFlow when its add-on is on. The palette does have a RosterFlow action.
- The 100 % and 200 % UI-scale check was not done.
- The old `Bar` is still used for the start/founding screens and the launch page (`sh.launch`); the old `.co-nav`/`.co-rail` CSS is now unused for company pages but not deleted.
- The start screen's tiles are unchanged; "Other company" is gone from the dashboard but is still on the start screen and in the palette.

## Possibly broken or to check on Windows
- Pages that size themselves (`useSize`) now get a narrower area beside the sidebar; check the crowded pages (planning, fleet map, dealer) at 100 % and 200 %.
- The Account row in the avatar menu forwards its click to `OnlineButton`'s own button; check that the online panel opens and closes properly.
- The bell's agenda count is worked out in the shell tick, cached by generation and at most every 5 s; check the cost on a large company.
- The sidebar's `Item` is an inner component, so it remounts on every render: harmless, but the star button can lose focus.
- RosterFlow (full screen) and the remote and Casual modes were not clicked through.
