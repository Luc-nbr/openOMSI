# r65-pages-a: status

Branch of `r65-base`. The redesign of six company-mode pages was stopped early because the
cloud credit ran out.

## Done (not built or tested yet)
- **Shared look**: `launcher/ui/js/omsihub/company/pages-a.js` (Card, Kpi/Kpis, Empty, Chip,
  Dot, Track, HBars, Spark, Tabs, Stars) and `launcher/ui/omsihub/company-pages-a.css`. Every
  class is `co-pg-…`, every rule starts at `:root.hub`, and the colours come from the `--hub-*`
  brand tokens, so they follow the theme and the accent. The layout uses container queries, so
  it works at any content width (next to the sidebar, or full width).
- **Company chooser** (`chooser.js`, `company/chooser.rs`):
  - A card per company: map picture banner (colour gradient fallback), logo badge, name and
    map, one pill ("Career/Sandbox · mode"), the cloud/local chip.
  - Four mini stats (cash, buses, lines, this month) and "date · played X ago".
  - The last played company comes first, marked, with Continue; a dashed "New company" card.
  - New tile fields: `map`, `map_file`, `colours`, `pill`, `stats`, `when`, `last`. The played
    time is the save file's mtime, so no new saved field.
  - The selftest now looks for `.co-pg-company`.
- **Locales**: `crates/omsihub-locales/locales/company-pages-a.yml` (chooser texts: de, fr,
  nl, pl, ru, uk), added in alphabetical order to `TABLES`.

## Still broken
- `crates/omsi-launcher-api/src/omsihub/company/chooser.rs` line 44 does not compile: the pill
  icon is `&str` in one branch and `String` in the other. The fix is to replace the else branch
  with `super::mode_icon(c.game_mode)` (check that `mode_icon` is public in `company.rs`).
- `cargo test` has not been run since these changes; `node --check chooser.js` passes.

## Not done
- **Finances**: KPI cards (cash, this month, gross margin, debt, fleet value); a P&L card with a
  waterfall and Income/Costs bars by booking kind; a compact loans card; a ticket prices card;
  a ledger with All/Income/Costs/Investments filters and search.
- **Tenders**: the list with chips and a countdown; the detail with the timeline, the
  cap/advice/bid tiles, the 70/30 explainer with the expected score (`cn::standings` with a
  hypothetical `Placed` bid), and the rivals table; "Your contracts" with RAG dots; removing the
  scoring sentence at the bottom. Keep `.cm-tender` or update `selftest-more.js`.
- **Goals & contracts**: the header strip; the three-column board with empty states; the other
  tabs in the card style. Keep `.cm-goal` or update `selftest-more.js`.
- **Reputation**:
  - The gauge with an 8-week sparkline. This needs a new `#[serde(default)]` weekly history in
    `reputation::Log`, because the daily history keeps only 35 days.
  - Factor chips grouped Positive/Negative, the "Last week by cause" bars, the action cards.
  - **Stale text not fixed yet**: "What it drives: … the authority's payment per kilometre".
    By the code, reputation now drives passengers (`demand_factor`), advert income, the rating
    stars, a little of the bank's credit room, and own weekend lines' tourism grant. Tender
    quality is the default-point track record (`bus_contracts::quality`), not the reputation,
    so the new text should say that rather than claim reputation sets tender quality.
- **Career**: the profile header on every tab; the Mon–Sun week strip; the empty state with
  "Plan yourself a duty"; "How it works" as three steps.
- Luc's layout-setting request (Sidebar / Command palette) is shell work for the
  r65-company-ui session and was not done here.
