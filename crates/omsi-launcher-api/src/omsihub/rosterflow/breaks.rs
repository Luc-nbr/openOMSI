//! A duty's breaks and driving periods as RosterFlow's page shows them (testers, 2026-10-14:
//! "when you define for it to show you periods between a break it does not tell me so its a bit
//! of guesswork"): the labels in the Drivers Gantt's gaps, the band of driving periods under a
//! duty, their hover tips and the duty panel's lists - all from the core's `BreakPlan`, the one
//! the issues are counted from. And the rules as the Preferences change them: what breaks each
//! rule while the window is open (`prefs_check`), and after a save what the changed rules break,
//! with Optimise offered (`offer`).

use super::super::company::words::{tr, trf};
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, BreakKind, BreakPlan, DutyBreak, MealDue, Prefs, Scope, Shaped, Stretch};
use serde_json::{json, Value};

/// The rest that ends a driving period: a break of `break_min`, and a gap is a break from
/// `break_from` on.
fn rest(p: &Prefs) -> i32 {
    p.break_min.max(p.break_from)
}

/// A break's kind as the page names it, and its words ("Break 0:35", "Meal 0:45", "Split 3:10").
fn label(b: &DutyBreak) -> (&'static str, String) {
    let t = rf::hm(b.to - b.from);
    match b.kind {
        BreakKind::Split => ("split", trf("Split %{t}", &[("t", &t)])),
        BreakKind::Meal => ("meal", trf("Meal %{t}", &[("t", &t)])),
        BreakKind::Break => ("break", trf("Break %{t}", &[("t", &t)])),
        BreakKind::Short => ("short", trf("Break %{t}", &[("t", &t)])),
    }
}

/// The meal rule broken, in words: by when the meal break was due, and when it came if late.
fn meal_words(p: &Prefs, m: &MealDue) -> String {
    let (at, after, min) = (rf::clock(m.from), rf::hm(p.meal_after), rf::hm(p.meal.value));
    match m.late {
        Some(t) => trf("The meal break comes too late: %{time} of work before it, at most %{after}.", &[("time", &rf::hm(t)), ("after", &after)]),
        None => trf("No meal break by %{at}: a duty working over %{after} needs one of %{min} before then.", &[("at", &at), ("after", &after), ("min", &min)]),
    }
}

/// What a break counts for, in words (its hover card).
fn break_tip(p: &Prefs, plan: &BreakPlan, b: &DutyBreak) -> String {
    let mut lines = vec![format!("{}  ·  {}–{}", label(b).1, rf::clock(b.from), rf::clock(b.to))];
    if b.kind == BreakKind::Split {
        lines.push(tr("A split: unpaid, the driver signs off and on again."));
    }
    if p.meal.on {
        let min = rf::hm(p.meal.value);
        let late = plan.meal_due.is_some_and(|m| m.late.is_some() && m.to == b.from);
        lines.push(if late {
            plan.meal_due.map(|m| meal_words(p, &m)).unwrap_or_default()
        } else if b.meal {
            trf("Long enough for the meal break (at least %{min}).", &[("min", &min)])
        } else {
            trf("Too short for the meal break (at least %{min}).", &[("min", &min)])
        });
    }
    if p.continuous.on {
        let min = rf::hm(rest(p));
        lines.push(if b.resets { trf("Ends a driving period (a break of at least %{min}).", &[("min", &min)]) } else { trf("Too short to end a driving period (at least %{min}).", &[("min", &min)]) });
    }
    lines.join("\n")
}

/// A driving period in words: how long, from when to when, the continuous rule's verdict and
/// the meal rule's where it is broken in it.
fn period_tip(p: &Prefs, plan: &BreakPlan, st: &Stretch) -> String {
    let mut lines = vec![trf("Driving period %{t}  ·  %{from}–%{to}", &[("t", &rf::hm(st.to - st.from)), ("from", &rf::clock(st.from)), ("to", &rf::clock(st.to))])];
    if p.continuous.on {
        let args = [("max", rf::hm(p.continuous.value)), ("min", rf::hm(rest(p)))];
        let args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
        lines.push(if st.over { trf("Over the Continuous Break limit: at most %{max} before a break of %{min}.", &args) } else { trf("Within the Continuous Break limit: at most %{max} before a break of %{min}.", &args) });
    } else {
        lines.push(tr("The Continuous Break rule is off."));
    }
    if let Some(m) = plan.meal_due.filter(|m| m.from < st.to && st.from < m.to.max(m.from + 1)) {
        lines.push(meal_words(p, &m));
    }
    lines.join("\n")
}

