//! The workshop job the player does himself (Trello "Tauri 6/6"), as our game-drawn launcher's
//! repair game (`crates/omsi-launcher-ui/src/company/repair_game.rs`; the rules are
//! `training::Job`'s): the bus from the side with its eight parts marked on it; a part picked,
//! checked - which takes a moment - what the check finds read and dealt with, the job finished
//! before the time runs out; then what each part was and what was done, and the job booked with
//! what it saved. The page keeps the clock and the check's moment; this keeps the job, opened
//! from the Career page's workshop part (`job_start`) and shown there instead of its list.

use super::words::{eur, tr, trf};
use super::Session;
use omsi_launcher_lib::company::training::{self, Fix, Job, JobDone, JobKind, Part};
use serde_json::{json, Value};

/// A job under way, or done (`repair_game::Game`).
pub struct Game {
    job: Job,
    bus: String,
    colours: [String; 2],
    /// The job booked (its quality and what it saved), or why it could not be.
    result: Option<Result<JobDone, String>>,
}

/// Where each part sits on the bus seen from the side (shares of its width and height; the front
/// on the right), as `repair_game::spot` places them.
fn spot(p: Part) -> (f32, f32) {
    match p {
        Part::Lights => (0.965, 0.72),
        Part::Wipers => (0.93, 0.30),
        Part::Doors => (0.51, 0.50),
        Part::Engine => (0.06, 0.62),
        Part::Brakes => (0.80, 0.90),
        Part::Tyres => (0.21, 0.90),
        Part::Suspension => (0.30, 0.70),
        Part::Battery => (0.66, 0.80),
    }
}

fn id_of<T: serde::Serialize>(v: T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn part_of(args: &Value) -> Option<Part> {
    serde_json::from_value(args.get("part").cloned().unwrap_or(Value::Null)).ok()
}

/// The job's buttons: begun from the workshop's list, a part checked (once its moment is over)
/// and dealt with, finished (by the player or at the bell, with the seconds left), left.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    Some(match what {
        "job_start" => {
            let Some(c) = s.company.as_ref() else { return Some(Ok(json!({}))) };
            let vehicle = args.get("vehicle").and_then(Value::as_u64).unwrap_or(0) as u32;
            let kind = if args.get("kind").and_then(Value::as_str) == Some("repair") { JobKind::Repair } else { JobKind::Service };
            let Some(v) = c.vehicle(vehicle) else { return Some(Ok(json!({}))) };
            s.repair = Some(Game { job: Job::new(c, v, kind), bus: format!("{} {}", v.number, v.name), colours: [c.colours[0].clone(), c.colours[1].clone()], result: None });
            s.generation += 1;
            Ok(json!({}))
        }
        "job_inspect" => {
            if let (Some(g), Some(p)) = (s.repair.as_mut(), part_of(args)) {
                g.job.inspect(p);
            }
            s.generation += 1;
            Ok(json!({}))
        }
        // (dealt with: on to the next part not dealt with, as the game-drawn launcher goes on)
        "job_apply" => {
            let fix: Option<Fix> = serde_json::from_value(args.get("fix").cloned().unwrap_or(Value::Null)).ok();
            let (Some(g), Some(p), Some(fix)) = (s.repair.as_mut(), part_of(args), fix) else { return Some(Ok(json!({}))) };
            g.job.apply(p, fix);
            let parts: Vec<Part> = g.job.checks.iter().map(|c| c.part).collect();
            let next = parts.iter().copied().skip_while(|x| *x != p).skip(1).chain(parts.iter().copied()).find(|x| g.job.checks.iter().any(|c| c.part == *x && c.done.is_none())).unwrap_or(p);
            s.generation += 1;
            Ok(json!({ "next": id_of(next) }))
        }
        "job_finish" => {
            let left = args.get("left").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let Some(g) = s.repair.as_ref() else { return Some(Ok(json!({}))) };
            let (q, id, kind) = (g.job.quality(left), g.job.vehicle, g.job.kind);
            let mut err = None;
            let done = super::act(s, |c| training::finish_job(c, id, kind, q).inspect_err(|e| err = Some(*e)));
            if let Some(g) = s.repair.as_mut() {
                g.result = Some(done.map_err(|_| tr(err.unwrap_or("The job could not be booked."))));
            }
            s.generation += 1;
            Ok(json!({}))
        }
        "job_leave" => {
            s.repair = None;
            s.generation += 1;
            Ok(json!({}))
        }
        _ => return None,
    })
}

