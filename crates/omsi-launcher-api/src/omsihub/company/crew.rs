//! The driver's side of the company for a player of "Play together" (`co_crew`, relayed by his
//! launcher: `together::relay`): his duty swaps with the other players, his sick call and his days
//! off, a handover note and a defect of the bus he drives - each on his own duty and his own bus
//! only, the rules the company's (`co::crew`). The boss and a single player have the same on the
//! phone (`companion/company.rs`, the company's queue).

use super::words::tr;
use super::{plan_of, week_work, Session};
use crate::Answer;
use omsi_launcher_lib::company::crew::{self, DefectKind, NoteTemplate, SwapState};
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::holidays::Reason;
use omsi_launcher_lib::company::plan::{BusOf, DayPlan, Who};
use omsi_launcher_lib::company::together::book::ClaimState;
use serde_json::{json, Value};

/// Why a player's crew action is refused (English: keys of the translations).
pub const NOT_YOUR_BUS: &str = "That is not the bus of your duty.";
pub const NOT_READY: &str = "The timetable of the day is still being read.";

/// The company's day plan (None: still being read).
fn today(s: &mut Session) -> Option<DayPlan> {
    let date = s.company.as_ref()?.date.clone();
    week_work(s);
    plan_of(s, &date)?.ok()
}

/// The buses of player `guest`'s duties of the day: those he may write a note or a defect for.
fn buses_of(s: &mut Session, guest: u32) -> Vec<u32> {
    let Some(p) = today(s) else { return Vec::new() };
    let Some(c) = s.company.as_ref() else { return Vec::new() };
    c.together
        .claims
        .iter()
        .filter(|x| x.guest == guest && x.duty.date == p.date && x.state != ClaimState::Asked)
        .filter_map(|x| x.duty.find(&p).and_then(|(ti, _)| p.tours[ti].bus))
        .filter_map(|b| if let BusOf::Own(id) = b { Some(id) } else { None })
        .collect()
}

fn duty_of(args: &Value) -> Option<DutyRef> {
    serde_json::from_value(args.get("duty").cloned()?).ok()
}

fn s_arg<'a>(args: &'a Value, k: &str) -> &'a str {
    args.get(k).and_then(Value::as_str).unwrap_or("")
}

/// What player `guest` sees of his side: the swaps asked of him and his own, the players he may
/// ask, the buses of his duties with their notes, the forms' choices.
pub fn view(s: &mut Session, guest: u32) -> Value {
    let buses = buses_of(s, guest);
    let Some(c) = s.company.as_ref() else { return json!({ "none": true }) };
    let me = Some(Who::Guest(guest));
    let swaps: Vec<Value> = c
        .crew
        .swaps
        .iter()
        .filter(|x| x.from == me || x.with == me)
        .map(|x| json!({ "no": x.no, "mine": x.mine, "theirs": x.theirs, "state": x.state, "why": tr(&x.why), "asked_of_me": x.with == me && x.state == SwapState::Asked, "from": x.from.map(|w| if let Who::Guest(g) = w { c.together.name_of(g) } else { String::new() }) }))
        .collect();
    let players: Vec<Value> = c.together.guests.iter().filter(|g| g.no != guest && !g.banned).map(|g| json!({ "no": g.no, "name": g.name })).collect();
    let buses: Vec<Value> = buses
        .iter()
        .filter_map(|id| c.vehicle(*id))
        .map(|v| json!({ "id": v.id, "number": v.number, "notes": crew::notes_for(c, v.id).iter().map(|n| super::launch::note_words(n)).collect::<Vec<_>>() }))
        .collect();
    json!({
        "swaps": swaps, "players": players, "buses": buses,
        "templates": NoteTemplate::ALL.iter().map(|t| json!([t.key(), tr(t.text())])).collect::<Vec<_>>(),
        "checks": DefectKind::ALL.iter().map(|k| json!([k.key(), tr(k.label())])).collect::<Vec<_>>(),
        "reasons": Reason::ALL.iter().map(|r| json!([r, tr(r.label())])).collect::<Vec<_>>(),
        "words": {
            "caps": tr("My duties").to_uppercase(), "swaps": tr("Swap requests"), "asked_of_me": tr("%{name} asks you to swap duty %{mine}"),
            "accept": tr("Accept"), "refuse": tr("Refuse"), "swap": tr("Swap"), "with": tr("Ask to swap with"), "ask": tr("Ask"),
            "asked": tr("asked"), "accepted": tr("accepted"), "refused": tr("refused"),
            "sick": tr("Call in sick"), "sick_confirm": tr("Your duties of those days go to the dispatcher. Call in sick?"),
            "bus": tr("Bus %{bus} today"), "notes": tr("Handover notes"), "no_notes": tr("No notes for today."), "note": tr("Leave a note"), "note_ph": tr("Or type a note"), "none": tr("None"),
            "defect": tr("Report a defect"), "defect_ph": tr("What is wrong (optional)"), "report": tr("Report"), "no_bus": tr("No duty with a bus today."),
            "sent": tr("Sent: the boss's launcher carries it out."),
        },
    })
}

