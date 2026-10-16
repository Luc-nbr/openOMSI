//! RosterFlow's plan copied to the other days it works with (Trello #100, `co::plandays`): what
//! each day of the coming week runs - its weekday's plan, one of its own, a copy of another
//! day's and whether it is still as copied - for the day switcher and the toolbar's chip; the
//! "Apply to" window of SAVE (each day of the week with what a copy would match there); the save
//! that copies; "Reset to pattern" / "Refresh"; and the selection bar's "Copy to…" (Ctrl+C,
//! Ctrl+V) of the rows ticked (`plandays::copy_rows`).

use super::super::company::words::{day_label, tr, trf};
use super::super::company::{self, Session};
use super::{block_id, duty_id, edit, said, Rf, DAY_NAMES};
use omsi_launcher_lib::company::plandays::{self as pd, Copied, Day, Missed, What};
use omsi_launcher_lib::company::rosterflow as rf;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_launcher_lib::lines::DAY_GROUPS;
use omsi_launcher_lib::LineInfo;
use serde_json::{json, Value};

/// The days of the day switcher: the company's day and the next six.
pub(super) fn window(c: &Company) -> Vec<String> {
    (0..7).map(|k| co::dates::add(&c.date, k)).collect()
}

/// "FRI 16 OCT", as the day switcher says a day.
pub(super) fn short(date: &str) -> String {
    let wd = co::plan::weekday_of(date) as usize % 7;
    let rest: Vec<String> = day_label(date).split_whitespace().skip(1).take(2).map(str::to_uppercase).collect();
    format!("{} {}", tr(DAY_NAMES[wd]).to_uppercase(), rest.join(" "))
}

/// The day type of a weekday (`lines::DAY_GROUPS`: Mon - Fri, Saturday, Sunday).
pub(super) fn group_of(weekday: u8) -> usize {
    match weekday {
        0..=4 => 0,
        5 => 1,
        _ => 2,
    }
}

/// A slot's plan in words: a weekday's ("MON"), or that of the date that runs it.
fn slot_words(c: &Company, slot: u8) -> String {
    if slot < pd::FIRST_DATE_SLOT {
        tr(DAY_NAMES[slot as usize % 7]).to_uppercase()
    } else {
        pd::dates_of(c, slot).first().map(|d| short(d)).unwrap_or_default()
    }
}

/// The marks a day's plan carries (`tone`: dim, sky, warn), shortest first.
fn tags(c: &Company, s: &pd::State) -> Vec<Value> {
    let mut out = Vec::new();
    let tag = |text: String, tone: &str, kind: &str| json!({ "text": text, "tone": tone, "kind": kind });
    if s.forked {
        out.push(tag(tr("This date only"), "sky", "date"));
    } else if let Some(p) = s.pattern {
        out.push(tag(trf("Follows %{day}", &[("day", &slot_words(c, p))]), "dim", "follows"));
        if s.date_only {
            out.push(tag(tr("This date only"), "sky", "date"));
        }
    } else if s.date_only {
        out.push(tag(tr("This date only"), "sky", "date"));
    }
    if s.own && !s.forked {
        out.push(tag(tr("Own changes"), "warn", "own"));
    } else if s.unsynced {
        out.push(tag(tr("UNSYNCED"), "warn", "unsynced"));
    } else if !s.missing.is_empty() && !s.own {
        out.push(tag(trf("%{n} trips to plan by hand", &[("n", &s.missing.len().to_string())]), "warn", "missing"));
    }
    if !s.followers.is_empty() {
        let days: Vec<String> = s.followers.iter().map(|&f| slot_words(c, f)).filter(|w| !w.is_empty()).collect();
        if !days.is_empty() {
            out.push(tag(trf("Pattern: %{days}", &[("days", &days.join(", "))]), "sky", "pattern"));
        }
    }
    out
}

