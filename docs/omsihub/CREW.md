# The driver's side of the bus company (r67-crew)

What a driver of the company - the player, or a player of "Play together" - sees and does on a
company duty, besides driving it. Everything runs on the company's own models: the RosterFlow
duties and the day's plan, the staff and holiday model, the workshop's jobs.

| Feature | Rules | Where it shows |
|---|---|---|
| Duty strip | the duty's sheet (`duty_launch::sheet`) and what changed today (`crew::changes`) | the game: `crates/omsihub-plugin/lua/duty_strip.lua` |
| Handover notes | `crew::add_note`, one day long (`crew::night`) | the game (at sign-on, the strip's "Leave a note"), the phone, the Fleet page's bus dialog |
| Walk-around and faults | `crew::report_defect` (a repair job, `depot::order`), `crew::log_fault` | the game (at sign-off), the phone, the Fleet page's bus dialog, the Workshop page |
| Fuel / charge | `bus.fuel`, `bus.get_vars` where the bus has them | the game's strip |
| Week, swaps, holiday, sick call | `crew::ask_swap` / `ai_answer`, `crew::ask_leave`, `crew::call_in_sick` | the phone's Company tab; players of a session on their "Play together" page |

## How it travels

* **Launcher → game**: the company's duty launcher (`omsihub/company/launch.rs`, `crew_of`) puts the
  duty's pieces, today's changes, the bus's notes and the forms' choices into the plugin's
  assignment (`bridge::Crew`).
* **Game → company**: the plugin reports a note, a defect of the walk-around or a collision it saw
  (`bridge::Report` kinds `note`, `defect`, `fault`); the launcher turns each into an order of the
  company's queue (`addon::crew_order`, `remote::queue`), which it carries out with the company's
  rules (`remote::take`).
* **Phone → company**: the companion's Company tab sends the same orders (`remote::Order::Note`,
  `Defect`, `Swap`, `Leave`, `Sick`, `Well`), checked by the game and carried out by the launcher.
* **Players of a session**: their launcher relays `co_crew` (`together/relay.rs`: a driver's
  permission; `company/crew.rs` checks that he acts on his own duty and bus).

## The strip is switchable - and the only one

`duty_strip.lua` draws one panel of its own (`omsihub_strip`). It stays away when the assignment
says `crew.strip = false` (the setting `nav_strip`) or when another module calls
`strip.set_enabled(false)` - there is only ever one duty strip on the screen.

The overlay restoration (`r67-overlay-cloud`) has a "strip" part of its own in `company.lua` (the
next break or relief and the duty's end, switched by the driver on the company's panel). When the
two branches meet, keep this one as the duty strip: `company.lua` leaves its "strip" part out while
`require("duty_strip").enabled()` is true, and its panel's "Duty strip" switch calls
`strip.set_enabled(on)` instead of drawing a strip itself. Nothing else of either needs to change.

## Notifications: where to switch over to the message system

Every notification of the crew features (a note left for you, a swap answered, a defect logged, a
sick call, a holiday answered) goes through **one function**, `crew::notify` in
`crates/omsi-launcher-core/src/company/crew.rs`. Until the message system of r67-messages is
there, it queues them in the company (`Crew::outbox`), and the launcher shows each as its existing
toast and as a card in the game (`omsihub/company.rs`, `crew_notices`; `addon::notify_game`).

When r67-messages lands, replace the body of `crew::notify` with its
`messages::post(c, to, priority, template, args, action)` and drop `crew_notices` (or let it only
mirror to the corner). The arguments map as follows (r67-messages as of 2026-10-16):

| `crew::notify` | `messages::post` |
|---|---|
| `to`: `None`, `Some(Who::Player)` | `To::All` (the player is the company's driver) |
| `to`: `Some(Who::Staff(id))` | `To::Driver { employee: id }` |
| `to`: `Some(Who::Guest(_))` | `To::All` (or leave it to the player's own launcher) |
| `priority`: `Low`, `Normal` / `High` | `Priority::Info` / `Priority::Urgent` |
| `template`, `args` | the same (`args.into_iter().map(...).collect()` into its `Vec<(String, String)>`) |
| `action` (`"phone"`, `"fleet"`, `"workshop"`, `"ops"`: a page) | `None` - its `Action` is service control's way out, not a page | The in-game "relief due" and "fuel low" cards are the strip's own (the game's clock
and the bus): they stay the plugin's toasts (`duty_strip.lua`'s `notify`).
