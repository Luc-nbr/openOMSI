//! The control room's messages on the launcher's side (`co::messages`, docs/omsihub/MESSAGES.md):
//! what the Operations page shows and sends of them, and the link to the driver's devices.
//!
//! The link (`tick`, every two seconds with the page): the duty Omsi-Hub gave the plugin of the
//! game it starts (`addon::duty_link`), when the game runs, is the one the player drives
//! (`Messages::driving`): what the plugin replied is taken in (`data/replies.jsonl`: delivered,
//! read, accepted, declined, a quick button), how the duty stands goes to the AI dispatcher
//! (`data/status.json`: `messages::dispatch`), and the duty's inbox goes back translated - to the
//! plugin (`data/messages.json`) and beside the company for the phone (`messages::inbox_file`).
//! The AI drivers answer what came to them, and what was accepted is carried out with today's
//! plan (`messages::ai_answers`, `messages::carry_out`).

use super::words::{filled, tr, trf};
use super::Session;
use omsi_launcher_lib::company::clock::{self as ck, Run};
use omsi_launcher_lib::company::control::Act;
use omsi_launcher_lib::company::messages::{self as msg, Action, Answer, Canned, From, Msg, Priority, Quick, To, What};
use omsi_launcher_lib::company::plan::{BusOf, DayPlan};
use omsi_launcher_lib::company::Company;
use omsihub_plugin::bridge::{self, Choice, Inbox, InboxAction, InboxMsg, Reply, Status};
use serde_json::{json, Value};
use std::time::Instant;

/// How often the link looks (s).
const EVERY: f32 = 2.0;
/// The messages the driver's inbox holds.
const INBOX_KEPT: usize = 12;
/// The messages the Operations page lists.
const LOG_SHOWN: usize = 40;
/// A status older than this (s) is of a game that stopped writing it.
const STATUS_FRESH: f64 = 60.0;

/// What the link keeps between two looks.
#[derive(Default)]
pub struct Link {
    at: Option<Instant>,
    /// The phone's inbox as last written.
    phone: String,
}

/// The inbox's own words, for a screen without the plugin's texts (the phone).
const WORDS: &[&str] = &["Messages", "Accept", "Decline", "Report to control", "No messages", "Control", "Sent to control", "Hide"];

/// A message's text as it is shown: its canned text translated and filled, or the sender's own.
pub fn text(m: &Msg) -> String {
    if m.free {
        m.text.clone()
    } else {
        filled(&m.text, &m.args)
    }
}

fn number(dp: Option<&DayPlan>, line: &str) -> String {
    dp.and_then(|p| p.tours.iter().find(|t| t.tour.line.eq_ignore_ascii_case(line))).map(|t| t.tour.number.clone()).unwrap_or_else(|| line.to_string())
}

/// Who sent a message, in words.
pub fn from_label(m: &Msg, dp: Option<&DayPlan>) -> String {
    match &m.from {
        From::Control => tr("Control"),
        From::Dispatcher => tr("Dispatcher"),
        From::Manager { name } => name.clone(),
        From::Driver { line, tour, name } if name.trim().is_empty() => trf("Driver of line %{n} tour %{tour}", &[("n", &number(dp, line)), ("tour", tour)]),
        From::Driver { name, .. } => name.clone(),
    }
}

/// Who a message goes to, in words.
pub fn to_label(c: &Company, to: &To, dp: Option<&DayPlan>) -> String {
    match to {
        To::Duty { line, tour } => trf("Line %{n} tour %{tour}", &[("n", &number(dp, line)), ("tour", tour)]),
        To::Driver { employee } => c.employee(*employee).map(|e| e.name.clone()).unwrap_or_default(),
        To::Line { line } => trf("Every driver of line %{n}", &[("n", &number(dp, line))]),
        To::All => tr("Every driver"),
        To::Control => tr("Control"),
    }
}