/// The breaks, the driving periods and the meal rule of duty `s` as the Drivers Gantt draws them.
pub(crate) fn duty_json(p: &Prefs, s: &Shaped) -> Value {
    let plan = &s.plan;
    let breaks: Vec<Value> = plan
        .breaks
        .iter()
        .map(|b| {
            let (kind, text) = label(b);
            json!({ "from": b.from, "to": b.to, "kind": kind, "label": text, "short": rf::hm(b.to - b.from), "counts": b.resets || b.meal, "tip": break_tip(p, plan, b) })
        })
        .collect();
    let periods: Vec<Value> = plan.stretches.iter().map(|st| json!({ "from": st.from, "to": st.to, "text": rf::hm(st.to - st.from), "over": st.over, "tip": period_tip(p, plan, st) })).collect();
    let meal = plan.meal_due.map(|m| json!({ "from": m.from, "to": m.to, "late": m.late.is_some(), "tip": meal_words(p, &m) }));
    json!({ "breaks": breaks, "periods": periods, "meal": meal, "period_hard": p.continuous.hard, "meal_hard": p.meal.hard })
}

/// The duty panel's lists: its breaks and its driving periods, each in words with its tip, and
/// the meal rule where it is broken.
pub(crate) fn panel_json(p: &Prefs, s: &Shaped) -> Value {
    let plan = &s.plan;
    let breaks: Vec<Value> = plan
        .breaks
        .iter()
        .map(|b| {
            let (kind, text) = label(b);
            json!({ "words": format!("{text}  ·  {}–{}", rf::clock(b.from), rf::clock(b.to)), "kind": kind, "counts": b.resets || b.meal, "tip": break_tip(p, plan, b) })
        })
        .collect();
    let periods: Vec<Value> = plan.stretches.iter().map(|st| json!({ "words": format!("{}  ·  {}–{}", rf::hm(st.to - st.from), rf::clock(st.from), rf::clock(st.to)), "over": st.over, "tip": period_tip(p, plan, st) })).collect();
    json!({
        "breaks": breaks,
        "periods": periods,
        "meal": plan.meal_due.map(|m| meal_words(p, &m)),
        "meal_hard": p.meal.hard, "period_hard": p.continuous.hard,
        "none": trf("No break: every gap is shorter than %{t}.", &[("t", &rf::hm(p.break_from))]),
    })
}

// --- the rules, as the Preferences change them ------------------------------------------------------------

/// What a rule's count is of.
#[derive(Clone, Copy, PartialEq)]
enum Of {
    Duties,
    Vehicles,
    Day,
}

/// The Preferences' items with a rule the day can break, the issues of each and what they are of.
const RULES: [(&str, &[&str], Of); 10] = [
    ("breaks", &[rf::I_MEAL, rf::I_MEAL_LATE], Of::Duties),
    ("continuous", &[rf::I_CONTINUOUS], Of::Duties),
    ("limits", &[rf::I_SPREAD], Of::Duties),
    // (Time Limitations' least length of a duty, counted apart from the spread at most)
    ("length", &[rf::I_SHORT_DUTY], Of::Duties),
    ("pieces", &[rf::I_SHORT_PIECE], Of::Duties),
    ("changeover", &[rf::I_CHANGEOVERS], Of::Duties),
    ("types", &[rf::I_TYPE], Of::Duties),
    ("travel", &[rf::I_TRAVEL], Of::Duties),
    ("layover", &[rf::I_LAYOVER], Of::Vehicles),
    ("depot", &[rf::I_DEPOT], Of::Day),
];

/// The Preferences' name of an item (the page's `NAME`).
fn rule_name(item: &str) -> &'static str {
    match item {
        "breaks" => "Duty Breaks",
        "continuous" => "Continuous Break",
        "limits" => "Time Limitations",
        "length" => "Duty length at least",
        "pieces" => "Piece Length",
        "changeover" => "Driver Changeover Vehicle",
        "types" => "Duty Types",
        "travel" => "Duty Travel",
        "layover" => "Layover",
        _ => "Depot Setup",
    }
}

/// The fields of the preferences each item's rule hangs on.
const FIELDS: [(&str, &[&str]); 10] = [
    ("breaks", &["meal", "meal_after", "break_from"]),
    ("continuous", &["continuous", "break_min", "break_from", "layover_breaks"]),
    ("limits", &["max_spread"]),
    ("length", &["min_duty"]),
    ("pieces", &["min_piece"]),
    ("changeover", &["changeovers"]),
    ("types", &["duty_types", "split_min"]),
    ("travel", &["travel", "walk", "public", "car"]),
    ("layover", &["layover"]),
    ("depot", &["depot_capacity"]),
];