/// A trip a copy left to plan by hand, as the page lists it (with the row it is on).
fn missed_json(m: &Missed) -> Value {
    json!({
        "line": m.line, "number": m.number, "block": m.block, "id": format!("B|{}|{}", m.line, m.block),
        "dep": m.dep, "arr": m.arr, "from": m.from, "to": m.to,
        "label": format!("{}/{}  {}–{}  {} → {}", m.number, m.block, rf::clock(m.dep), rf::clock(m.arr), m.from, m.to),
    })
}

/// What each day of the week runs, for the day switcher (`v` the company as the studio shows
/// it).
pub(super) fn marks(v: &Company) -> Vec<Value> {
    window(v).iter().map(|d| json!(tags(v, &pd::state(v, pd::slot_of(v, d))))).collect()
}

/// The day shown: its marks, whether it can be reset to its pattern or refreshed, and the
/// trips its copy left to plan by hand.
pub(super) fn shown(v: &Company, date: &str) -> Value {
    let s = pd::state(v, pd::slot_of(v, date));
    json!({
        "tags": tags(v, &s),
        "reset": s.forked || (s.pattern.is_some() && s.own),
        "refresh": !s.forked && s.pattern.is_some() && s.unsynced && !s.own,
        "pattern": s.pattern.map(|p| slot_words(v, p)),
        "matched": s.matched, "total": s.total,
        "missing": s.missing.iter().map(missed_json).collect::<Vec<_>>(),
    })
}

// --- the "Apply to" window ----------------------------------------------------------------------------

/// The "Apply to" window of the day `date` (`rf_view` "apply"): the day and its day type, each
/// other day of the week - in its type or not, of the same timetable or not, what it runs now -
/// and what a copy would match there; and the other days the draft changed (saved too).
pub(super) fn apply_view(s: &mut Session, date: &str) -> Value {
    company::week_work(s);
    let Some(c) = s.company.clone() else { return json!({ "none": true }) };
    let v = rf::view(&c);
    let win = window(&c);
    let wd = co::plan::weekday_of(date);
    let group = group_of(wd);
    let src_lines = company::lines_of(s, date);
    let key = src_lines.as_ref().map(|l| pd::timetable_key(&v, l));
    let slot = pd::slot_of(&v, date);
    let days: Vec<Value> = win
        .iter()
        .enumerate()
        .filter(|(_, d)| d.as_str() != date)
        .map(|(k, d)| {
            let w = co::plan::weekday_of(d);
            let lines = company::lines_of(s, d);
            let same = match (&lines, key) {
                (Some(l), Some(k)) => Some(pd::timetable_key(&v, l) == k),
                _ => None,
            };
            let preview = match (&src_lines, &lines) {
                (Some(a), Some(b)) => {
                    let mut probe = v.clone();
                    let m = pd::apply(&mut probe, Day { date, lines: a }, Day { date: d, lines: b }, true, What::ALL);
                    json!({ "matched": m.matched, "total": m.total, "exact": m.exact(), "missing": m.missing.len(), "gone": m.gone })
                }
                _ => Value::Null,
            };
            let st = pd::state(&v, pd::slot_of(&v, d));
            json!({
                "k": k, "date": d, "label": short(d), "weekday": w, "in_type": group_of(w) == group, "same": same,
                "tags": tags(&v, &st), "own": st.own || st.forked, "preview": preview, "reading": lines.is_none(),
            })
        })
        .collect();
    let others: Vec<String> = pd::changed_slots(&c).into_iter().filter(|&x| x != slot).map(|x| slot_words(&c, x)).collect();
    json!({
        "date": date, "label": short(date), "k": win.iter().position(|d| d == date).unwrap_or(0),
        "weekday": tr(DAY_NAMES[wd as usize % 7]), "type": tr(DAY_GROUPS[group].0), "date_only": slot >= pd::FIRST_DATE_SLOT,
        "dirty": rf::has_draft(&c), "reading": src_lines.is_none(),
        "days": days, "others": others,
    })
}

/// What SAVE was asked to do besides saving (`rf_act` "save"'s `apply`): the day shown for its
/// date only, and the days to copy it to (each every week or that date only).
struct Plan {
    only: bool,
    targets: Vec<(String, bool)>,
    what: What,
}

