//! A day's rows copied to other days by themselves (testers, Beta 56: "a copy button to copy it
//! to another day. that may work better"): RosterFlow's selection bar's "Copy to…" - the buses'
//! blocks or the drivers' duties ticked - matched to the other day's trips as SAVE › Apply to
//! matches them (`matching`), the rest of that day's plan left as it is. Only the trips that
//! match go along; the rows' others are listed, and so are those a vehicle or a duty locked
//! there holds - unless that is the row itself, copied there before (with its lock): a second
//! copy goes over the first.
//!
//! A bus's block takes its matched trips off the buses that run them there, under its own name
//! (a bus of that name there is named anew), with its lock and its links. A duty becomes a duty
//! there of the pieces its matched trips are on those buses - cut where its trips begin and end,
//! the rest cut as before - with its note (its type, lock, signing and points) and its joins.

use super::super::studio::PieceRef;
use super::*;

/// The rows of a day's plan to copy: buses' blocks (their line and name), drivers' duties (their
/// key, `rosterflow::duty_key`) and spare duties (their number).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rows {
    pub blocks: Vec<(String, String)>,
    pub duties: Vec<PieceRef>,
    pub spares: Vec<u32>,
}

impl Rows {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.duties.is_empty() && self.spares.is_empty()
    }
}

/// The slot rows copied onto `date` go to: its weekday's (`every_week`), else a plan of the
/// date's own - begun as the plan it ran, saved and in the draft alike, so that only the rows
/// change there (Reset to pattern takes it back).
pub fn rows_slot(c: &mut Company, date: &str, every_week: bool) -> u8 {
    if every_week {
        return plan::weekday_of(date);
    }
    let s = slot_of(c, date);
    if s >= FIRST_DATE_SLOT && dates_of(c, s).len() == 1 {
        return s;
    }
    let f = free_slot(c);
    let p = &mut c.planning;
    let mut saved = Draft { layouts: std::mem::take(&mut p.layouts), joined: std::mem::take(&mut p.joined), sheet: std::mem::take(&mut p.flow.sheet) };
    let part = part_of(&saved.layouts, &saved.joined, &saved.sheet, s);
    put_part(&mut saved, f, part);
    (p.layouts, p.joined, p.flow.sheet) = (saved.layouts, saved.joined, saved.sheet);
    if let Some(d) = p.flow.draft.as_mut() {
        let part = part_of(&d.layouts, &d.joined, &d.sheet, s);
        put_part(d, f, part);
    }
    renumber_spares(c, f);
    set_date(c, date, f);
    let fs = &mut c.planning.flow.days.follows;
    fs.retain(|x| x.day != f);
    fs.push(Follow { day: f, pattern: s, vehicles: true, drivers: true, ..Default::default() });
    f
}

/// Where trip `r` of line `line` is on the day (`now`): its bus and place there.
fn placed(now: &[TourOfDay], line: &str, r: &TripRef) -> Option<(usize, usize)> {
    now.iter().enumerate().filter(|(_, u)| same(&u.line, line)).find_map(|(i, u)| u.trips.iter().position(|x| r.is(x)).map(|j| (i, j)))
}

/// Whether bus `i` is locked on `slot`.
fn bus_locked(c: &Company, slot: u8, now: &[TourOfDay], i: usize) -> bool {
    rf::block_locked(c, slot, &now[i].line, &now[i].tour)
}

/// Whether the duty trip `j` of bus `i` is in is locked on `slot`.
fn duty_locked(c: &Company, slot: u8, now: &[TourOfDay], (i, j): (usize, usize)) -> bool {
    network::duties_of(&now[i]).iter().position(|r| r.contains(&j)).is_some_and(|k| rf::piece_locked(c, slot, now, i, k))
}

/// The trips of the duty trip `j` of bus `i` is in, on the day `now` (`there` its duties).
fn duty_trips(there: &[st::Duty], now: &[TourOfDay], (i, j): (usize, usize)) -> Vec<TripRef> {
    let Some(d) = st::duty_of_trip(there, now, i, j) else { return Vec::new() };
    there[d].pieces.iter().flat_map(|p| network::duties_of(&now[p.tour]).get(p.duty).cloned().into_iter().flat_map(move |r| now[p.tour].trips[r].iter())).filter(|x| !x.origin.trim().is_empty()).map(TripRef::of).collect()
}