/// How many duties (or buses, or the day) break each rule on `board`, and (as "split") how many
/// duties have a split.
pub(crate) fn rule_counts(board: &Board) -> Vec<(&'static str, usize)> {
    let mut out: Vec<(&'static str, usize)> = RULES
        .iter()
        .map(|(item, texts, _)| {
            let mut rows: Vec<(Scope, Option<usize>)> = board.issues.iter().filter(|i| texts.contains(&i.text)).map(|i| (i.scope, i.row)).collect();
            rows.sort_by_key(|r| (r.0 as u8, r.1));
            rows.dedup();
            (*item, rows.len())
        })
        .collect();
    out.push(("split", board.shaped.iter().filter(|s| s.split()).count()));
    out
}

fn of(item: &str) -> Of {
    RULES.iter().find(|r| r.0 == item).map(|r| r.2).unwrap_or(Of::Duties)
}

/// A rule's count in words, next to its fields ("4 duties break this rule now"), and what the
/// saved rules make of it where that differs.
fn count_words(item: &str, n: usize, saved: Option<usize>) -> String {
    let n_text = n.to_string();
    let now = match (item, of(item), n) {
        ("split", _, 0) => tr("No duty has a split now."),
        ("split", _, _) => trf("%{n} duties have a split now.", &[("n", &n_text)]),
        (_, _, 0) => tr("Nothing breaks this rule now."),
        (_, Of::Duties, _) => trf("%{n} duties break this rule now.", &[("n", &n_text)]),
        (_, Of::Vehicles, _) => trf("%{n} vehicles break this rule now.", &[("n", &n_text)]),
        (_, Of::Day, _) => tr("The day breaks this rule now."),
    };
    match saved.filter(|m| *m != n) {
        Some(m) => format!("{now} {}", trf("(%{n} with the saved rules)", &[("n", &m.to_string())])),
        None => now,
    }
}

/// The Preferences as edited and not saved yet (`edited`, the day made with them) against the day
/// as it is (`now`): what breaks each rule, in words next to its fields.
pub(crate) fn prefs_check(now: &Board, edited: &Board) -> Value {
    let saved = rule_counts(now);
    let counts: serde_json::Map<String, Value> = rule_counts(edited)
        .into_iter()
        .map(|(item, n)| {
            let was = saved.iter().find(|x| x.0 == item).map(|x| x.1);
            (item.to_string(), json!({ "n": n, "was": was, "text": count_words(item, n, was) }))
        })
        .collect();
    json!({ "counts": counts })
}

/// The items whose rules changed from `was` to `now`.
fn changed(was: &Prefs, now: &Prefs) -> Vec<&'static str> {
    let (w, n) = (serde_json::to_value(was).unwrap_or_default(), serde_json::to_value(now).unwrap_or_default());
    FIELDS.iter().filter(|(_, fs)| fs.iter().any(|f| w.get(*f) != n.get(*f))).map(|(item, _)| *item).collect()
}

/// After the Preferences were saved (`was` the rules before, `before` what broke each then): the
/// rules the day breaks now that changed or are broken more, said at once, with Optimise offered
/// to work to them (testers: "its not actually realising this immediately"). Null when there are
/// none.
pub(crate) fn offer(was: &Prefs, now: &Prefs, before: Option<&[(&'static str, usize)]>, board: &Board) -> Value {
    let changed = changed(was, now);
    let broken: Vec<(&str, usize)> = rule_counts(board)
        .into_iter()
        .filter(|(item, n)| *item != "split" && *n > 0)
        .filter(|(item, n)| changed.contains(item) || before.is_some_and(|b| b.iter().any(|x| x.0 == *item && x.1 < *n)))
        .collect();
    if broken.is_empty() {
        return Value::Null;
    }
    let list: Vec<String> = broken
        .iter()
        .map(|(item, n)| {
            let what = match of(item) {
                Of::Duties => trf("%{n} duties", &[("n", &n.to_string())]),
                Of::Vehicles => trf("%{n} vehicles", &[("n", &n.to_string())]),
                Of::Day => tr("the day"),
            };
            format!("{} ({what})", tr(rule_name(item)))
        })
        .collect();
    let groups: Vec<&str> = broken.iter().flat_map(|(item, _)| RULES.iter().find(|r| r.0 == *item).map(|r| r.1.to_vec()).unwrap_or_default()).collect();
    json!({
        "text": trf("Broken now by the changed rules: %{list}", &[("list", &list.join(", "))]),
        "button": tr("Optimise them"),
        "groups": groups,
    })
}