/// What the page draws of the job: under way (`repair_game::play`) or done (`results`).
pub fn view(g: &Game) -> Value {
    let job = &g.job;
    let repair = job.kind == JobKind::Repair;
    let parts: Vec<Value> = job
        .checks
        .iter()
        .map(|ch| {
            let (fx, fy) = spot(ch.part);
            json!({
                "part": id_of(ch.part), "label": tr(ch.part.label()), "icon": ch.part.icon(), "at": [fx, fy],
                "inspected": ch.inspected, "done": ch.done.map(id_of),
                // (what the check finds, once it was checked)
                "reading": ch.inspected.then(|| tr(ch.part.reading(ch.fault))),
            })
        })
        .collect();
    let Some(result) = g.result.as_ref() else {
        let done = job.checks.iter().filter(|c| c.done.is_some()).count();
        return json!({
            "title": format!("{}  ·  {}", tr(if repair { "Repair job" } else { "Service job" }), g.bus),
            "sub": trf("%{n} of %{all} parts dealt with", &[("n", &done.to_string()), ("all", &job.checks.len().to_string())]),
            "seconds": job.kind.seconds(), "colours": g.colours, "parts": parts,
            "fixes": Fix::ALL.iter().map(|f| json!({ "id": id_of(*f), "label": tr(f.label()) })).collect::<Vec<_>>(),
            "finish": tr("Finish the job"), "leave": tr("Stop the job"),
            "hint": tr("Pick a part on the bus, check it, and deal with what the check finds."),
            "unchecked": tr("Not checked yet."), "check": tr("Check it"), "checking": tr("Checking…"), "finds": tr("The check finds").to_uppercase(),
        });
    };
    let (right, wrong, missed, wasted) = job.tally();
    let head = match result {
        Ok(j) => {
            let pct = (j.quality * 100.0).round() as i64;
            let saved = if j.saved >= 0 { trf("Saved: %{amount}", &[("amount", &eur(j.saved))]) } else { trf("The bus goes to the workshop after all; the parts cost %{amount}.", &[("amount", &eur(-j.saved))]) };
            json!({ "pct": format!("{pct} %"), "tone": if pct >= 75 { "ok" } else if pct >= 40 { "warn" } else { "danger" }, "saved": saved, "saved_tone": if j.saved >= 0 { "ok" } else { "warn" } })
        }
        Err(e) => json!({ "error": e }),
    };
    let rows: Vec<Value> = job
        .checks
        .iter()
        .map(|ch| {
            let good = ch.fault.unwrap_or(Fix::Leave) == ch.done.unwrap_or(Fix::Leave) && (ch.done.is_some() || ch.fault.is_none());
            let did = ch.done.map(|f| tr(f.label())).unwrap_or_else(|| tr("Not dealt with"));
            let text = match (ch.fault, good) {
                (Some(w), false) => format!("{did}  →  {}", tr(w.label())),
                _ => did,
            };
            json!({ "good": good, "label": tr(ch.part.label()), "reading": tr(ch.part.reading(ch.fault)), "did": text })
        })
        .collect();
    json!({
        "done": true, "title": tr(if repair { "Repair done" } else { "Service done" }), "bus": g.bus, "head": head,
        "tally": trf("%{right} faults fixed right, %{wrong} the wrong way, %{missed} missed; %{wasted} good parts worked on for nothing", &[("right", &right.to_string()), ("wrong", &wrong.to_string()), ("missed", &missed.to_string()), ("wasted", &wasted.to_string())]),
        "rows": rows, "back": tr("Back to the workshop"),
    })
}