/// A line's blocks as the day runs them now (`now`), to change: its stack kept.
fn line_layout(c: &Company, slot: u8, line: &str, now: &[TourOfDay]) -> LineLayout {
    let mine: Vec<&TourOfDay> = now.iter().filter(|u| same(&u.line, line)).collect();
    let mut lay = layout_of(slot, line, &mine);
    lay.stack = c.planning.layouts.iter().find(|l| l.weekday == slot && same(&l.line, line)).map(|l| l.stack.clone()).unwrap_or_default();
    lay
}

fn save_layout(c: &mut Company, lay: LineLayout) {
    c.planning.layouts.retain(|l| !(l.weekday == lay.weekday && same(&l.line, &lay.line)));
    c.planning.layouts.push(lay);
}

/// A source trip that is a trip with passengers by itself (a trip cut on the way once).
fn counted(x: &PlannedTrip) -> bool {
    x.counts() && !x.is_second()
}

/// The rows `rows` of the plan of slot `from_slot` (as it runs on `src`) copied onto the day
/// `dst` into the plan of slot `to_slot`; the rest of that plan stays. See the module.
pub fn copy_rows(c: &mut Company, from_slot: u8, src: Day, to_slot: u8, dst: Day, rows: &Rows) -> Copied {
    let mut made = Copied { slot: to_slot, same_plan: from_slot == to_slot, ..Default::default() };
    if from_slot == to_slot || rows.is_empty() {
        return made;
    }
    let from = network::tours_of_slot(c, src.lines, src.date, from_slot);
    let srcs = sources(c, from_slot, &from, src.lines);
    let raw = network::timetable_tours(c, dst.lines);
    let found = matching(&srcs, &raw);
    let m = Match { from: &from, srcs: &srcs, found: &found, raw: &raw };
    for (line, name) in &rows.blocks {
        if let Some(bi) = from.iter().position(|t| same(&t.line, line) && same(&t.tour, name)) {
            copy_block(c, &m, from_slot, bi, to_slot, dst, &mut made);
        }
    }
    if !rows.duties.is_empty() {
        copy_duties(c, &m, from_slot, &rows.duties, to_slot, dst, &mut made);
    }
    // the spare duties of custom work, numbered anew there
    let mut id = c.planning.flow.sheet.spares.iter().map(|s| s.id).max().unwrap_or(0);
    let spares: Vec<Spare> = c.planning.flow.sheet.spares.iter().filter(|s| s.weekday == from_slot && rows.spares.contains(&s.id)).cloned().collect();
    for sp in spares {
        id += 1;
        c.planning.flow.sheet.spares.push(Spare { weekday: to_slot, id, ..sp });
    }
    // what of the day's duties no longer finds its pieces goes
    let tours = network::tours_of_slot(c, dst.lines, dst.date, to_slot);
    for j in c.planning.joined.iter_mut().filter(|j| j.weekday == to_slot) {
        j.pieces.retain(|p| st::piece_at(&tours, p).is_some());
    }
    c.planning.joined.retain(|j| j.weekday != to_slot || j.pieces.len() > 1);
    c.planning.flow.sheet.notes.retain(|n| n.weekday != to_slot || st::piece_at(&tours, &n.key).is_some());
    made.missing.sort_by(|a, b| (&a.line, a.dep, &a.block).cmp(&(&b.line, b.dep, &b.block)));
    made
}

/// The source day and how its trips match the day's: its tours, its trips, the day's trip each
/// is (`matching`) and the day's timetable.
struct Match<'a> {
    from: &'a [TourOfDay],
    srcs: &'a [Src],
    found: &'a [Option<(usize, usize)>],
    raw: &'a [TourOfDay],
}

impl Match<'_> {
    fn trip(&self, id: (usize, usize)) -> &PlannedTrip {
        &self.raw[id.0].trips[id.1]
    }

    fn of(&self, id: (usize, usize)) -> TripRef {
        TripRef::of(self.trip(id))
    }
}

