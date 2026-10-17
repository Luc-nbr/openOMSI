# Messages: the control room and its drivers

A light CAD/AVL for the bus company mode, as real operators have it (INIT, IVU): control sends a
driver a message - you are running late, short-turn at the next trip, hold for a connection -,
it arrives in the game's overlay, the navigator's inbox and the phone, and the driver accepts,
declines or replies; the driver tells control with a quick button (a delay in traffic, a
breakdown, ...). Luc, 2026-10-16: *"a message system, so that when a manager sees that you are
running late or whatever, they can send you a message, and that then arrives in the app, in the
overlay in openOMSI."*

It is the base for the next overlay features: anything that tells the driver something posts it
here (`post`, below).

## The model (`crates/omsi-launcher-core/src/company/messages.rs`)

Every company keeps one queue, saved with it (`Company::messages`, `#[serde(default)]`: a file
of before has none; the save's `VERSION` is unchanged). The history is bounded (`KEPT` = 300:
the oldest go first, an instruction still waiting for its answer last).

| | |
|---|---|
| `id`, `at` | its number in the company, and the company's clock when it was sent |
| `from` | `Control` (the player at the control desk), `Dispatcher` (the AI dispatcher, and the rest of the company through `post`), `Manager { name }` (a manager of "Play together"), `Driver { line, tour, name }` |
| `to` | `Duty { line, tour }`, `Driver { employee }`, `Line { line }`, `All`, `Control` |
| `priority` | `Info`, `Instruction`, `Urgent` (an action makes it an instruction at least) |
| `text`, `args`, `free` | a canned English text with `%{name}` places (translated where it is shown), or the sender's own words (`free`: one clean line, 240 characters at most) |
| `action` | an instruction's action, below |
| `state` | `Sent` → `Delivered` (on the driver's screen) → `Read` → `Accepted` / `Declined` - it only moves on, and an answer stays |
| `reply`, `quick`, `re` | the driver's canned reply (`Canned`), the quick button a driver's message was (`Quick`), the message it answers |
| `applied`, `outcome`, `done` | an accepted action carried out (and why not, where it could not be); control has seen to a driver's message |

The dispatcher's memory (`said`: what it said and when) and the duty the player drives in the
game now (`driving`) are kept with the queue, so a restart of the launcher does not tell the same
thing twice.

### The actions

| `What` | carried out by the company (`carry_out`) | carried out in the game (`lua/messages.lua`) |
|---|---|---|
| `ShortTurn { part, saved }` | `control::apply` with `Act::ShortTurn` - as the Operations page's way out | the duty turns back at the stop `part` from the trip's end: the next trip from the same stop (`duty.start`), the IBIS typed, the trip reported `short_turn` |
| `OutOfService { part, saved }` | `Act::Express` | the trip's stops skipped to its terminus (`duty.skip_to`) |
| `Layover { saved }` | `Act::Layover` | (the departure stays: the driver leaves sooner) |
| `Swap { vehicle, from_bus }` | `Act::Swap`: the spare runs the rest of the block | the duty ends with the trip before (`duty.finish`): the bus goes back to the depot |
| `Hold { minutes, connection }` | told in the feed | a countdown at the next stop |
| `Relief { stop, at }` | told in the feed | the duty ends at the stop (`duty.finish`) |

The first four are service control's ways out (`control.rs`, `ops.rs`): accepted, they go into the
day's controls the delay model, the day's close, the feed, the fleet map and "Done today" read,
exactly as when the player clicks them on the Operations page. Declined, the feed says so and the
Operations page's list shows it.

## Who sends

* **The player as manager** - the Operations page (`company/ops.js`, `company/messages.js`): a
  "Message" button on every incident row (a late bus, a late driver, a breakdown, a driver's own
  message) and "Message every driver" on the list. The compose box: a canned message, the
  sender's own words, a priority, and for a late bus its ways out as instructions plus holds of 2,
  3 or 5 minutes (for a connection or the headway). The list under the incidents shows the day's
  messages with where each stands.
* **The AI dispatcher** (single player, Normal and Realistic - Casual's controller stays calm):
  - to the duty the player drives, from the plugin's status (`dispatch`): running late from the
    alert's minutes (`Clock::late_notice`, "Never" says nothing) - once a trip, again when it
    grew five minutes, urgent from three times the alert's minutes; a short-turn (or out of
    service, little of the trip left) to accept or decline when it is twice the alert's minutes
    late and the layover will not take the delay up - once a trip; "back on time" after being
    told; the spare bus on its way to his broken-down bus;
  - to an AI driver whose bus service control tells late (`control::watch` → `tell_late`), once
    a day per tour.

  Never two of its own messages to one driver within ten company minutes (`GAP`), never the same
  thing twice (`Messages::may_say`).
* **Multiplayer** ("Play together"): a player whose role has **Service** sends from the boss's
  Operations page (relayed: `ops_msg`, `ops_msg_ack` need `Perm::Service` in
  `together/relay.rs`), by his name (`From::Manager`). A player with **Drive** answers the
  messages to the duty *his* game drives, and tells control from it (`msg_reply`, `msg_tell` in
  `together/host.rs`: refused for another's duty or without Drive); his snapshot carries his
  duty's messages and his launcher hands them to his own game's plugin (`together/join.rs`).

**Who answers:** the player's duty in the game and a player's of "Play together" answer for
themselves (`human`). An AI driver answers at once (`ai_answers`): he reads an info, carries an
instruction out when it can be done, and declines it (`NOT_POSSIBLE`) when it cannot.

## How it reaches the driver

The bridge's files (`crates/omsihub-plugin/src/bridge.rs`), beside the others in the plugin's
`data` folder:

| file | from → to | what |
|---|---|---|
| `messages.json` | Omsi-Hub → plugin | the duty's inbox (`Inbox`): the newest 12 messages, in the player's language, with the quick buttons' and the replies' words |
| `replies.jsonl` | plugin → Omsi-Hub | one `Reply` a line: `delivered`, `read`, `accept`, `decline`, `reply` (a canned reply), `quick` (a quick button); each with its own `uid` (taken in once) |
| `status.json` | plugin → Omsi-Hub | how the duty stands (`Status`): the trip, its times, the delay, the stops passed |

A game without the file functions puts the replies into its log (`OMSIHUB-REPLY {...}`,
`replies_from_log`).

The launcher's link (`omsi-launcher-api`'s `company/messages.rs`, `tick`, every two seconds with
the company's pages): the duty Omsi-Hub gave the plugin of the game it starts
(`addon::duty_link`), while that game runs, is the one the player drives; its replies are taken
in, its status goes to the dispatcher, the AI drivers answer, what was accepted is carried out,
and the inbox goes back to the plugin and, beside the company, to the phone
(`companies/<id>.inbox.json`).

**In the game** (`crates/omsihub-plugin/lua/messages.lua`, a file of its own): every new message
comes as a card with a sound - soft for an info, two tones for an instruction, a louder alarm for
an urgent one (`msg_*.wav`, made by `install::sounds`) -, and the inbox panel opens by itself
while an instruction waits (F9 opens and closes it, F10 gives the panels the mouse): the
messages with Accept and Decline for an instruction, the canned replies ("Understood", "On my
way", ...) for one not read, and "Report to control" with the quick buttons.

**On the phone** (the companion's "Company" tab, `assets/companion/app.js`): while the player
drives a duty of the company, the same inbox with Accept, Decline, the replies and the quick
buttons - sent as the tab's orders (`remote::Order::Reply`, `remote::Order::Tell`), which the
launcher carries out.

## Driver to control

The quick buttons (`Quick`): *Delay: traffic*, *Breakdown*, *Accident*, *Aggressive passenger*,
*Road closed*, *Request relief*, *Request to talk*. Each is a message to control, told in the
feed and shown as a row of the Operations page's incident table until control sees to it
("Acknowledge": the driver is told "Control: understood, we are on it"). A breakdown or an
accident is also the day's incident for the driver's bus (`clock::Broken`, once): the Operations
page offers a spare bus, a rental bus or dropping its trips, as for the clock's own.

## The API

```rust
use omsi_launcher_lib::company::messages::{self, Action, Priority, To, What};

// a notification for the driver of line 5's tour 3 (canned text, translated where it is shown)
messages::post(&mut c, To::Duty { line: "Linie5".into(), tour: "3".into() }, Priority::Info,
    "Your bus is due for a wash at the depot", vec![], None);

// with an instruction the driver accepts or declines
let a = Action { line: "Linie5".into(), tour: "3".into(), dep: 14 * 60 + 5, what: What::Hold { minutes: 3, connection: true } };
messages::post(&mut c, To::Duty { line: "Linie5".into(), tour: "3".into() }, Priority::Instruction,
    messages::HOLD_CONNECTION, vec![("min".into(), "3".into())], Some(a));
```

`post(company, to, priority, template, args, action) -> id` sends from the company's control
(`From::Dispatcher`). The rest:

* `send(company, from, to, priority, text, args, free, action)` - from anyone, free text too;
* `answer(company, id, Answer, by)` - a driver's answer (`by`: his duty, which must be the
  message's);
* `carry_out(company, day_plan, run)`, `ai_answers(company, day_plan, run)` - what the launcher's
  link does with today's plan;
* `driver_says(company, line, tour, name, Quick, day_plan, run)`, `acknowledge(company, id,
  from, quiet)`;
* `dispatch(company, &Driving)`, `tell_late(company, line, tour, minutes)` - the AI dispatcher.

A new canned text goes into the translations (`crates/omsihub-locales/locales/`, all seven
languages); a text of a later feature can live in that feature's table. The places `min`,
`time`, `bus`, `stop` are written as they come.

## Changes to shared files

Kept small, for the sessions working beside this one:

* `crates/omsihub-plugin/lua/main.lua`: `require("messages")` with its hooks, one line each in
  `duty` (a short-turn takes the same duty again: no new duty reported), `stop_arrive`,
  `ui_click`, `key` and the timers, the files listed at the top, four functions in `NEEDS`, the
  version 1.6.0;
* `crates/omsihub-plugin/src/install.rs`: `messages.lua` in `FILES`, its texts at the end of
  `TEXTS`, the sounds, `VERSION` 1.6.0; `compat.rs`: three features (`messages`,
  `message_actions`, `message_sound`); `tests/plugin.rs`: the needs compared without repeats;
* `control.rs`: one line in `watch` (`tell_late`); `remote.rs`: two orders; `together`: two
  requests, the snapshot's `messages`; `store.rs`: the phone's inbox file deleted with the
  company;
* `omsi-launcher-api`: `ops.rs` (each row's `to` and `instr`, the driver's rows, the compose box
  and the list, `ops_msg` / `ops_msg_ack`), `relay.rs` (their permission), `addon.rs`
  (`Assigned`'s company duty, `duty_link`, `game_plugin_dir`), `company.rs` (the session's link
  and the relayed sender), `together/join.rs` and `host.rs` (the player's own plugin, the
  sender's name); `ops.js` (three imports and uses).