fn action_of(a: &Action) -> InboxAction {
    let (part, minutes, stop) = match &a.what {
        What::ShortTurn { part, .. } | What::OutOfService { part, .. } => (*part, 0, String::new()),
        What::Hold { minutes, .. } => (0.0, *minutes, String::new()),
        What::Relief { stop, .. } => (0.0, 0, stop.clone()),
        What::Layover { saved } => (0.0, *saved, String::new()),
        What::Swap { .. } => (0.0, 0, String::new()),
    };
    InboxAction { kind: a.what.key().to_string(), departure: a.dep as f64 * 60.0, part, minutes, stop, label: tr(a.what.label()) }
}

fn choice(key: &str, label: &str, icon: &str) -> Choice {
    Choice { key: key.to_string(), label: tr(label), icon: icon.to_string() }
}

/// The inbox of the driver of tour `tour` of line `line`, in the page's language: the plugin's
/// and the phone's.
pub fn inbox(c: &Company, line: &str, tour: &str, dp: Option<&DayPlan>) -> Inbox {
    inbox_of(&c.name, c.messages.of_driver(line, tour, None, INBOX_KEPT), line, tour, dp)
}

/// The inbox of messages `list` (the newest first) of company `company`'s duty (`line`, `tour`):
/// also a player's of "Play together", whose snapshot carries them.
pub fn inbox_of<'a>(company: &str, list: impl IntoIterator<Item = &'a Msg>, line: &str, tour: &str, dp: Option<&DayPlan>) -> Inbox {
    let messages = list
        .into_iter()
        .map(|m| InboxMsg {
            id: m.id,
            time: ck::hhmm(m.at),
            from: from_label(m, dp),
            mine: m.from_driver(),
            priority: m.priority.key().to_string(),
            text: text(m),
            action: m.action.as_ref().map(action_of),
            state: m.state.key().to_string(),
            state_label: tr(m.state.label()),
            reply: Canned::of(&m.reply).map(|r| tr(r.label())).unwrap_or_default(),
            outcome: if m.outcome.is_empty() { String::new() } else { tr(&m.outcome) },
        })
        .collect();
    Inbox {
        company: company.to_string(),
        line: line.to_string(),
        tour: tour.to_string(),
        messages,
        quick: Quick::ALL.iter().map(|q| choice(q.key(), q.label(), q.icon())).collect(),
        replies: Canned::ALL.iter().map(|r| choice(r.key(), r.label(), "")).collect(),
        words: WORDS.iter().map(|w| (w.to_string(), tr(w))).collect(),
        ..Inbox::default()
    }
}

/// How the duty stands, as the plugin wrote it, for the dispatcher: with what the day's plan
/// knows of the tour (its trips' worth, its bus).
pub fn driving_of(st: &Status, dp: Option<&DayPlan>) -> Option<msg::Driving> {
    let minute = |s: Option<f64>| s.map(|x| (x / 60.0).round() as i32);
    let (dep, arr) = (minute(st.departure)?, minute(st.arrival)?);
    let t = dp.and_then(|p| p.tours.iter().find(|t| t.tour.line.eq_ignore_ascii_case(&st.line) && t.tour.tour.eq_ignore_ascii_case(&st.tour)));
    let trip = t.and_then(|t| t.tour.trips.iter().find(|x| (x.dep - dep).abs() <= 1));
    let left = if st.stops > 0 { 1.0 - st.passed as f64 / st.stops as f64 } else { 0.5 };
    Some(msg::Driving {
        line: st.line.clone(),
        tour: st.tour.clone(),
        trip: st.trip,
        dep,
        arr,
        next_dep: minute(st.next_departure),
        next_arr: minute(st.next_arrival),
        counts: trip.is_none_or(|x| x.counts()) && !st.trip_done,
        late: st.delay.unwrap_or(0.0) / 60.0,
        left: left.clamp(0.0, 1.0),
        vehicle: t.and_then(|t| match t.bus {
            Some(BusOf::Own(id)) => Some(id),
            _ => None,
        }),
    })
}

