# Omsi-Hub's overlays while driving

Since add-on phase 2 the overlays Omsi-Hub shows during a drive are the Omsi-Hub plugin's
(`crates/omsihub-plugin/lua/`), drawn through openOMSI's plugin UI (`omsi.ui`, version 3: see
[`docs/PLUGINS.md`](../PLUGINS.md), "On-screen panels"). Before phase 2 the game drew them
itself (`trip_report.rs`, `drive_watch.rs` and the duty board of `omsihub_game::nav_duty` in the
navigator); the plugin now draws them as those did.

| What | Where | Module |
| --- | --- | --- |
| The duty board | in the navigator, under its next-stop bar | `board.lua` |
| The company's parts of the board | under the board | `company.lua` |
| Service control's messages | notifications, top right | `company.lua`, `main.lua` |
| The trip's card | top centre, when the bus stands after a trip | `card.lua` |
| The drive watch (red lights, speed cameras) | notifications, top right | `watch.lua` |
| The company's panel (signing on, starting the duty, the switches) | right edge | `main.lua`, `signon.lua` |

## The duty board

The trip under way: its line plate, where it goes, and a chip with how it stands against the
timetable (on time green, late red, early blue; before the trip "leaves in", at the terminus
the break that is left). Under it "trip 2 of 3 · arrives 07:45", then the stop behind the bus
and the ones ahead on a rail, each with its planned time and, when the bus is off the
timetable, when it will be there, the terminus tagged. "and 2 more to …" counts the rest, and
an amber box says what comes next: the next trip's departure, its line and terminus, the
break before it and a change of line or tour. During the break the board looks ahead to the
next trip.

It is the navigator's board: Shift+N's second step and the handle at the right end of the
navigator's bar show and hide it, it opens by itself once on a duty, it takes the navigator's
width and scale, and the navigator's own list of stops makes way for it (so the stops are not
shown twice). It shows as many stops ahead as fit (four at most). An openOMSI without the
navigator's board (`omsi.ui.version` below 3) gets it as a panel of its own on the left.

The launcher's "duty board" setting (`nav_board`) switches it off altogether; it waits until
the driver signed on, where signing on is asked.

## The company's parts

Five parts under the board, each from what the company knows (the assignment the launcher
writes, `bridge::Assignment`: the duty's sheet, service control's decisions, the fares, the
line's contract) and from the duty as it runs. Each is switched on and off in the company's
panel: F10 gives the panels the mouse and the panel lists them under "On the duty board". The
choice is kept for the next drive; all are on until the driver chooses. On a drive of the
player's own (no company) there are none.

- **Punctuality coach** - standing at a stop too early: "Hold 1:30 here · leave at 07:52", and
  in red when leaving now would count as early (the contract's on-time window, else OMSI 2's
  two minutes). Running early: "ease off". Late: whether the layover at the terminus brings the
  next trip back on time, or how late it will still be (the least layover kept, as service
  control counts it). When the navigator has little room, the coach is the one part shown.
- **Duty strip** - the next break (or the one under way) with where, and where and when the
  duty ends: signing off, or the relief that takes the bus over.
- **Service control messages** - notifications, each once and five minutes before what it is
  about: today's decisions of the company's service control for the tour (a short-turn, a trip
  run out of service, a layover cut, another bus taking over), the breaks and the relief.
- **Fare box** - the passengers boarded on this trip and on the drive, what they bring the
  company at today's average per boarding (none on a line whose fares go to the authority),
  and the load aboard now.
- **Contract** - the line's contract with the authority (gross, net, hire), how the authority
  rates the company today (green, amber, red) and the on-time window it counts.

## The trip's card

Top centre when the bus stands after a trip (a trip that ends on the move waits for it to
stand). As the game's own card: "Trip completed" (or "Trip ended") with trip k of n, the line plate and where it
went, the times, the driving time and the distance; the punctuality in large figures with a
bar of early, on time and late; the worst and the average delay, the driving time and the
distance on tiles; jolts, collisions and passengers; the grade with its stars, score and
experience, its parts and the fines. A second tab lists every stop with when it was due, came
and left, and the ± in its colour. A line along its foot shows the 15 seconds it stays.

A click on it keeps it open (no F10 needed: the panel takes its clicks, `grab`); × or Enter
closes it. Enter with Ctrl, Alt or Shift held is left to the bus (Ctrl+Enter is the IBIS's).

## The drive watch

A red light crossed and a speed camera passed too fast come as notifications (red for a red
light, amber for a camera) with the fine; the speed in km/h or, for a player who reads
Britain's or America's units, in mph with the limit as the signs show it.

## Look

Omsi-Hub's dark sheet (`#141A26`) with its texts (`#E8EBF2`, `#C4CAD6`, `#959DB0`), the orange
accent `#F58620`, the yellow line plate, and punctuality in red, green and blue - the same by
day and at night (the sheets are nearly opaque, so the sky does not wash them out and the
night does not glare). Everything scales with the interface's size; the board with the
navigator's.