fn plan_of(c: &Company, apply: Option<&Value>) -> Option<Plan> {
    let a = apply?;
    let win = window(c);
    let targets = a
        .get("targets")
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(|t| Some((win.get(t.get("k")?.as_u64()? as usize)?.clone(), t.get("as").and_then(Value::as_str) != Some("date")))).collect())
        .unwrap_or_default();
    let flag = |k: &str| a.get(k).and_then(Value::as_bool).unwrap_or(true);
    Some(Plan { only: a.get("source_as").and_then(Value::as_str) == Some("date"), targets, what: What { vehicles: flag("vehicles"), drivers: flag("drivers") } })
}

/// The days a copy from `date` reached that run the timetable of another day type and did not
/// match it all (testers: "I can't copy a timetable from MF to SA or SU"): a plan copies onto the
/// trips a day has, so the line's timetable goes there first - the Timeplan's "Copy to…"
/// (`plandays::copy_table`). Said after what was copied.
fn timetable_hint(date: &str, made: &[(String, bool, Copied)]) -> Option<String> {
    let group = group_of(co::plan::weekday_of(date));
    let days: Vec<String> = made.iter().filter(|m| !m.2.exact() && group_of(co::plan::weekday_of(&m.0)) != group).map(|m| short(&m.0)).collect();
    (!days.is_empty()).then(|| trf("%{days} run the timetable of another day type: copy the line's timetable there first (Timeplan, Timetable, Copy to…), save and Refresh - then the plan finds the same trips there.", &[("days", &days.join(", "))]))
}

/// What the copies that did not take say (testers: "sometimes it works, sometimes it doesn't"): the
/// days that run the source's planning already, those with no trip to copy onto, and those where
/// nothing matched - each named, never a copy that silently did nothing.
fn not_copied(made: &[(String, bool, Copied)]) -> Vec<String> {
    let days = |f: &dyn Fn(&Copied) -> bool| made.iter().filter(|m| f(&m.2)).map(|m| short(&m.0)).collect::<Vec<_>>().join(", ");
    let mut out = Vec::new();
    let same = days(&|m| m.same_plan);
    if !same.is_empty() {
        out.push(trf("%{days} run this planning already: nothing to copy there.", &[("days", &same)]));
    }
    let empty = days(&|m| !m.same_plan && m.total == 0 && m.gone == 0);
    if !empty.is_empty() {
        out.push(trf("%{days} have no trips of the company's lines: there was nothing to copy onto.", &[("days", &empty)]));
    }
    let none = days(&|m| !m.same_plan && m.matched == 0 && m.total + m.gone > 0);
    if !none.is_empty() {
        out.push(trf("Nothing of the planning matched on %{days}: no trip there is one of this day's (same line, way, departure and stops), so they run as their timetable has them.", &[("days", &none)]));
    }
    out
}

/// Whether a copy reached its day (`not_copied` says the others).
fn took(m: &Copied) -> bool {
    !m.same_plan && m.matched > 0
}

/// What a SAVE says: saved, and how many duties the roster has no driver for now (`plan::follow`).
fn saved_text(open: usize) -> String {
    let saved = tr("Saved: the roster, the day and the game run this planning now.");
    if open == 0 {
        return saved;
    }
    format!("{saved} {}", trf("%{n} duties have no driver in the roster: their trips are dropped until they get one (the Roster tab, Fill the roster).", &[("n", &open.to_string())]))
}