/// Bus `bi` of the source copied (`rows`' module).
fn copy_block(c: &mut Company, m: &Match, from_slot: u8, bi: usize, to_slot: u8, dst: Day, made: &mut Copied) {
    let t = &m.from[bi];
    let line = t.line.clone();
    let now = network::tours_of_slot(c, dst.lines, dst.date, to_slot);
    // its trips matched, those a locked vehicle or duty there holds left out
    let mut got: Vec<(usize, usize)> = Vec::new();
    for (s, f) in m.srcs.iter().zip(m.found) {
        let Some((_, k)) = s.at.filter(|a| a.0 == bi) else { continue };
        let x = &t.trips[k];
        made.total += u32::from(counted(x));
        // (a locked bus there of this one's name is this bus, copied there before: copied over)
        let held = f.and_then(|id| placed(&now, &line, &m.of(id))).is_some_and(|at| (bus_locked(c, to_slot, &now, at.0) && !same(&now[at.0].tour, &t.tour)) || duty_locked(c, to_slot, &now, at));
        match f {
            Some(id) if !held => got.push(*id),
            _ if counted(x) => {
                made.locked += u32::from(held);
                made.missing.push(missed(t, x, &t.tour));
            }
            _ => {}
        }
    }
    got.sort_by_key(|id| m.trip(*id).dep);
    // (a trip that runs while the bus still runs the one before - its running times differ that
    // day - stays where it is)
    let mut kept: Vec<(usize, usize)> = Vec::new();
    let mut free_at = i32::MIN;
    for id in got {
        let x = m.trip(id);
        if x.dep < free_at {
            if x.counts() {
                made.missing.push(missed(&m.raw[id.0], x, &t.tour));
            }
            continue;
        }
        free_at = x.arr;
        kept.push(id);
    }
    if !kept.iter().any(|id| m.trip(*id).counts()) {
        made.missing.extend(kept.iter().filter(|id| m.trip(**id).counts()).map(|id| missed(&m.raw[id.0], m.trip(*id), &t.tour)));
        return;
    }
    made.matched += kept.iter().filter(|id| m.trip(**id).counts()).count() as u32;
    let refs: Vec<TripRef> = kept.iter().map(|id| m.of(*id)).collect();
    // the trips off the buses (and the stack) that run them there
    let mut lay = line_layout(c, to_slot, &line, &now);
    for b in lay.blocks.iter_mut() {
        b.trips.retain(|r| !refs.contains(r));
        if let Some(cs) = b.cuts.as_mut() {
            cs.retain(|r| !refs.contains(r));
        }
        b.reliefs.retain(|x| !refs.contains(&x.trip));
    }
    lay.stack.retain(|r| !refs.contains(r));
    lay.blocks.retain(|b| !b.trips.is_empty());
    // (a bus of its name there is named anew - one locked keeps its name, the copy takes another)
    let names: Vec<String> = lay.blocks.iter().map(|b| b.tour.trim().to_string()).collect();
    let mut name = t.tour.trim().to_string();
    if lay.blocks.iter().any(|b| same(&b.tour, &name)) {
        let fresh = fresh_name(&names);
        if rf::block_locked(c, to_slot, &line, &name) {
            name = fresh;
        } else {
            rename_block(c, &mut lay, to_slot, &line, &name, &fresh);
        }
    }
    lay.blocks.push(BlockLayout { tour: name.clone(), trips: refs.clone(), cuts: None, reliefs: Vec::new() });
    save_layout(c, lay);
    let lock = rf::block_locked(c, from_slot, &line, &t.tour);
    rf::set_block_lock(c, to_slot, &line, &name, lock);
    // its links, where all their trips came along
    let sheet = &mut c.planning.flow.sheet;
    sheet.links.retain(|l| !(l.weekday == to_slot && same(&l.line, &line) && l.trips.iter().any(|r| refs.contains(r))));
    let links: Vec<Link> = sheet
        .links
        .iter()
        .filter(|l| l.weekday == from_slot && same(&l.line, &line))
        .filter_map(|l| {
            let trips: Vec<TripRef> = l.trips.iter().filter_map(|r| m.srcs.iter().zip(m.found).find(|(s, _)| same(&s.line, &l.line) && r.is(&s.x)).and_then(|(_, f)| f.map(|id| m.of(id)))).collect();
            (trips.len() >= 2 && trips.len() == l.trips.len() && trips.iter().all(|r| refs.contains(r))).then(|| Link { weekday: to_slot, line: line.clone(), trips })
        })
        .collect();
    sheet.links.extend(links);
}