/// A line of the plugin taken in (each once): an answer to a message, or a quick button to control
/// from the duty (`line`, `tour`) of driver `driver`. Whether it changed anything.
pub fn take_reply(c: &mut Company, r: &Reply, line: &str, tour: &str, driver: &str, dp: Option<&DayPlan>, run: &mut Run) -> bool {
    if !c.messages.first_seen(&r.uid) {
        return false;
    }
    if r.kind == "quick" {
        return match Quick::of(&r.key) {
            Some(q) => {
                msg::driver_says(c, line, tour, driver, q, dp, run);
                true
            }
            None => false,
        };
    }
    let key = if r.kind == "reply" { r.key.as_str() } else { r.kind.as_str() };
    match Answer::of(key) {
        Some(a) => msg::answer(c, r.id, a, Some((line, tour))).is_ok(),
        None => false,
    }
}

fn now_unix() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// The link with the driver's devices, every two seconds (`company::work`).
pub fn tick(s: &mut Session) {
    if s.closing || s.msg_link.at.is_some_and(|t| t.elapsed().as_secs_f32() < EVERY) {
        return;
    }
    s.msg_link.at = Some(Instant::now());
    let Some(c) = s.company.as_ref() else { return };
    let date = c.date.clone();
    let name = c.name.clone();
    // (the duty the plugin was given for this company)
    let link = super::super::addon::duty_link().filter(|(_, a)| a.company == name);
    let game = link.is_some() && s.in_game();
    super::week_work(s);
    let dp = super::plan_of(s, &date).and_then(Result::ok);
    let Some(c) = s.company.as_mut() else { return };
    let before = c.messages.clone();
    let feed = c.clock.feed.len();
    let breaks = c.clock.today.as_ref().map_or(0, |x| x.breaks.len());
    c.messages.driving = link.as_ref().filter(|_| game).map(|(_, a)| (a.line.clone(), a.tour.clone()));
    let mut run = Run::default();
    if let (Some((dir, a)), true) = (link.as_ref(), game) {
        let mut reader = bridge::ReplyReader { offset: c.messages.link };
        let new = reader.read_new(dir);
        c.messages.link = reader.offset;
        for r in &new {
            take_reply(c, r, &a.line, &a.tour, &a.driver, dp.as_ref(), &mut run);
        }
        let st = bridge::read_status(dir).filter(|st| st.issued == a.issued && (now_unix() - st.at).abs() < STATUS_FRESH);
        if let Some(d) = st.as_ref().and_then(|st| driving_of(st, dp.as_ref())) {
            msg::dispatch(c, &d);
        }
    }
    if let Some(dp) = dp.as_ref().filter(|p| p.today) {
        msg::ai_answers(c, dp, &mut run);
        msg::carry_out(c, dp, &mut run);
    }
    // the inbox back to the plugin and the phone
    if let Some((dir, a)) = link.as_ref() {
        let ib = inbox(c, &a.line, &a.tour, dp.as_ref());
        if let Err(e) = bridge::write_inbox(dir, &ib) {
            omsi_launcher_lib::log_to_file(&format!("messages: the plugin's inbox not written: {e}"));
        }
        let text = serde_json::to_string_pretty(&ib).unwrap_or_default();
        if text != s.msg_link.phone {
            let id = c.id.clone();
            if std::fs::write(msg::inbox_file(&super::data(), &id), &text).is_ok() {
                s.msg_link.phone = text;
            }
        }
    }
    let Some(c) = s.company.as_ref() else { return };
    let moved = c.messages != before || c.clock.feed.len() != feed || c.clock.today.as_ref().map_or(0, |x| x.breaks.len()) != breaks;
    if moved {
        if c.clock.today.as_ref().map_or(0, |x| x.breaks.len()) != breaks {
            s.plan_gen += 1;
        }
        super::changed(s);
    }
}

// --- the Operations page --------------------------------------------------------------------------

/// The canned texts and the priorities the compose box offers, and its words.
pub fn compose_json() -> Value {
    json!({
        "title": tr("Message to the driver"),
        "to": tr("To"),
        "templates": msg::TEMPLATES.iter().map(|t| json!({ "key": t, "label": tr(t) })).collect::<Vec<_>>(),
        "template": tr("Canned message"),
        "own": tr("Your own words"),
        "own_ph": tr("Type a message…"),
        "priority": tr("Priority"),
        "priorities": Priority::ALL.iter().map(|p| json!([p.key(), tr(p.label())])).collect::<Vec<_>>(),
        "instruction": tr("Instruction"),
        "none": tr("No instruction"),
        "minutes": tr("min"),
        "send": tr("Send"),
        "cancel": tr("Cancel"),
        "button": tr("Message"),
    })
}

