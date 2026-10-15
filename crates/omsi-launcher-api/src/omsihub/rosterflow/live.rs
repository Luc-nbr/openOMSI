//! RosterFlow with several planners at once (company multiplayer, Luc: "when one does something,
//! it must be visible to the other immediately"): the boss's launcher keeps the one plan - the
//! company's draft, its roster, its timetables - and every player's RosterFlow works on it
//! through the boss's (`together::relay`), each with a RosterFlow state of his own (`Rf`: the day
//! made, undo and redo, the Timeplan's line being changed, Optimise's proposal).
//!
//! * A change of the plan bumps the company's generation: every RosterFlow and plan page asks
//!   again with the next tick of its shell, the boss's and the players' alike.
//! * Who changed it is kept here (`Change`), and told once to everyone else in words of his own
//!   language (`told`), as is who is in RosterFlow now (`here`).
//! * The last write wins. What a planner's Undo would take back of somebody else's change is not
//!   undone: his undo and redo begin anew when somebody else changed the draft or the roster
//!   since his last command, and he is told. An edit of a day made on a board that somebody
//!   else changed after it was shown to him is refused (`stale`) rather than made on the wrong
//!   trips, and the day is shown anew.

use super::super::company::words::{day_label, tr, trf};
use super::super::company::Session;
use super::Rf;
use omsi_launcher_lib::company::rosterflow as rf;
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// Who works in RosterFlow: the boss on his own launcher, or a player of the company's session
/// (his number).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Editor {
    #[default]
    Boss,
    Guest(u32),
}

impl Editor {
    /// The name the company knows him by.
    pub(crate) fn name(self, c: &Company) -> String {
        match self {
            Editor::Boss => c.profile.clone(),
            Editor::Guest(g) => c.together.name_of(g),
        }
    }
}

/// What a change was (said in the words of whoever is told).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The day's blocks and duties (the draft).
    Blocks,
    /// The draft saved, discarded, or a day's plan copied.
    Saved,
    /// The roster: who drives, which bus.
    Roster,
    /// A line's timetable.
    Timetable,
    /// The preferences, the rules.
    Prefs,
    /// The planning's own pages.
    Planning,
}

/// A change of the plan, by whom, of which day.
struct Change {
    n: u64,
    by: Editor,
    who: String,
    kind: Kind,
    date: String,
}

/// Somebody in RosterFlow: which tab, which day, when last heard of.
struct Here {
    by: Editor,
    name: String,
    view: u64,
    day: u64,
    at: Instant,
}

/// One company's planners.
#[derive(Default)]
struct Live {
    n: u64,
    changes: Vec<Change>,
    here: Vec<Here>,
}

/// How many changes are kept.
const KEPT: usize = 30;
/// After how long without a word somebody is no longer in RosterFlow (its page says so every
/// few seconds: `rf_here`).
const GONE_AFTER: Duration = Duration::from_secs(15);

/// What an editor's RosterFlow state keeps of the others (`Rf::live`).
#[derive(Default)]
pub(crate) struct Mine {
    /// The last change he was told of (None: none yet - what came before he did is not told).
    told: Option<u64>,
    /// The last change there was when his last command ended.
    seen: u64,
    /// The draft's and the roster's fingerprints as his last command left them.
    marks: Option<(u64, u64)>,
    /// The days' boards last shown to him (their revisions), and the last change there was then.
    shown: Vec<(String, u64)>,
    /// What to tell him besides (his undo begun anew).
    notes: Vec<String>,
}

/// How many boards shown are kept (a page asks again for the one it shows).
const SHOWN_KEPT: usize = 8;

fn all() -> MutexGuard<'static, HashMap<String, Live>> {
    static L: OnceLock<Mutex<HashMap<String, Live>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner())
}

