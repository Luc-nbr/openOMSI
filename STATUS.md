# r65-pages-b: status

The content of five company-mode pages redesigned in one look (light content area, rounded cards
with soft shadows, the Omsi-Hub orange, KPI cards on top, compact empty states). The shared
pieces are `launcher/ui/js/omsihub/company/pagesb.js` and `launcher/ui/omsihub/company-pages-b.css`;
every colour comes from the brand tokens (`--hub-*`, omsihub.css), none is hard-coded. The new
texts are in `crates/omsihub-locales/locales/company-pages-b.yml` (de, fr, nl, pl, ru, uk).

The concept images could not be fetched from this environment (the host was blocked): the pages
follow the written descriptions and Luc's Operations brief.

## Done

- **Research**: a wide "current research" card (icon, name, stage, progress, days left,
  researchers, effect, Pause/Resume and Stop with its second click); sections with compact tiles
  (icon, title, one-line effect, money/trophies/time chips, Researched / Start / Locked with its
  prerequisite); a detail panel for the tile picked (beside the tiles, above them when narrow).
  Core: `research::Running::paused` (`#[serde(default)]`) and `research::pause`; API action
  `research_pause` (relayed as Spend).
- **Service control** (Luc's brief): clock and date, the alert threshold as a segmented control,
  the bell with `control::waiting`; KPIs (on time as a ring, buses late of those in service,
  drivers late of those on duty, breakdowns); the live incidents table (severity bar and label,
  line plate in its colour, bus, delay and where, cause and detail, the existing ways out as
  compact buttons with the suggestion filled and the effect as tooltip; late drivers and
  breakdowns in the same table); "All clear" with the lines running normally, faded by age; the
  fleet map embedded (`FleetMapPage({ embed })`: buses coloured by delay against the threshold,
  grey when not in service, legend, zoom and locate, a row click centres the map); what was
  done today; the last day's advice, folded. The map drops under the table when narrow.
- **Multiplayer**, not shared: hero "Run your company together" with Start sharing, the three
  steps as cards, "Your friends online" (from `ol_state`, Invite starts sharing first) or a
  sign-in prompt, a Roles card. The shared state is unchanged.
- **Sandbox**: head with the settings changed and Reset to career; presets Relaxed / Career /
  Hard / Custom (`sandbox::PRESETS`, action `sandbox_preset`, relayed as Settings); group tabs
  with changed counts; rows with a description, a ticked slider and value chip, the career value,
  switches, an orange dot when changed. Stale texts fixed: the income knob ("Income": fares,
  concession reimbursement, contract payments and BSOG - no payment per km), the penalties knob
  (the bus contracts' deductions and default points), inflation (contracts' payments indexed),
  unlimited money ("every amount", not "every euro").
- **Workshop**: KPI strip; the parts as a table (stock on a level bar with the minimum, on order,
  price with the old one struck through, a quantity stepper, the line total); a sticky order card
  (delivery segmented, sum, Order); cards for buses waiting, orders, auto-ordering, mechanics.

## Presets

| knob | Relaxed | Career | Hard |
|---|---|---|---|
| never go broke / loans without interest | on / on | off / off | off / off |
| income | ×1.5 | ×1 | ×0.8 |
| wages / running costs / bus prices | ×0.8 / ×0.8 / ×0.8 | ×1 | ×1.2 / ×1.25 / ×1.25 |
| contract penalties | ×0.5 | ×1 | ×1.5 |
| inflation | off | on | on |
| breakdowns / accidents / delays / illness | ×0.5 / ×0.5 / ×0.7 / ×0.5 | ×1 | ×1.5 / ×1.5 / ×1.3 / ×1.5 |
| reputation gained / lost | ×1.5 / ×0.5 | ×1 | ×0.75 / ×1.5 |
| hire without limits | on | off | off |

Unlimited money, everything unlocked, complaints, morale and holidays stay as in a career.

## Not done / open

- No UI click-through was run (as asked); the layouts are untested at 100 %/200 % and in both
  looks beyond reading the CSS.
- The bell uses the `campaign` icon (the symbol set has no `notifications`).
- The Operations map only shows buses once the fleet map has read the roads (as the Fleet map
  page does: `co_more_map` through the game program).
- `reputation.rs` (another session's page) still says "the authority's payment per kilometre".

## Shared files touched

`company/fleetmap.js` (an `embed` mode, additive), `company_more/fleetmap.rs` (a bus's minutes
late, `m`), `together/relay.rs` (permissions of `research_pause` and `sandbox_preset`),
`omsihub-locales/src/lib.rs` (the new table), core `research.rs` and `sandbox.rs`.

## Tests

`cargo test --release --no-fail-fast -p omsi-launcher-core -p omsi-launcher-api -p omsihub-locales`
passes on Linux (core 635 passed, api 146 passed, locales 1 passed; the ignored ones as before).
New tests: `research::a_paused_research_waits_and_goes_on_where_it_was`,
`sandbox::a_preset_sets_every_knob_and_is_recognised`. `node --check` passes on every changed
script. Not yet built or run on Windows.