/// A crew action of player `guest` (`args.act`): carried out on the company by its rules, on his
/// own duty and bus only. Its answer, or the refusal's words.
pub fn act(s: &mut Session, guest: u32, args: &Value) -> Answer {
    let me = Who::Guest(guest);
    let by = s.company.as_ref().map(|c| c.together.name_of(guest)).unwrap_or_default();
    let refused = |e: Value| e.get("text").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| tr("This cannot be done"));
    match s_arg(args, "act") {
        "view" => Ok(view(s, guest)),
        "note" | "defect" => {
            let act = s_arg(args, "act");
            let bus = args.get("bus").and_then(Value::as_u64).unwrap_or(0) as u32;
            if !buses_of(s, guest).contains(&bus) {
                return Err(tr(NOT_YOUR_BUS));
            }
            let text = s_arg(args, "text").to_string();
            let r = if act == "note" {
                let template = NoteTemplate::from_key(s_arg(args, "template"));
                super::act(s, |c| crew::add_note(c, bus, &by, template, &text))
            } else {
                let kind = DefectKind::from_key(s_arg(args, "kind")).unwrap_or(DefectKind::Other);
                super::act(s, |c| crew::report_defect(c, bus, kind, &text, &by, false))
            };
            r.map(|id| json!({ "ok": true, "id": id })).map_err(refused)
        }
        "swap" => {
            let (Some(mine), Some(with)) = (duty_of(args), args.get("with").and_then(Value::as_u64)) else { return Err(tr(crew::NO_DUTY)) };
            let date = mine.date.clone();
            week_work(s);
            let Some(Ok(p)) = plan_of(s, &date) else { return Err(tr(NOT_READY)) };
            super::act(s, |c| crew::ask_swap(c, &p, me, &mine, Who::Guest(with as u32))).map(|x| json!({ "ok": true, "state": x.state, "why": tr(&x.why) })).map_err(refused)
        }
        "swap_answer" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            let yes = args.get("yes").and_then(Value::as_bool).unwrap_or(false);
            let date = s.company.as_ref().and_then(|c| c.crew.swaps.iter().find(|x| x.no == no).map(|x| x.mine.date.clone())).unwrap_or_default();
            week_work(s);
            let Some(Ok(p)) = plan_of(s, &date) else { return Err(tr(crew::NO_SWAP)) };
            super::act(s, |c| crew::answer_swap(c, &p, no, me, yes)).map(|x| json!({ "ok": true, "state": x })).map_err(refused)
        }
        "sick" => {
            let days = args.get("days").and_then(Value::as_u64).unwrap_or(1) as u32;
            super::act(s, |c| crew::call_in_sick(c, me, days)).map(|until| json!({ "ok": true, "until": until })).map_err(refused)
        }
        "leave" => {
            let reason = args.get("reason").cloned().and_then(|r| serde_json::from_value::<Reason>(r).ok()).unwrap_or_default();
            let (from, until) = (s_arg(args, "from").to_string(), s_arg(args, "until").to_string());
            super::act(s, |c| crew::ask_leave(c, me, &from, &until, reason)).map(|(no, yes)| json!({ "ok": true, "no": no, "approved": yes })).map_err(refused)
        }
        other => Err(format!("no crew action {other}")),
    }
}