fn hash_of(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// The draft's and the roster's fingerprints.
fn marks(c: &Company) -> (u64, u64) {
    (hash_of(&serde_json::to_string(&rf::draft_of(c)).unwrap_or_default()), hash_of(&super::roster::mark(c)))
}

/// Before a command of `by`: whether somebody else changed the plan since his last one - and
/// then his undo and redo begun anew where what they would put back is no longer what he left.
pub(crate) fn before(s: &Session, r: &mut Rf, by: Editor) {
    let Some(c) = s.company.as_ref() else { return };
    let others = all().get(&c.id).is_some_and(|l| l.changes.iter().any(|x| x.n > r.live.seen && x.by != by));
    if !others {
        return;
    }
    let Some((draft, roster)) = r.live.marks else { return };
    let (d, ro) = marks(c);
    if d != draft && !(r.undo.is_empty() && r.redo.is_empty()) {
        r.undo.clear();
        r.redo.clear();
        r.before = None;
        r.live.notes.push(tr("Somebody else changed the blocks and duties: your Undo begins again from here."));
    }
    if ro != roster && !(r.roster_undo.is_empty() && r.roster_redo.is_empty()) {
        r.roster_undo.clear();
        r.roster_redo.clear();
        r.live.notes.push(tr("Somebody else changed the roster: your Undo begins again from here."));
    }
}

/// After a command of `by` (the company's generation `gen` before it): a change of the plan kept
/// (`kind`, of `date`) when it made one, his marks taken, he heard of now.
pub(crate) fn after(s: &Session, r: &mut Rf, by: Editor, gen: u64, kind: Option<Kind>, date: &str) {
    let Some(c) = s.company.as_ref() else { return };
    let mut g = all();
    let l = g.entry(c.id.clone()).or_default();
    if let Some(kind) = kind.filter(|_| s.generation != gen) {
        l.n += 1;
        l.changes.push(Change { n: l.n, by, who: by.name(c), kind, date: date.to_string() });
        if l.changes.len() > KEPT {
            l.changes.remove(0);
        }
    }
    // (still there; where he is the page's own word says, `here`)
    if let Some(h) = l.here.iter_mut().find(|h| h.by == by) {
        h.at = Instant::now();
    }
    r.live.seen = l.n;
    r.live.told.get_or_insert(l.n);
    drop(g);
    r.live.marks = Some(marks(c));
}

/// A change of the planning made on its own pages (by a player: `together::host`), told to the
/// planners in RosterFlow.
pub(crate) fn planning_changed(c: &Company, by: Editor) {
    let mut g = all();
    let l = g.entry(c.id.clone()).or_default();
    l.n += 1;
    l.changes.push(Change { n: l.n, by, who: by.name(c), kind: Kind::Planning, date: c.date.clone() });
    if l.changes.len() > KEPT {
        l.changes.remove(0);
    }
}

/// Who changed what since `by` was last told, in his words (and what else he is to be told).
pub(crate) fn told(c: &Company, r: &mut Rf, by: Editor) -> Vec<String> {
    let mut out = std::mem::take(&mut r.live.notes);
    let g = all();
    let Some(l) = g.get(&c.id) else { return out };
    let since = *r.live.told.get_or_insert(l.n);
    for x in l.changes.iter().filter(|x| x.n > since && x.by != by) {
        let day = day_label(&x.date);
        let who = [("who", x.who.as_str()), ("day", day.as_str())];
        out.push(match x.kind {
            Kind::Blocks => trf("%{who} changed the blocks and duties of %{day}.", &who),
            Kind::Saved => trf("%{who} saved the planning of %{day}.", &who),
            Kind::Roster => trf("%{who} changed the roster of %{day}.", &who),
            Kind::Timetable => trf("%{who} changed a timetable.", &who),
            Kind::Prefs => trf("%{who} changed the preferences.", &who),
            Kind::Planning => trf("%{who} changed the planning.", &who),
        });
    }
    r.live.told = Some(l.n);
    out
}

/// A day's board (its revision `rev`) shown to an editor now.
pub(crate) fn shown(c: &Company, r: &mut Rf, rev: &str) {
    let n = all().get(&c.id).map_or(0, |l| l.n);
    let m = &mut r.live.shown;
    m.retain(|x| x.0 != rev);
    m.push((rev.to_string(), n));
    if m.len() > SHOWN_KEPT {
        m.remove(0);
    }
}

/// An edit of `by`'s made on the day's board `was` (as his page showed it), which is `now` by
/// now: who else changed the plan since it was shown to him - the edit is not made then, its
/// numbers pointing at what is no longer there. None: the day is as he saw it, the change since
/// was his own, or the board is not one he was shown.
pub(crate) fn stale(c: &Company, r: &Rf, by: Editor, was: &str, now: &str) -> Option<String> {
    if was.is_empty() || was == now {
        return None;
    }
    let (_, n) = r.live.shown.iter().find(|x| x.0 == was)?;
    all().get(&c.id).and_then(|l| l.changes.iter().rev().find(|x| x.n > *n && x.by != by).map(|x| x.who.clone()))
}

/// The page's word that `by` is in RosterFlow (`rf_here`: the tab `view` and day `day`; `left`:
/// he left it): who else is in it, and what he is to be told.
pub(crate) fn here(s: &Session, r: &mut Rf, by: Editor, args: &Value) -> Value {
    let Some(c) = s.company.as_ref() else { return json!({ "here": [], "told": [] }) };
    let num = |k: &str| args.get(k).and_then(Value::as_u64).unwrap_or(0);
    let left = args.get("left").and_then(Value::as_bool).unwrap_or(false);
    let now = Instant::now();
    let others: Vec<Value> = {
        let mut g = all();
        let l = g.entry(c.id.clone()).or_default();
        l.here.retain(|h| h.by != by && now.duration_since(h.at) < GONE_AFTER);
        if !left {
            l.here.push(Here { by, name: by.name(c), view: num("view"), day: num("day"), at: now });
        }
        l.here
            .iter()
            .filter(|h| h.by != by)
            .map(|h| {
                let tab = match h.view {
                    0 => "Roster",
                    1 => "Vehicles",
                    2 => "Drivers",
                    3 => "Timeplan",
                    _ => "Routes",
                };
                let date = omsi_launcher_lib::company::dates::add(&c.date, h.day.min(6) as i64);
                json!({
                    "name": h.name, "boss": h.by == Editor::Boss,
                    "initial": h.name.chars().next().map(|x| x.to_uppercase().to_string()).unwrap_or_default(),
                    "where": format!("{} · {}", tr(tab), day_label(&date)),
                })
            })
            .collect()
    };
    json!({ "here": others, "told": told(c, r, by) })
}

#[cfg(test)]
pub(crate) fn forget(company: &str) {
    all().remove(company);
}
