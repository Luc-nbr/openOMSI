//! Where a duty signs on and off and takes its breaks (testers, Beta 56: "Would be nice to
//! specify the Starting Point for sign on & Sign Off and then break locations"): a relief point
//! that allows it (`ReliefPoint::sign`, `breaks`) chosen for the duty (`DutyNote::sign_on_at`,
//! `sign_off_at`, `breaks_at`), else the company's (`Prefs::sign_on_at`, `sign_off_at`), else -
//! as before - where the bus is. The driver travels between the point and the bus as the relief
//! points' travel has it (`travel_between`, a W, P or R box in the Gantt); the issues check the
//! points chosen, and a duty's points stay its own when Optimise makes the duties anew
//! (`keep_points`).

use super::*;

/// The way of the travel to the sign-on point and from the sign-off point as a duty's note keeps
/// it (`DutyNote::travel`, besides the gaps after its pieces by their number).
pub const ON_LEG: u32 = u32::MAX;
pub const OFF_LEG: u32 = u32::MAX - 1;

/// A break taken at a relief point chosen where no break may be taken there.
pub const I_BREAK_AT: &str = "A break at %{stop}, where no break may be taken";
pub const TEXTS: [&str; 1] = [I_BREAK_AT];
const TITLES: [&str; 1] = ["Breaks at a wrong place"];

/// The short title of a text of these.
pub fn title_of(text: &str) -> Option<&'static str> {
    TEXTS.iter().position(|t| *t == text).map(|k| TITLES[k])
}

/// Where a duty signs on and off and takes its breaks between pieces elsewhere, as worked out:
/// relief points by their key (`point_key`), None where the bus is.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Points {
    pub on: Option<String>,
    pub off: Option<String>,
    /// The breaks after piece `n` taken at a point chosen: (`n`, the point).
    pub breaks: Vec<(usize, String)>,
}

impl DutyNote {
    /// It chose where its duty signs on or off or takes a break.
    pub fn has_points(&self) -> bool {
        self.sign_on_at.is_some() || self.sign_off_at.is_some() || !self.breaks_at.is_empty()
    }
}

/// Where the duty of note `n` signs on (`off`: off): its own choice, else the company's; None
/// where the bus is (an empty choice says so too, over the company's).
pub fn sign_point(p: &Prefs, n: &DutyNote, off: bool) -> Option<String> {
    let own = if off { &n.sign_off_at } else { &n.sign_on_at };
    let company = if off { &p.sign_off_at } else { &p.sign_on_at };
    let at = own.as_ref().or(company.as_ref())?.trim();
    (!at.is_empty()).then(|| at.to_string())
}

/// Where the duty of note `n` takes the break after its piece `w`, when it is not where the bus
/// is.
pub fn break_point(n: &DutyNote, w: usize) -> Option<String> {
    n.breaks_at.iter().find(|(k, at)| *k as usize == w && !at.trim().is_empty()).map(|x| x.1.trim().to_string())
}

/// The travel from sign-on point `at` to where the bus is taken over (`bus`), or from where it is
/// left to sign-off point `at` (`off`): its way and minutes, None where there is none.
pub(super) fn leg(c: &Company, tours: &[TourOfDay], n: &DutyNote, at: &str, bus: &str, off: bool) -> Option<(Travel, i32)> {
    let set = n.travel.iter().find(|(k, _)| *k == if off { OFF_LEG } else { ON_LEG }).map(|x| x.1);
    let (from, to) = if off { (bus, at) } else { (at, bus) };
    let (way, m) = travel_between(c, tours, from, to, set);
    (m > 0).then_some((way, m))
}