/// The ways out of a late bus as instructions for its driver (the row's own), and the holds.
pub fn instructions(c: &Company, dp: &DayPlan, line: &str, tour: &str, dep: i32, ways: &[(i32, Act)]) -> Vec<Value> {
    let hm = |m: i32| ck::hhmm(ck::moment(&dp.date, m as i64));
    let bus = |v: u32| c.vehicle(v).map(|x| x.number.clone()).unwrap_or_default();
    let mut out: Vec<Value> = ways
        .iter()
        .filter_map(|(d, act)| Action::of(line, tour, *d, *act))
        .map(|a| {
            let (t, args) = msg::action_text(&a, hm, bus);
            json!({ "label": filled(t, &args), "args": { "line": line, "tour": tour, "dep": a.dep, "how": serde_json::to_value(&a.what).unwrap_or(Value::Null) } })
        })
        .collect();
    for connection in [true, false] {
        for m in msg::HOLDS {
            let a = Action { line: line.to_string(), tour: tour.to_string(), dep, what: What::Hold { minutes: m, connection } };
            let (t, args) = msg::action_text(&a, hm, bus);
            out.push(json!({ "label": filled(t, &args), "args": { "line": line, "tour": tour, "dep": dep, "how": serde_json::to_value(&a.what).unwrap_or(Value::Null) } }));
        }
    }
    out
}

/// The messages of today and before, the newest first, for the Operations page's list.
pub fn log_json(c: &Company, dp: Option<&DayPlan>) -> Vec<Value> {
    c.messages
        .list
        .iter()
        .rev()
        .take(LOG_SHOWN)
        .map(|m| {
            json!({
                "id": m.id,
                "time": ck::hhmm(m.at),
                "from": from_label(m, dp),
                "to": to_label(c, &m.to, dp),
                "text": text(m),
                "prio": { "key": m.priority.key(), "label": tr(m.priority.label()) },
                "state": { "key": m.state.key(), "label": tr(m.state.label()) },
                "reply": Canned::of(&m.reply).map(|r| tr(r.label())),
                "outcome": (!m.outcome.is_empty()).then(|| tr(&m.outcome)),
                "mine": m.from_driver(),
                "open": m.open(),
                "done": m.done,
                "what": m.action.as_ref().map(|a| tr(a.what.label())),
            })
        })
        .collect()
}

/// A driver's messages control has not seen to yet: rows of the incident table.
pub fn incident_rows(c: &Company, dp: &DayPlan) -> Vec<Value> {
    let day0 = ck::moment(&c.date, 0);
    c.messages
        .list
        .iter()
        .rev()
        .filter(|m| m.from_driver() && !m.done && m.at >= day0)
        .filter_map(|m| {
            let From::Driver { line, tour, .. } = &m.from else { return None };
            let t = dp.tours.iter().find(|t| t.tour.line.eq_ignore_ascii_case(line) && t.tour.tour.eq_ignore_ascii_case(tour));
            let bus = t.and_then(|t| match t.bus {
                Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.number.clone()),
                _ => None,
            });
            let (key, label) = match m.priority {
                Priority::Urgent => ("high", tr("High")),
                Priority::Instruction => ("medium", tr("Medium")),
                Priority::Info => ("low", tr("Low")),
            };
            Some(json!({
                "kind": "msg",
                "sev": { "key": key, "label": label },
                "line": line.to_lowercase(),
                "plate": t.map(|t| super::lines::plate(&t.tour.number, &super::ops::hex(omsi_launcher_lib::company::rosterflow::line_colour(c, &t.tour.line)))),
                "bus": bus.unwrap_or_default(),
                "delay": ck::hhmm(m.at),
                "at": "",
                "cause": text(m),
                "detail": from_label(m, Some(dp)),
                "map": t.map(|t| format!("{}|{}", t.tour.line, t.tour.tour)),
                "to": { "kind": "duty", "line": line, "tour": tour, "label": to_label(c, &To::Duty { line: line.clone(), tour: tour.clone() }, Some(dp)) },
                "options": [{ "label": tr("Seen to: tell the driver"), "short": tr("Acknowledge"), "icon": "check_circle", "effect": "", "best": false, "good": true, "args": { "id": m.id } }],
            }))
        })
        .collect()
}