/// Bus `old` of `lay` named `new` there, its lock, joins and notes with it.
fn rename_block(c: &mut Company, lay: &mut LineLayout, slot: u8, line: &str, old: &str, new: &str) {
    for b in lay.blocks.iter_mut().filter(|b| same(&b.tour, old)) {
        b.tour = new.to_string();
    }
    let p = &mut c.planning;
    let mine = |l: &str, t: &str| same(l, line) && same(t, old);
    for x in p.joined.iter_mut().filter(|j| j.weekday == slot).flat_map(|j| j.pieces.iter_mut()).filter(|x| mine(&x.line, &x.tour)) {
        x.tour = new.to_string();
    }
    for n in p.flow.sheet.notes.iter_mut().filter(|n| n.weekday == slot && mine(&n.key.line, &n.key.tour)) {
        n.key.tour = new.to_string();
    }
    for l in p.flow.sheet.locks.iter_mut().filter(|l| l.weekday == slot && mine(&l.line, &l.tour)) {
        l.tour = new.to_string();
    }
}

/// The source's duties `keys` copied (`rows`' module).
fn copy_duties(c: &mut Company, m: &Match, from_slot: u8, keys: &[PieceRef], to_slot: u8, dst: Day, made: &mut Copied) {
    let duties = st::duties(&c.planning.joined, from_slot, m.from);
    let chosen: Vec<usize> = (0..duties.len()).filter(|&d| rf::duty_key(m.from, &duties[d]).is_some_and(|k| keys.iter().any(|x| x.is(&k)))).collect();
    if chosen.is_empty() {
        return;
    }
    let now = network::tours_of_slot(c, dst.lines, dst.date, to_slot);
    // (the source's trips of each chosen duty, as the day has them)
    let mut wanted: HashMap<usize, HashSet<TripRef>> = HashMap::new();
    for (s, f) in m.srcs.iter().zip(m.found) {
        let (Some((bi, k)), Some(id)) = (s.at, f) else { continue };
        let Some(pc) = network::duties_of(&m.from[bi]).iter().position(|r| r.contains(&k)) else { continue };
        if let Some(d) = st::duty_of(&duties, bi, pc).filter(|d| chosen.contains(d)) {
            wanted.entry(d).or_default().insert(m.of(*id));
        }
    }
    let there = st::duties(&c.planning.joined, to_slot, &now);
    // each of the day's trips a chosen duty's trip is: that duty - on a bus there, not locked (a
    // locked duty there of nothing but its trips is this duty, copied there before: copied over)
    let mut label: HashMap<TripRef, usize> = HashMap::new();
    for (s, f) in m.srcs.iter().zip(m.found) {
        let Some((bi, k)) = s.at else { continue };
        let Some(pc) = network::duties_of(&m.from[bi]).iter().position(|r| r.contains(&k)) else { continue };
        let Some(d) = st::duty_of(&duties, bi, pc).filter(|d| chosen.contains(d)) else { continue };
        let x = &m.from[bi].trips[k];
        made.total += u32::from(counted(x));
        let at = f.and_then(|id| placed(&now, &s.line, &m.of(id)));
        let held = at.is_some_and(|at| bus_locked(c, to_slot, &now, at.0) || (duty_locked(c, to_slot, &now, at) && !wanted.get(&d).is_some_and(|w| duty_trips(&there, &now, at).iter().all(|r| w.contains(r)))));
        match f.filter(|_| at.is_some() && !held) {
            Some(id) => {
                made.matched += u32::from(counted(x));
                label.insert(m.of(id), d);
            }
            None if counted(x) => {
                made.locked += u32::from(held);
                made.missing.push(missed(&m.from[bi], x, &m.from[bi].tour));
            }
            None => {}
        }
    }
    if label.is_empty() {
        return;
    }
    let lab = |x: &PlannedTrip| if x.origin.trim().is_empty() { None } else { label.get(&TripRef::of(x)).copied() };
    // the cuts: a piece for each run of one chosen duty's trips on a bus, elsewhere as they were
    let mut lines: Vec<String> = Vec::new();
    let mut starts: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, t) in now.iter().enumerate() {
        let labs: Vec<Option<usize>> = t.trips.iter().map(lab).collect();
        if labs.iter().all(Option::is_none) {
            continue;
        }
        let old: HashSet<usize> = network::duties_of(t).iter().map(|r| r.start).collect();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for k in 0..t.trips.len() {
            if k == 0 || labs[k] != labs[k - 1] || (labs[k].is_none() && old.contains(&k)) {
                runs.push((k, k + 1));
            } else if let Some(r) = runs.last_mut() {
                r.1 = k + 1;
            }
        }
        // (a run without a trip with passengers - a depot run - goes with the one beside it)
        let mut k = 0;
        while k < runs.len() && runs.len() > 1 {
            if t.trips[runs[k].0..runs[k].1].iter().any(PlannedTrip::counts) {
                k += 1;
            } else if k + 1 < runs.len() {
                runs[k + 1].0 = runs[k].0;
                runs.remove(k);
            } else {
                runs[k - 1].1 = runs[k].1;
                runs.remove(k);
            }
        }
        starts.push((i, runs.iter().skip(1).map(|r| r.0).collect()));
        if !lines.iter().any(|l| same(l, &t.line)) {
            lines.push(t.line.clone());
        }
    }
    for line in &lines {
        let mut lay = line_layout(c, to_slot, line, &now);
        for (i, at) in starts.iter().filter(|(i, _)| same(&now[*i].line, line)) {
            if let Some(b) = lay.blocks.iter_mut().find(|b| same(&b.tour, &now[*i].tour)) {
                st::set_cuts_of(b, &now[*i], at);
            }
        }
        save_layout(c, lay);
    }
    // the duties: each one's pieces there joined, its note on the first
    let tours = network::tours_of_slot(c, dst.lines, dst.date, to_slot);
    let mut groups: Vec<(usize, Vec<PieceRef>)> = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        for (k, r) in network::duties_of(t).iter().enumerate() {
            let Some(d) = t.trips[r.clone()].iter().find_map(lab) else { continue };
            let Some(p) = st::piece_ref(&tours, i, k) else { continue };
            match groups.iter_mut().find(|g| g.0 == d) {
                Some(g) => g.1.push(p),
                None => groups.push((d, vec![p])),
            }
        }
    }
    let ours: Vec<PieceRef> = groups.iter().flat_map(|g| g.1.iter().cloned()).collect();
    let p = &mut c.planning;
    for j in p.joined.iter_mut().filter(|j| j.weekday == to_slot) {
        j.pieces.retain(|x| !ours.iter().any(|o| o.is(x)));
    }
    p.joined.retain(|j| j.weekday != to_slot || j.pieces.len() > 1);
    p.flow.sheet.notes.retain(|n| !(n.weekday == to_slot && ours.iter().any(|o| o.is(&n.key))));
    for (d, g) in groups.iter_mut() {
        g.sort_by_key(|x| x.first.dep);
        if g.len() > 1 {
            p.joined.push(JoinedDuty { weekday: to_slot, pieces: g.clone() });
        }
        let key = rf::duty_key(m.from, &duties[*d]);
        let note = key.and_then(|k| p.flow.sheet.notes.iter().find(|n| n.weekday == from_slot && n.key.is(&k)).cloned());
        if let Some(n) = note {
            p.flow.sheet.notes.push(DutyNote { weekday: to_slot, key: g[0].clone(), ..n });
        }
    }
}

#[cfg(test)]
mod tests;