/// The gap from `a_to` (piece `a` left at `end_a`) to `b_from` (piece `b` taken at `start_b`)
/// with its break at relief point `at`: the travel there and back (`set` the way chosen for the
/// gap), around a split its sign-off and sign-on there, and the split's break.
#[allow(clippy::too_many_arguments)]
pub(super) fn break_at(c: &Company, tours: &[TourOfDay], at: &str, (end_a, a_to): (&str, i32), (start_b, b_from): (&str, i32), set: Option<Travel>, events: &mut Vec<Ev>, splits: &mut Vec<(i32, i32)>, no_rest: &mut Vec<(i32, i32)>) {
    let p = &c.planning.flow.prefs;
    let (there, m1) = travel_between(c, tours, end_a, at, set);
    let (back, m2) = travel_between(c, tours, at, start_b, set);
    let (mut from, mut to) = (a_to, b_from);
    if m1 > 0 {
        events.push(Ev { kind: EvKind::Travel(there), from, to: from + m1 });
        from += m1;
    }
    if m2 > 0 {
        events.push(Ev { kind: EvKind::Travel(back), from: to - m2, to });
        to -= m2;
    }
    if b_from - a_to >= p.split_min {
        let (so, son) = (p.sign_off_split, p.sign_on_split);
        events.push(Ev { kind: EvKind::SignOff, from, to: from + so });
        events.push(Ev { kind: EvKind::SignOn, from: to - son, to });
        if to - son > from + so {
            splits.push((from + so, to - son));
        }
    }
    // (a point that allows no break: no break counts there - and the issue says so)
    if !p.points.is_empty() && uses_at(c, tours, at).is_some_and(|u| !u.breaks) {
        no_rest.push((a_to, b_from));
    }
}

/// The points duty `s` chose (or the company chose for it) that the relief points do not allow:
/// a sign-on or sign-off where no duty may begin or end, a break where none may be taken - and
/// one that is no relief point any more (a stop taken out, a terminus no longer). Each issue's
/// words and the stop.
pub(super) fn pick_issues(c: &Company, tours: &[TourOfDay], s: &Shaped) -> Vec<(&'static str, String)> {
    let may = |at: &str, f: fn(&Uses) -> bool| uses_at(c, tours, at).is_some_and(|u| f(&u));
    let mut out = Vec::new();
    if s.spans.is_empty() {
        return out;
    }
    if let Some(at) = s.points.on.as_ref().filter(|at| !may(at, |u| u.sign)) {
        out.push((I_SIGN_ON, at.clone()));
    }
    if let Some(at) = s.points.off.as_ref().filter(|at| !may(at, |u| u.sign)) {
        out.push((I_SIGN_OFF, at.clone()));
    }
    for (_, at) in s.points.breaks.iter().filter(|(_, at)| !may(at, |u| u.breaks)) {
        out.push((I_BREAK_AT, at.clone()));
    }
    out
}

/// Optimise made the duties of `date` anew (`lines` its timetable; `before` the company as it
/// was, `c` with what Optimise made): the points a duty chose stay its own - on the duty that
/// now holds the trip it began with, where that one chose none of its own.
pub fn keep_points(before: &Company, c: &mut Company, lines: &[LineInfo], date: &str) {
    let wd = super::super::plandays::slot_of(c, date);
    let picked: Vec<DutyNote> = before.planning.flow.sheet.notes.iter().filter(|n| n.weekday == wd && n.has_points()).cloned().collect();
    if picked.is_empty() {
        return;
    }
    let tours = network::tours_of_day(c, lines, date);
    let duties = st::duties(&c.planning.joined, wd, &tours);
    let holds = |d: &Duty, key: &PieceRef| {
        d.pieces.iter().any(|pc| {
            let t = &tours[pc.tour];
            same(&t.line, &key.line) && network::duties_of(t).get(pc.duty).is_some_and(|r| t.trips[r.clone()].iter().any(|x| key.first.is(x)))
        })
    };
    for n in picked {
        let Some(d) = duties.iter().find(|d| holds(d, &n.key)) else { continue };
        let Some(key) = duty_key(&tours, d) else { continue };
        if key.is(&n.key) || note(c, wd, &key).is_some_and(DutyNote::has_points) {
            continue;
        }
        let gaps = d.pieces.len().saturating_sub(1);
        let old = note_mut(c, wd, &n.key);
        (old.sign_on_at, old.sign_off_at, old.breaks_at) = (None, None, Vec::new());
        let new = note_mut(c, wd, &key);
        new.sign_on_at = n.sign_on_at.clone();
        new.sign_off_at = n.sign_off_at.clone();
        new.breaks_at = n.breaks_at.iter().filter(|(k, _)| (*k as usize) < gaps).cloned().collect();
    }
    tidy_notes(c);
}

#[cfg(test)]
mod tests;