/// Why a message is not sent (English).
pub const NOTHING_TO_SEND: &str = "Choose a canned message, type your own or choose an instruction.";
pub const NO_DRIVER: &str = "Choose who the message goes to.";

fn to_of(v: &Value) -> Option<To> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Some(match v.get("kind").and_then(Value::as_str)? {
        "duty" if !s("line").is_empty() => To::Duty { line: s("line"), tour: s("tour") },
        "driver" => To::Driver { employee: v.get("employee").and_then(Value::as_u64)? as u32 },
        "line" if !s("line").is_empty() => To::Line { line: s("line") },
        "all" => To::All,
        _ => return None,
    })
}

/// A message from the compose box (`ops_msg`): to whom, a canned text or the sender's own words,
/// a priority, an instruction; sent from control, or by name from a manager of "Play together"
/// (`Session::sender`). The AI drivers answer at once. What to say, or why not.
pub fn send(s: &mut Session, args: &Value) -> Result<String, String> {
    let to = args.get("to").and_then(to_of).ok_or_else(|| tr(NO_DRIVER))?;
    let template = args.get("template").and_then(Value::as_str).unwrap_or("");
    let own = args.get("text").and_then(Value::as_str).map(msg::clean).unwrap_or_default();
    let priority = args.get("priority").and_then(Value::as_str).and_then(Priority::of).unwrap_or_default();
    let action = args.get("action").filter(|a| !a.is_null()).and_then(|a| {
        let line = a.get("line").and_then(Value::as_str)?.to_string();
        let tour = a.get("tour").and_then(Value::as_str)?.to_string();
        let dep = a.get("dep").and_then(Value::as_i64)? as i32;
        let what: What = serde_json::from_value(a.get("how")?.clone()).ok()?;
        Some(Action { line, tour, dep, what })
    });
    let from = s.sender.clone().map(|name| From::Manager { name }).unwrap_or(From::Control);
    let Some(date) = s.company.as_ref().map(|c| c.date.clone()) else { return Err(tr("That cannot be done now.")) };
    let dp = super::plan_of(s, &date).and_then(Result::ok);
    let Some(c) = s.company.as_mut() else { return Err(tr("That cannot be done now.")) };
    let hm = |m: i32| ck::hhmm(ck::moment(&date, m as i64));
    let (text, targs, free) = if !own.is_empty() {
        (own, Vec::new(), true)
    } else if msg::TEMPLATES.contains(&template) {
        // (the canned texts' places: the minutes late of the bus written to)
        let late = args.get("min").and_then(Value::as_i64).unwrap_or(0);
        (template.to_string(), if template == msg::LATE { vec![("min".to_string(), late.to_string())] } else { Vec::new() }, false)
    } else if let Some(a) = action.as_ref() {
        let (t, x) = msg::action_text(a, hm, |v| c.vehicle(v).map(|b| b.number.clone()).unwrap_or_default());
        (t.to_string(), x, false)
    } else {
        return Err(tr(NOTHING_TO_SEND));
    };
    let id = msg::send(c, from, to.clone(), priority, &text, targs, free, action);
    let mut run = Run::default();
    if let Some(dp) = dp.as_ref().filter(|p| p.today) {
        msg::ai_answers(c, dp, &mut run);
        msg::carry_out(c, dp, &mut run);
    }
    let said = match c.messages.get(id).map(|m| (m.state, m.outcome.clone())) {
        Some((msg::State::Accepted, _)) => trf("Sent to %{to}: the driver carries it out.", &[("to", &to_label(c, &to, dp.as_ref()))]),
        Some((msg::State::Declined, why)) => trf("Sent to %{to}: declined - %{why}", &[("to", &to_label(c, &to, dp.as_ref())), ("why", &tr(&why))]),
        _ => trf("Sent to %{to}.", &[("to", &to_label(c, &to, dp.as_ref()))]),
    };
    if dp.is_some() {
        s.plan = None;
        s.plan_gen += 1;
    }
    Ok(said)
}