/// SAVE (and Apply to): the draft saved - the day's change for its date only when asked - and the
/// day's plan copied to the days chosen. What the copies matched comes back as the report.
pub(super) fn save(s: &mut Session, r: &mut Rf, date: &str, apply: Option<&Value>) -> Value {
    let Some(c) = s.company.clone() else { return json!({}) };
    // (the roster's buses and drivers stay with their blocks and duties on every day of the week
    // whose timetable is read: #96)
    let week: Vec<(String, Vec<LineInfo>)> = (0..7).map(|k| co::dates::add(&c.date, k)).filter_map(|d| company::lines_of(s, &d).map(|l| (d, l))).collect();
    let Some(plan) = plan_of(&c, apply) else {
        return match company::act(s, |c| Ok(rf::save_draft_following(c, &week))) {
            Ok(Some(f)) => said(saved_text(f.open), f.open > 0),
            _ => json!({}),
        };
    };
    if plan.what == (What { vehicles: false, drivers: false }) && !plan.targets.is_empty() {
        return said(tr("Choose what to copy: the vehicles, the drivers or both."), true);
    }
    // (the timetables of the day and of the days to copy to, as the week has them read)
    let src_lines = company::lines_of(s, date);
    if src_lines.is_none() && !plan.targets.is_empty() {
        return said(tr("Reading the timetable…"), true);
    }
    let mut jobs: Vec<(String, Vec<LineInfo>, bool)> = Vec::new();
    let mut unread: Vec<String> = Vec::new();
    for (d, week) in &plan.targets {
        match company::lines_of(s, d) {
            Some(l) => jobs.push((d.clone(), l, *week)),
            None => unread.push(short(d)),
        }
    }
    let src_lines = src_lines.unwrap_or_default();
    let what = plan.what;
    let only = plan.only;
    let d0 = date.to_string();
    let done = company::act(s, |c| {
        if only {
            pd::fork_draft(c, &d0);
        }
        let followed = rf::save_draft_following(c, &week);
        let saved = followed.is_some();
        let open = followed.map(|f| f.open).unwrap_or(0);
        let mut made: Vec<(String, bool, Copied)> = Vec::new();
        for (d, lines, week) in &jobs {
            let m = pd::apply(c, Day { date: &d0, lines: &src_lines }, Day { date: d, lines }, *week, what);
            made.push((d.clone(), *week, m));
        }
        pd::tidy(c);
        Ok((saved, open, made))
    });
    let Ok((saved, open, made)) = done else { return json!({}) };
    if only || !made.is_empty() {
        // (undoing now would put the day's old plan back in the weekday's: begun anew)
        r.undo.clear();
        r.redo.clear();
    }
    r.cache = None;
    let missing: usize = made.iter().filter(|m| took(&m.2)).map(|m| m.2.missing.len()).sum();
    let copied: Vec<String> = made.iter().filter(|m| took(&m.2)).map(|m| short(&m.0)).collect();
    let failed = not_copied(&made);
    let mut text = if !copied.is_empty() {
        trf("Copied to %{days}: the roster, those days and the game run it now.", &[("days", &copied.join(", "))])
    } else if only {
        trf("Saved for %{day} only: its weekday's planning stays as it was.", &[("day", &short(date))])
    } else if saved {
        saved_text(open)
    } else {
        String::new()
    };
    for f in &failed {
        text = format!("{text} {f}");
    }
    if missing > 0 {
        text = format!("{text} {}", trf("%{n} trips matched nothing: they run as their timetable has them, to plan by hand.", &[("n", &missing.to_string())]));
    }
    if let Some(h) = timetable_hint(date, &made) {
        text = format!("{text} {h}");
    }
    if !unread.is_empty() {
        text = format!("{text} {}", trf("Not copied, their timetable is still being read: %{days}.", &[("days", &unread.join(", "))]));
    }
    let win = window(&c);
    let report: Vec<Value> = made
        .iter()
        .map(|(d, week, m)| {
            json!({
                "k": win.iter().position(|x| x == d).unwrap_or(0), "label": short(d), "every_week": week,
                "matched": m.matched, "total": m.total, "exact": m.exact(), "gone": m.gone,
                "missing": m.missing.iter().map(missed_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut out = if text.is_empty() { json!({}) } else { said(text.trim(), !unread.is_empty() || open > 0 || !failed.is_empty()) };
    if !report.is_empty() {
        out["report"] = json!({ "from": short(date), "days": report });
    }
    out
}

/// "Reset to pattern" (and "Refresh"): the day shown copied again from the day it follows, or -
/// a plan of its date's own - back to what the date ran before.
pub(super) fn reset(s: &mut Session, r: &mut Rf, date: &str) -> Value {
    company::week_work(s);
    let Some(c) = s.company.clone() else { return json!({}) };
    let v = rf::view(&c);
    let slot = pd::slot_of(&v, date);
    let st = pd::state(&v, slot);
    let d0 = date.to_string();
    if st.forked {
        return match company::act(s, |c| Ok(pd::unfork(c, &d0))) {
            Ok(true) => {
                r.cache = None;
                said(trf("%{day} runs its weekday's planning again.", &[("day", &short(date))]), false)
            }
            _ => json!({}),
        };
    }
    let Some(p) = st.pattern else { return said(tr("This day follows no other day: there is nothing to reset it to."), true) };
    // (a copy takes the saved planning: the pattern's changes not saved yet would not come along,
    // and the day would stay UNSYNCED as if Refresh did nothing)
    if pd::changed_slots(&c).contains(&p) {
        return said(trf("%{day} has changes not saved: SAVE them first - a copy takes the saved planning - then try again.", &[("day", &slot_words(&c, p))]), true);
    }
    let others: Vec<String> = window(&c).into_iter().filter(|d| d != date).collect();
    let Some(src) = pd::date_for(&c, p, &others) else {
        return said(tr("The day this one follows is not in the coming week: apply its planning again from there."), true);
    };
    let (Some(src_lines), Some(lines)) = (company::lines_of(s, &src), company::lines_of(s, date)) else { return said(tr("Reading the timetable…"), true) };
    let done = company::act(s, |c| {
        let m = pd::recopy(c, slot, Day { date: &src, lines: &src_lines }, Day { date: &d0, lines: &lines });
        pd::draft_takes_saved(c, slot);
        Ok(m)
    });
    r.cache = None;
    match done {
        Ok(Some(m)) => {
            let mut out = said(trf("%{day} follows %{pattern} again: %{a} of %{b} trips as it has them.", &[("day", &short(date)), ("pattern", &slot_words(&c, p)), ("a", &m.matched.to_string()), ("b", &m.total.to_string())]), false);
            if !m.missing.is_empty() {
                out["report"] = json!({ "from": slot_words(&c, p), "days": [{ "k": window(&c).iter().position(|x| x == date).unwrap_or(0), "label": short(date), "every_week": slot < pd::FIRST_DATE_SLOT, "matched": m.matched, "total": m.total, "exact": m.exact(), "gone": m.gone, "missing": m.missing.iter().map(missed_json).collect::<Vec<_>>() }] });
            }
            out
        }
        _ => said(tr("This day follows no other day: there is nothing to reset it to."), true),
    }
}

// --- "Copy to…" ------------------------------------------------------------------------------------

/// The rows the page names by their ids (`block_id`, `duty_id`) on the day `b`: buses' blocks,
/// duties by their key, spare duties by their number.
fn rows_of(b: &rf::Board, ids: &[String]) -> pd::Rows {
    let mut rows = pd::Rows::default();
    for id in ids {
        if let Some(t) = b.tours.iter().find(|t| block_id(t) == *id) {
            rows.blocks.push((t.line.clone(), t.tour.clone()));
        } else if let Some(x) = b.shaped.iter().find(|x| duty_id(x) == *id) {
            match (&x.key, x.spare) {
                (Some(k), _) => rows.duties.push(k.clone()),
                (None, Some(sp)) => rows.spares.push(sp),
                _ => {}
            }
        }
    }
    rows
}

/// "Copy to…" (the selection bar, Ctrl+C and Ctrl+V; testers, Beta 56): the rows ticked on the
/// day `date` (`ids`) copied to the days chosen (`targets`: the day switcher's `k`, each `as`
/// "week" every week, "date" that date only, "runs" into the plan it runs) - into the draft, so
/// Undo takes it back and SAVE keeps it. Only the trips that match go along; what did not comes
/// back as the report, each a link to it on its day.
pub(super) fn copy_rows(s: &mut Session, r: &mut Rf, date: &str, args: &Value) -> Value {
    company::week_work(s);
    let Some(c) = s.company.clone() else { return json!({}) };
    let ids: Vec<String> = args.get("ids").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    let Some(src_lines) = company::lines_of(s, date) else { return said(tr("Reading the timetable…"), true) };
    let rows = rows_of(&rf::board(&rf::view(&c), &src_lines, date), &ids);
    if rows.is_empty() {
        // (Ctrl+V of rows renamed, moved or gone on their day since Ctrl+C)
        if !ids.is_empty() {
            return said(trf("The rows to copy are no longer on %{day}: tick them again there and copy.", &[("day", &short(date))]), true);
        }
        return said(tr("Tick the rows to copy first."), true);
    }
    let win = window(&c);
    let k0 = win.iter().position(|d| d == date).unwrap_or(0);
    let targets: Vec<(String, String)> = args
        .get("targets")
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(|t| Some((win.get(t.get("k")?.as_u64()? as usize)?.clone(), t.get("as").and_then(Value::as_str).unwrap_or("week").to_string()))).filter(|(d, _)| d != date).collect())
        .unwrap_or_default();
    if targets.is_empty() {
        return said(tr("Choose the days to copy to."), true);
    }
    let mut jobs: Vec<(String, String, Vec<LineInfo>)> = Vec::new();
    let mut unread: Vec<String> = Vec::new();
    for (d, how) in targets {
        match company::lines_of(s, &d) {
            Some(l) => jobs.push((d, how, l)),
            None => unread.push(short(&d)),
        }
    }
    // (that date only: a plan of the date's own, begun as the one it ran - before the edit, so
    // Undo takes the copy back and the date keeps that plan)
    let dates: Vec<String> = jobs.iter().filter(|j| j.1 == "date").map(|j| j.0.clone()).collect();
    if !dates.is_empty() {
        let _ = company::act(s, |c| {
            for d in &dates {
                pd::rows_slot(c, d, false);
            }
            Ok(())
        });
    }
    // (every week onto a date with a planning of its own: the copy goes into the weekday's, which
    // that date does not run - said, or the rows seem not to have been copied)
    let own: Vec<String> = jobs.iter().filter(|j| j.1 == "week" && pd::slot_of(&c, &j.0) >= pd::FIRST_DATE_SLOT).map(|j| short(&j.0)).collect();
    let n = ids.len().to_string();
    let days: Vec<String> = jobs.iter().map(|j| short(&j.0)).collect();
    let words = trf("%{n} rows copied to %{days}", &[("n", &n), ("days", &days.join(", "))]);
    let mut made: Vec<(String, bool, Copied)> = Vec::new();
    let d0 = date.to_string();
    let out = edit(s, r, words, |c| {
        let from = pd::slot_of(c, &d0);
        for (d, how, lines) in &jobs {
            let to = if how == "week" { co::plan::weekday_of(d) } else { pd::slot_of(c, d) };
            let m = pd::copy_rows(c, from, Day { date: &d0, lines: &src_lines }, to, Day { date: d, lines }, &rows);
            made.push((d.clone(), to < pd::FIRST_DATE_SLOT, m));
        }
        Ok(())
    });
    if out.get("edited").is_none() {
        return out;
    }
    let same: Vec<String> = made.iter().filter(|m| m.2.same_plan).map(|m| short(&m.0)).collect();
    made.retain(|m| !m.2.same_plan);
    let days: Vec<String> = made.iter().map(|m| short(&m.0)).collect();
    let matched: u32 = made.iter().map(|m| m.2.matched).sum();
    let missing: usize = made.iter().map(|m| m.2.missing.len()).sum();
    let locked: usize = made.iter().map(|m| m.2.locked as usize).sum();
    let mut text = if made.is_empty() {
        String::new()
    } else if matched == 0 && rows.spares.is_empty() {
        if locked > 0 && locked == missing {
            tr("Nothing was copied: a locked vehicle or duty there holds those trips. Unlock it there and copy again.")
        } else {
            tr("Nothing was copied: no trip of those days matches the rows' trips (same line, way, departure and stops).")
        }
    } else {
        trf("Copied to %{days}: in your draft - SAVE keeps it, Undo takes it back.", &[("days", &days.join(", "))])
    };
    if missing > locked && matched > 0 {
        text = format!("{text} {}", trf("%{n} of their trips were not copied: they match no trip there, or a locked vehicle or duty there has them.", &[("n", &(missing - locked).to_string())]));
    }
    if locked > 0 && (matched > 0 || locked < missing) {
        text = format!("{text} {}", trf("%{n} of their trips are held by a locked vehicle or duty there: unlock it there and copy again to take them.", &[("n", &locked.to_string())]));
    }
    if !same.is_empty() {
        text = format!("{text} {}", trf("%{days} run this planning already: nothing to copy there.", &[("days", &same.join(", "))]));
    }
    if !own.is_empty() {
        text = format!("{text} {}", trf("%{days} run a planning of their own for that date: the rows went into the planning of every week, which that date does not run. Copy again with “This date only” to change that date.", &[("days", &own.join(", "))]));
    }
    if let Some(h) = timetable_hint(date, &made) {
        text = format!("{text} {h}");
    }
    if !unread.is_empty() {
        text = format!("{text} {}", trf("Not copied, their timetable is still being read: %{days}.", &[("days", &unread.join(", "))]));
    }
    // (the trips not copied are the source day's: their links go there)
    let report: Vec<Value> = made
        .iter()
        .map(|(d, week, m)| {
            let missing: Vec<Value> = m.missing.iter().map(|x| {
                let mut v = missed_json(x);
                v["k"] = json!(k0);
                v
            }).collect();
            json!({ "k": win.iter().position(|x| x == d).unwrap_or(0), "label": short(d), "every_week": week, "matched": m.matched, "total": m.total, "exact": m.exact(), "gone": m.gone, "missing": missing })
        })
        .collect();
    let mut out = said(text.trim(), matched == 0 || missing > 0 || !unread.is_empty() || !same.is_empty() || !own.is_empty());
    out["edited"] = json!(true);
    if missing > 0 {
        out["report"] = json!({ "from": short(date), "rows": true, "days": report });
    }
    out
}

/// "Stop following": the day keeps its plan as it is, no pattern's any more.
pub(super) fn unfollow(s: &mut Session, r: &mut Rf, date: &str) -> Value {
    let Some(c) = s.company.clone() else { return json!({}) };
    let slot = pd::slot_of(&c, date);
    match company::act(s, |c| Ok(pd::unfollow(c, slot))) {
        Ok(true) => {
            r.cache = None;
            said(trf("%{day} keeps its planning as it is: it follows no other day now.", &[("day", &short(date))]), false)
        }
        _ => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_is_said_short_and_its_weekday_has_a_day_type() {
        super::super::super::company::words::set_language("en");
        assert!(short("2024-03-08").starts_with("FRI 8"));
        assert_eq!((group_of(0), group_of(4), group_of(5), group_of(6)), (0, 0, 1, 2));
    }

    #[test]
    fn a_plan_copied_onto_the_weekend_says_to_copy_the_timetable_first() {
        super::super::super::company::words::set_language("en");
        let partial = Copied { gone: 3, ..Default::default() };
        // (Friday onto Saturday and Sunday, matched in part: the timetable goes there first)
        let made = vec![("2024-03-09".to_string(), true, partial.clone()), ("2024-03-10".to_string(), true, partial.clone()), ("2024-03-05".to_string(), true, partial)];
        let hint = timetable_hint("2024-03-08", &made).unwrap();
        assert!(hint.starts_with("SAT 9") && hint.contains("SUN 10") && !hint.contains("TUE"), "{hint}");
        // a day of the same timetable, or one copied exactly: nothing to say
        assert!(timetable_hint("2024-03-08", &made[2..]).is_none());
        assert!(timetable_hint("2024-03-08", &[("2024-03-09".to_string(), true, Copied::default())]).is_none());
    }

    #[test]
    fn a_copy_that_did_not_take_says_why() {
        // (testers: "sometimes it works, sometimes it doesn't": never "Copied" for nothing)
        super::super::super::company::words::set_language("en");
        let ok = Copied { matched: 16, total: 16, ..Default::default() };
        let made = vec![
            ("2024-03-05".to_string(), true, ok.clone()),
            ("2024-03-06".to_string(), true, Copied { same_plan: true, ..Default::default() }),
            ("2024-03-07".to_string(), true, Copied::default()),
            ("2024-03-08".to_string(), true, Copied { total: 16, missing: vec![Missed::default(); 16], ..Default::default() }),
        ];
        assert!(took(&made[0].2) && !made[1..].iter().any(|m| took(&m.2)));
        let said = not_copied(&made).join(" ");
        assert!(said.contains("WED 6") && said.contains("run this planning already"), "{said}");
        assert!(said.contains("THU 7") && said.contains("no trips of the company's lines"), "{said}");
        assert!(said.contains("Nothing of the planning matched on FRI 8"), "{said}");
        assert!(!said.contains("TUE 5"), "{said}");
        assert!(not_copied(&made[..1]).is_empty());
    }

    /// A session of `line5_company` with its week read, and Tuesday a copy of Monday.
    fn session() -> (Session, omsi_launcher_lib::LineInfo) {
        let (mut c, l) = super::super::tests::line5_company();
        let lines = vec![l.clone()];
        let mon = co::network::tours_of_day(&c, &lines, "2024-03-04");
        co::studio::move_trip(&mut c, 0, &mon, 0, 7, co::studio::Target::New).unwrap();
        pd::apply(&mut c, Day { date: "2024-03-04", lines: &lines }, Day { date: "2024-03-05", lines: &lines }, true, What::ALL);
        let mut s = Session::default();
        s.company = Some(c);
        company::test_week(&mut s, lines);
        (s, l)
    }

    #[test]
    fn refresh_before_save_and_a_day_that_follows_none_say_what_to_do() {
        super::super::super::company::words::set_language("en");
        let (mut s, l) = session();
        let mut r = Rf::default();
        // Monday changed in the studio, not saved: Tuesday's Refresh would copy the saved Monday
        let c = s.company.as_mut().unwrap();
        let mut v = rf::view(c);
        let mon = co::network::tours_of_day(&v, &[l.clone()], "2024-03-04");
        co::studio::move_trip(&mut v, 0, &mon, 1, 7, co::studio::Target::New).unwrap();
        rf::keep_draft(c, &v);
        let before = s.company.clone();
        let out = reset(&mut s, &mut r, "2024-03-05");
        assert_eq!(out["say"]["error"], true);
        assert!(out["say"]["text"].as_str().is_some_and(|t| t.starts_with("MON has changes not saved: SAVE them first")), "{out}");
        assert_eq!(s.company, before, "nothing changed");
        // a day that follows no other: said, not a click that does nothing
        let out = reset(&mut s, &mut r, "2024-03-06");
        assert_eq!(out["say"]["text"], "This day follows no other day: there is nothing to reset it to.");
    }

    #[test]
    fn rows_gone_since_they_were_copied_are_said() {
        super::super::super::company::words::set_language("en");
        let (mut s, _) = session();
        let mut r = Rf::default();
        let out = copy_rows(&mut s, &mut r, "2024-03-04", &json!({ "ids": ["B|Linie5|9"], "targets": [{ "k": 2, "as": "week" }] }));
        assert_eq!(out["say"]["text"], "The rows to copy are no longer on MON 4 MAR: tick them again there and copy.");
        let out = copy_rows(&mut s, &mut r, "2024-03-04", &json!({ "ids": [], "targets": [{ "k": 2, "as": "week" }] }));
        assert_eq!(out["say"]["text"], "Tick the rows to copy first.");
    }
}