/// A driver's message seen to (`ops_msg_ack`): the driver told.
pub fn acknowledge(s: &mut Session, args: &Value) -> Result<String, String> {
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0);
    let from = s.sender.clone().map(|name| From::Manager { name }).unwrap_or(From::Control);
    let c = s.company.as_mut().ok_or_else(|| tr("That cannot be done now."))?;
    msg::acknowledge(c, id, from, false).map_err(tr)?;
    Ok(tr("The driver is told that control is on it."))
}

/// Every text of this part (for their translations: `ops`'s test).
#[cfg(test)]
pub fn texts() -> Vec<&'static str> {
    let mut out = msg::texts();
    out.extend(WORDS);
    out.extend([NOTHING_TO_SEND, NO_DRIVER, msg::NO_MESSAGE, msg::ANSWERED, msg::NOT_YOURS, omsi_launcher_lib::company::remote::NOT_DRIVING, "Dispatcher", "Driver of line %{n} tour %{tour}", "Line %{n} tour %{tour}", "Every driver of line %{n}", "Every driver"]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_line_is_taken_in_once_and_a_quick_button_reaches_control() {
        super::super::words::set_language("en");
        let mut c = omsi_launcher_lib::company::found(&omsi_launcher_lib::company::Founding { name: "Link".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let id = msg::post(&mut c, To::Duty { line: "Linie5".into(), tour: "1".into() }, Priority::Info, msg::DIVERSION, Vec::new(), None);
        let r = Reply { kind: "read".into(), id, uid: "7-1".into(), ..Reply::default() };
        let mut run = Run::default();
        assert!(take_reply(&mut c, &r, "Linie5", "1", "Luc", None, &mut run));
        assert!(!take_reply(&mut c, &r, "Linie5", "1", "Luc", None, &mut run));
        assert_eq!(c.messages.get(id).unwrap().state, msg::State::Read);
        // (another tour's driver: refused)
        let other = msg::post(&mut c, To::Duty { line: "Linie5".into(), tour: "2".into() }, Priority::Info, msg::DIVERSION, Vec::new(), None);
        assert!(!take_reply(&mut c, &Reply { kind: "read".into(), id: other, uid: "7-2".into(), ..Reply::default() }, "Linie5", "1", "Luc", None, &mut run));
        assert!(take_reply(&mut c, &Reply { kind: "quick".into(), key: "request_relief".into(), uid: "7-3".into(), ..Reply::default() }, "Linie5", "1", "Luc", None, &mut run));
        let last = c.messages.list.last().unwrap();
        assert_eq!((last.to.clone(), last.quick), (To::Control, Some(Quick::RequestRelief)));
        // the driver's inbox: his, newest first, with his own and the buttons' words
        let ib = inbox(&c, "Linie5", "1", None);
        assert_eq!(ib.messages.iter().map(|m| m.id).collect::<Vec<_>>(), vec![last.id, id]);
        assert!(ib.messages[0].mine && ib.messages[1].text == msg::DIVERSION && ib.messages[1].state == "read");
        assert_eq!((ib.quick.len(), ib.replies.len()), (7, 4));
    }

    #[test]
    fn the_status_the_plugin_writes_is_what_the_dispatcher_reads() {
        let st = Status { line: "Linie5".into(), tour: "1".into(), trip: 2, departure: Some(25_200.0), arrival: Some(28_200.0), next_departure: Some(28_800.0), delay: Some(420.0), stops: 10, passed: 4, ..Status::default() };
        let d = driving_of(&st, None).unwrap();
        assert_eq!((d.trip, d.dep, d.arr, d.next_dep, d.late, d.left), (2, 420, 470, Some(480), 7.0, 0.6));
        assert!(driving_of(&Status { departure: None, ..st }, None).is_none());
    }
}
