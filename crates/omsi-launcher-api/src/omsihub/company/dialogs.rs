//! What lies over the company's pages (`crates/omsi-launcher-ui/src/company/mod.rs`, `clock.rs`,
//! `settings.rs`): the company's settings with its game mode and add-ons, the time dialog, a
//! confirmation, the report of the days closed, what happened while the player was away, and the
//! questions the clock waits for - a breakdown, a tender, the morning's disruptions.

use super::kit::Go;
use super::words::{day_label, eur, eur_cents, grouped, month_label, num, signed, span_text, tr, trf};
use super::{act as apply, feed_text, feed_tone, mark_json, may_step, mode_icon, mode_json, now_label, plan_of, simulate, week_work, Session, CONCESSIONS_TAB};
use omsi_launcher_lib::company::clock::{self as ck, Ask, Choice, FeedItem, Level, Run, Step, DAY};
use omsi_launcher_lib::company::day::{DayReport, Note};
use omsi_launcher_lib::company::dealer::BuyingMode;
use omsi_launcher_lib::company::plan::{self as pl, BusOf, DayPlan, DayTour, Fill, Problem, Warn, Who};
use omsi_launcher_lib::company::staff::Block;
use omsi_launcher_lib::company::{self as co, Company, GameMode, Tenure};
use serde_json::{json, Value};

/// A dialog's view (`view`: settings, time, confirm, report, away, ask, and the pages' own).
pub fn view(s: &mut Session, what: &str, args: &Value) -> Value {
    match what {
        "settings" => settings(s, args),
        "time" => time(s),
        "confirm" => confirm(s, args),
        "report" => report(s),
        "away" => away(s),
        "ask" => ask(s),
        "vehicle" => super::fleet::vehicle_view(s, args),
        "new" => super::fleet::new_view(s, args),
        "add_line" => super::lines::add_view(s, args),
        "fare" => super::money::fare_view(s, args),
        "fleet_value" => super::age::fleet_value_view(s, args),
        _ => Value::Null,
    }
}

// --- the company's settings ------------------------------------------------------------------------

/// The add-on's name and its note (Trello "Game modes 2/4").
pub const ROSTERFLOW: &str = "RosterFlow – Bus Scheduling & Crew Rostering";
pub const ROSTERFLOW_NOTE: &str = "Advanced: modelled on professional scheduling software that real transport operators use.";

pub const TO_CASUAL: &str = "The company runs its routine by itself: rosters, spare parts, repairs, staff and research, with simple screens and generous money. The planning, the depot, the workshop, the concessions, the research and the contracts pages are left out.";
pub const TO_CASUAL_RF: &str = "RosterFlow is switched off; its plans are kept.";
pub const FROM_CASUAL: &str = "Every page opens and you run the company yourself: the roster, the workshop, the concessions and the research.";
pub const TO_NORMAL: &str = "Forgiving rules and calmer days: fewer illnesses, breakdowns and delays, and the dispatcher repairs the roster every morning.";
pub const TO_REALISTIC: &str = "Real-life wages, prices and rules, and real disruptions: more illness, breakdowns and delays. The roster is no longer repaired by itself: you solve everything yourself.";
pub const KEPT: &str = "Your buses, people, lines, money and history stay as they are, and you can switch back at any time.";

/// What switching the company to `to` changes (`settings::switch_text`).
pub fn switch_text(c: &Company, to: GameMode) -> String {
    use GameMode::*;
    let mut parts: Vec<&str> = Vec::new();
    match (c.game_mode, to) {
        (_, Casual) => {
            parts.push(TO_CASUAL);
            if co::autopilot::rosterflow(c) {
                parts.push(TO_CASUAL_RF);
            }
        }
        (Casual, Normal) => parts.extend([FROM_CASUAL, TO_NORMAL]),
        (Realistic, Normal) => parts.push(TO_NORMAL),
        (Casual, Realistic) => parts.extend([FROM_CASUAL, TO_REALISTIC]),
        (Normal, Realistic) => parts.push(TO_REALISTIC),
        _ => {}
    }
    parts.push(KEPT);
    parts.iter().map(|p| tr(p)).collect::<Vec<_>>().join(" ")
}

fn settings(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let easy = co::autopilot::on(&c);
    let date = args.get("date").and_then(Value::as_str).filter(|d| co::dates::parse(d).is_some()).unwrap_or(&c.date).to_string();
    let logo_text = c.logo.as_deref().map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()).unwrap_or_else(|| tr("No logo picture: the short name is the mark."));
    let on = c.clock.real.on;
    let vehicles = super::vehicles::settings_part(s);
    json!({
        "title": tr("The company's settings"),
        "vehicles": vehicles,
        "mark_caps": tr("The company's mark").to_uppercase(), "mark": mark_json(&c), "logo_text": logo_text,
        "choose_logo": tr("Choose a logo picture"), "remove_logo": c.logo.is_some().then(|| tr("Remove the picture")),
        "mode_caps": tr("Game mode").to_uppercase(), "modes": GameMode::ALL.iter().map(|m| mode_json(*m)).collect::<Vec<_>>(), "mode": super::mode_key(c.game_mode), "mode_info": tr(c.game_mode.info()),
        "addons": c.game_mode.rules().rosterflow.then(|| json!({ "caps": tr("Add-ons").to_uppercase(), "label": tr(ROSTERFLOW), "note": tr(ROSTERFLOW_NOTE), "on": co::autopilot::rosterflow(&c) })),
        "company_mode": super::sandbox::settings_part(&c),
        "country": super::country::settings_part(&c),
        "currency": super::currency::settings_part(&c),
        "buying": (!easy).then(|| {
            let mode = BuyingMode::ALL.iter().position(|m| *m == c.dealer.mode).unwrap_or(1);
            json!({ "caps": tr("Buying buses").to_uppercase(), "modes": BuyingMode::ALL.iter().map(|m| tr(m.label())).collect::<Vec<_>>(), "mode": mode, "say": tr(if mode == 0 { "A model, a number, the list price: the buses are yours at once." } else { "Haggle with the dealer, agree on extras and sign a contract; new buses are delivered." }) })
        }),
        "date_caps": tr("The company's date").to_uppercase(),
        "now": trf("Today it is %{date}, %{time}.", &[("date", &day_label(&c.date)), ("time", &ck::hhmm(ck::now(&c)))]),
        "realtime": { "on": on, "label": tr("Run with the real clock, also while Omsi-Hub is closed"), "say": tr(if on { "When you open the company, the time since is simulated, a month at most. Fast-forwarding moves it ahead of the real clock." } else { "Off: the company's time stands still while it is closed." }) },
        "date": date, "date_label": day_label(&date),
        "earliest": ck::earliest_date(&c).map(|e| trf("Not before %{date}: the company's last booking.", &[("date", &day_label(&e))])),
        "warn": tr("Moving the date changes what the company finds there: the dealer's offers and the used market of that day, deliveries and contracts by their own dates, concessions and leases running to theirs. The days between are not simulated: they bring no money and no wear."),
        "delete": tr("Delete the company"), "close": tr("Close"),
        "move": (date != c.date).then(|| trf("Move to %{date}", &[("date", &day_label(&date))])),
    })
}

/// Run the open company in game mode `mode` from now on (`settings::set_mode`).
fn set_mode(s: &mut Session, mode: GameMode) -> Result<Value, String> {
    if let Err(p) = apply(s, |c| {
        co::autopilot::set_game_mode(c, mode);
        Ok(())
    }) {
        return Ok(json!({ "popup": p }));
    }
    let text = match mode {
        GameMode::Casual => "The company runs in Casual now: it sees to its routine by itself.",
        GameMode::Normal => "The company runs in Normal now: every page is open, and the dispatcher helps.",
        GameMode::Realistic => "The company runs in Realistic now: real-life rules, and you solve everything yourself.",
    };
    s.say(tr(text), false);
    Ok(json!({ "mode": super::mode_key(mode), "pages": super::pages_of(mode) }))
}

// --- the time dialog ---------------------------------------------------------------------------------

fn time(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let in_game = !may_step(s) && !s.closing && c.clock.ask.is_none();
    let steps: [(&str, Step, &str, i64); 9] = [
        ("+15 min", Step::Minutes(15), "minutes", 15),
        ("+1 h", Step::Minutes(60), "minutes", 60),
        ("+6 h", Step::Minutes(360), "minutes", 360),
        ("To the morning", Step::Morning, "morning", 0),
        ("To tomorrow", Step::Midnight, "midnight", 0),
        ("+1 day", Step::Days(1), "days", 1),
        ("+2 days", Step::Days(2), "days", 2),
        ("+3 days", Step::Days(3), "days", 3),
        ("+7 days", Step::Days(7), "days", 7),
    ];
    let steps: Vec<Value> = steps
        .iter()
        .map(|(label, step, kind, n)| {
            let to = ck::target(&c, *step);
            json!({ "label": tr(label), "step": kind, "n": n, "tip": format!("{}  {}", day_label(&ck::date_of(to)), ck::hhmm(to)) })
        })
        .collect();
    let real = c.clock.real.on;
    let mut words = super::realtime_tip(&c).replace('\n', " ");
    if !real {
        words = tr("The company's time stands still while it is closed: the real clock is off in the company's settings.");
    }
    json!({
        "title": trf("Company time: %{now}", &[("now", &now_label(&c))]),
        "in_game": in_game.then(|| tr("The game runs on the company's map: its clock leads, and the company's day goes along with it. Simulating is offered again when the game is closed.")),
        "simulate": tr("Simulate").to_uppercase(), "steps": steps,
        "until": s.clock.until, "until_label": tr("Until"), "until_text": ck::hhmm(s.clock.until as i64), "until_button": trf("Simulate until %{time}", &[("time", &ck::hhmm(s.clock.until as i64))]),
        "run": tr("Run the clock").to_uppercase(),
        "speeds": [[tr(if real { "×1" } else { "Stop" }), 0.0], ["×60", 1.0], ["×600", 10.0]], "speed": s.clock.speed,
        "speed_tip": real.then(|| tr("In step with the real clock")),
        "say": tr(if real { "×1: in step with the real clock. ×60: a minute a second; it stops for what needs you." } else { "×60: a minute a second; it stops for what needs you." }),
        "real": real, "real_words": words,
        "decisions": tr("Decisions").to_uppercase(), "dispatcher": c.clock.dispatcher, "dispatcher_label": tr("The dispatcher decides breakdowns (a rental bus while the cash allows)"),
        "quick": tr("Close the day quickly"), "quick_tip": tr("The rest of the day at once: nothing stops, the dispatcher decides"), "close": tr("Close"),
    })
}

// --- a confirmation ----------------------------------------------------------------------------------

/// A confirmation (`Confirm`): its title, its words, its button (`what`: sell, dismiss,
/// remove_line, delete_company, game_mode, holiday).
fn confirm(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let what = args.get("what").and_then(Value::as_str).unwrap_or("");
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    let name = args.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    let (title, text, button, icon, danger) = match what {
        "sell" => {
            let Some(v) = c.vehicle(id) else { return json!({ "gone": true }) };
            let amount = co::market::sale_offer(&c, v);
            let text = match v.tenure {
                Tenure::Owned { .. } => trf("A dealer pays %{amount} for %{bus}.", &[("amount", &eur(amount)), ("bus", &format!("{} {}", v.number, v.name))]),
                Tenure::Leased { .. } => trf("Giving the leased bus back early costs %{amount} (three monthly rates).", &[("amount", &eur(-amount))]),
                Tenure::Rented { .. } => tr("The rented bus goes back today; the days rented are paid."),
            };
            (format!("{} {}", v.number, v.name), text, tr(if matches!(v.tenure, Tenure::Owned { .. }) { "Sell" } else { "Give back" }), "warning", true)
        }
        "dismiss" => {
            let Some(e) = c.employee(id) else { return json!({ "gone": true }) };
            let r = co::economy::rules_for(&c);
            let mut text = trf("%{name} works %{days} more days (the notice) and then leaves.", &[("name", &e.name), ("days", &r.notice_days.to_string())]);
            if r.severance_months_per_year > 0.0 {
                text.push(' ');
                text.push_str(&tr("They are paid half a month's wage for every year they worked here."));
            }
            (e.name.clone(), text, tr("Dismiss"), "warning", true)
        }
        "remove_line" => {
            let number = c.lines.iter().find(|x| x.name == name).map(|x| x.number.clone()).unwrap_or_default();
            (trf("Line %{n}", &[("n", &number)]), tr("The company stops running this line from today. Its buses and drivers stay."), tr("Stop running it"), "warning", true)
        }
        "delete_company" => {
            let id = if name.is_empty() { c.id.clone() } else { name.clone() };
            let n = s.companies.as_ref().and_then(|cs| cs.iter().find(|x| x.id == id)).map(|x| x.name.clone()).unwrap_or_else(|| c.name.clone());
            (trf("Delete %{name}?", &[("name", &n)]), tr("The company is deleted for good: its buses, staff, lines, money and history. Your service record as a driver stays. This cannot be undone."), tr("Delete for good"), "warning", true)
        }
        "game_mode" => {
            let Some(to) = super::mode_of(&name) else { return json!({ "gone": true }) };
            (trf("Switch to %{mode}?", &[("mode", &tr(to.label()))]), switch_text(&c, to), tr("Switch mode"), mode_icon(to), false)
        }
        // (Beta 61: a career turned into a sandbox, for good)
        "sandbox" => {
            if co::sandbox::is_sandbox(&c) {
                return json!({ "gone": true });
            }
            let (title, text, button) = super::sandbox::confirm_text(&c);
            (title, text, button, "sports_esports", true)
        }
        // (a holiday approved over the limit, Beta 57: `id` is the request's number)
        "holiday" => {
            let Some((title, text)) = super::people::holidays::confirm(&c, id) else { return json!({ "gone": true }) };
            (title, text, tr("Approve anyway"), "warning", false)
        }
        _ => return json!({ "gone": true }),
    };
    json!({ "title": title, "text": text, "button": button, "icon": icon, "danger": danger, "cancel": tr("Cancel") })
}

// --- the report of the days closed --------------------------------------------------------------------

/// A note of the day's report: its words, tone, and the way that mends it (`note_for`, `note_text`).
fn note_text(n: &Note, easy: bool) -> (String, &'static str, Option<Go>) {
    match n {
        Note::Won { number, until } if easy => (trf("The concession for line %{n} is yours until %{date}: the line runs by itself from its first day.", &[("n", number), ("date", &day_label(until))]), "ok", None),
        Note::Breakdown { number, until, cost } => (trf("Bus %{n} broke down on its tour: in the workshop until %{date}, repairs %{amount}.", &[("n", number), ("date", &day_label(until)), ("amount", &eur(*cost))]), "danger", None),
        Note::Service { number } => (trf("Bus %{n} is due for its service: in the workshop tomorrow.", &[("n", number)]), "soft", None),
        Note::Returned { number, name } => (trf("Bus %{n} (%{name}) went back: its lease or rental ended.", &[("n", number), ("name", name)]), "soft", Some(Go::Dealer)),
        Note::LoanPaid { purpose } => (trf("A loan is paid off: %{what}.", &[("what", purpose)]), "ok", None),
        Note::Month { month, result } => (trf("%{month} is closed: wages, leases, insurance, the depot and loan rates are booked. The month's result: %{amount}.", &[("month", &month_label(month)), ("amount", &eur(*result))]), if *result >= 0 { "ok" } else { "warn" }, None),
        Note::Built { area } => (trf("The depot's building work is done: %{what}.", &[("what", &tr(area))]), "ok", None),
        Note::BayWait { number } => (trf("Bus %{n} waits for a free workshop bay.", &[("n", number)]), "warn", Some(Go::Depot)),
        Note::Won { number, until } => (trf("The concession for line %{n} is yours until %{date}. Plan its tours and start its service.", &[("n", number), ("date", &day_label(until))]), "ok", Some(Go::Planning)),
        Note::Lost { number, winner } => (trf("%{who} won the tender for line %{n}.", &[("n", number), ("who", winner)]), "warn", None),
        Note::Ended { number } => (trf("The concession for line %{n} has ended: the line is no longer yours.", &[("n", number)]), "danger", None),
        Note::NotStarted { number, days, charge } => (trf("Line %{n} is still not in service, %{days} days after it was taken on: the authority charged %{amount}. Plan its tours and start its service.", &[("n", number), ("days", &days.to_string()), ("amount", &eur(*charge))]), "danger", Some(Go::Planning)),
        Note::Accident { number, cost } => (trf("Bus %{n} had an accident on its tour: damage %{amount}.", &[("n", number), ("amount", &eur(*cost))]), "danger", Some(Go::Training)),
        Note::Unsaved => (tr("RosterFlow had changes of this day that were not saved: the day ran the saved blocks and duties. Save them in RosterFlow (or discard them) so the day runs what it shows."), "warn", Some(Go::Planning)),
    }
}

fn staff_text(n: &co::staff::StaffNote) -> (String, &'static str, Option<Go>) {
    use co::staff::StaffNote as S;
    match n {
        S::Sick { name, until } => (trf("%{name} is ill until %{date}.", &[("name", name), ("date", &day_label(until))]), "warn", Some(Go::Planning)),
        S::Holiday { name, until } => (trf("%{name} is on holiday until %{date}.", &[("name", name), ("date", &day_label(until))]), "soft", None),
        S::Resigned { name, until } => (trf("%{name} has resigned; their last day is %{date}.", &[("name", name), ("date", &day_label(until))]), "danger", Some(Go::Hire)),
        S::Unhappy { name } => (trf("%{name} is unhappy with their pay or their hours.", &[("name", name)]), "warn", None),
        S::Left { name } => (trf("%{name} has left the company.", &[("name", name)]), "soft", Some(Go::Hire)),
    }
}

/// The report of the day (or days) just closed (`report_dialog`).
fn report(s: &mut Session) -> Value {
    let Some(reports) = s.reports.clone() else { return Value::Null };
    let (Some(last), Some(c)) = (reports.last().cloned(), s.company.clone()) else { return Value::Null };
    let first = reports.first().cloned().unwrap_or_default();
    let title = if reports.len() > 1 { format!("{} – {}", day_label(&first.date), day_label(&last.date)) } else { day_label(&last.date) };
    let sum = |f: &dyn Fn(&DayReport) -> i64| reports.iter().map(f).sum::<i64>();
    let result = sum(&|r| r.result);
    let income = sum(&|r| r.income);
    let expenses = sum(&|r| r.expenses);
    let tours = sum(&|r| r.tours as i64);
    let covered = sum(&|r| r.covered as i64);
    let trips = sum(&|r| r.trips as i64);
    let dropped = sum(&|r| r.dropped as i64);
    let late = sum(&|r| r.late as i64);
    let pax = sum(&|r| r.passengers as i64);
    let km: f64 = reports.iter().map(|r| r.km).sum();
    let measured = sum(&|r| r.measured as i64);
    let penalties = sum(&|r| r.penalties);
    let uncovered = sum(&|r| r.uncovered as i64);
    let short_buses = sum(&|r| r.short_buses as i64);
    let short_drivers = sum(&|r| r.short_drivers as i64);
    let rep: f64 = reports.iter().map(|r| r.reputation_change).sum();
    let mut kinds: Vec<(co::BookingKind, i64)> = Vec::new();
    for b in c.ledger.iter().filter(|b| b.date >= first.date && b.date <= last.date && !b.kind.is_capital()) {
        match kinds.iter_mut().find(|k| k.0 == b.kind) {
            Some(k) => k.1 += b.amount,
            None => kinds.push((b.kind, b.amount)),
        }
    }
    kinds.retain(|k| k.1 != 0);
    kinds.sort_by_key(|k| std::cmp::Reverse(k.1));
    let (inc, _) = signed(income);
    let (out, _) = signed(-expenses);
    let punct = if trips - dropped > 0 { format!("{:.0} %", 100.0 * (trips - dropped - late).max(0) as f64 / (trips - dropped) as f64) } else { "–".into() };
    let figures = vec![
        json!({ "label": tr("Result"), "value": signed(result).0, "under": format!("{} {}  ·  {} {}", inc, tr("in"), out, tr("out")), "tone": if result >= 0 { "ok" } else { "danger" }, "tip": kinds.iter().map(|(k, a)| format!("{}: {}", tr(k.label()), signed(*a).0)).collect::<Vec<_>>().join("\n") }),
        json!({ "label": tr("Tours run"), "value": format!("{covered} / {tours}"), "under": trf("%{n} trips dropped", &[("n", &dropped.to_string())]), "tone": if covered == tours { "ok" } else { "warn" } }),
        json!({ "label": tr("On time"), "value": punct, "under": trf("%{n} trips late", &[("n", &late.to_string())]), "tone": "text" }),
        json!({ "label": tr("Passengers"), "value": grouped(pax as f64), "under": format!("{} km", grouped(km.round())), "tone": "text" }),
    ];
    // what happened
    let mut facts: Vec<(String, &'static str, Vec<Go>)> = Vec::new();
    if uncovered > 0 || dropped > 0 {
        let mut fixes = Vec::new();
        if short_buses > 0 {
            fixes.push(Go::Dealer);
        }
        if short_drivers > 0 {
            fixes.push(Go::Hire);
        }
        fixes.push(Go::Planning);
        let text = if uncovered > 0 {
            trf("%{trips} trips dropped: %{tours} tours were not covered - %{buses} without a bus, %{duties} duties without a driver.", &[("trips", &dropped.to_string()), ("tours", &uncovered.to_string()), ("buses", &short_buses.to_string()), ("duties", &short_drivers.to_string())])
        } else {
            trf("%{trips} trips dropped: a bus broke down, or a driver came late.", &[("trips", &dropped.to_string())])
        };
        facts.push((text, "danger", fixes));
    }
    if penalties > 0 {
        facts.push((trf("Contract penalties %{amount}: the authority charges for every trip dropped or late, and for a concession not started.", &[("amount", &signed(-penalties).0)]), "warn", vec![Go::Planning]));
    }
    if measured > 0 {
        facts.push((trf("You drove %{n} trips yourself: booked as measured.", &[("n", &measured.to_string())]), "accent2", Vec::new()));
    }
    let crowded = sum(&|r| r.crowded as i64);
    if crowded > 0 {
        let left = sum(&|r| r.left_behind as i64);
        facts.push((trf("%{n} trips of your own lines were full; %{p} passengers were left at the stop. Bigger buses help.", &[("n", &crowded.to_string()), ("p", &left.to_string())]), "warn", vec![Go::Dealer]));
    }
    let fines = sum(&|r| r.incidents.fines as i64);
    if fines > 0 {
        let cost = sum(&|r| r.incidents.fine_cost);
        facts.push((trf("Your drivers were fined %{n} times in traffic: %{amount}. Defensive driving halves it.", &[("n", &fines.to_string()), ("amount", &eur(cost))]), "warn", vec![Go::Training]));
    }
    let complaints = sum(&|r| r.incidents.complaints as i64);
    if complaints > 0 {
        facts.push((trf("%{n} passengers complained about their trip. The customer service course halves the complaints.", &[("n", &complaints.to_string())]), "warn", vec![Go::Training]));
    }
    let ill = sum(&|r| r.incidents.taken_ill as i64);
    if ill > 0 {
        let helped = sum(&|r| r.incidents.helped as i64);
        facts.push((trf("%{n} passengers were taken ill or hurt on board; a first-aider was at the wheel for %{m} of them.", &[("n", &ill.to_string()), ("m", &helped.to_string())]), if helped == ill { "soft" } else { "warn" }, if helped == ill { Vec::new() } else { vec![Go::Training] }));
    }
    if rep.abs() >= 0.05 {
        let t = if rep > 0.0 { "Your reputation rose to %{n}: trips on time and passengers carried." } else { "Your reputation fell to %{n}: trips dropped and late cost it." };
        facts.push((trf(t, &[("n", &num(last.reputation, 1))]), if rep > 0.0 { "ok" } else { "warn" }, Vec::new()));
    }
    let easy = co::autopilot::on(&c);
    for r in &reports {
        for (text, tone, go) in r.notes.iter().map(|n| note_text(n, easy)).chain(r.staff.iter().map(staff_text)) {
            facts.push((text, tone, go.into_iter().collect()));
        }
    }
    if tours == 0 {
        let unplanned = reports.iter().flat_map(|r| r.lines.iter()).any(|x| x.unplanned);
        let text = if unplanned && easy {
            tr("No tours ran: the company's lines were not in service yet. They go into service by themselves the next morning.")
        } else if unplanned {
            tr("No tours ran: the company's lines are not in service yet. Plan their tours and start their service on the Planning page.")
        } else {
            tr("No tours ran: add lines on the Lines page, and buy buses and hire drivers for them.")
        };
        facts.insert(0, (text, "soft", vec![if unplanned { Go::Planning } else { Go::Lines }]));
    }
    if easy {
        for f in facts.iter_mut() {
            f.2 = f.2.iter().filter_map(|g| super::casual::popup_go(*g)).collect();
        }
    }
    // the lines, each with its money
    #[derive(Default)]
    struct LineSum {
        line: String,
        number: String,
        tours: u32,
        covered: u32,
        dropped: u32,
        km: f64,
        passengers: u32,
        fares: i64,
        compensation: i64,
        running: i64,
        penalty: i64,
        unplanned: bool,
    }
    let mut lines_by: Vec<LineSum> = Vec::new();
    for r in &reports {
        for ld in &r.lines {
            let i = match lines_by.iter().position(|x| x.line == ld.line) {
                Some(i) => i,
                None => {
                    lines_by.push(LineSum { line: ld.line.clone(), number: ld.number.clone(), unplanned: true, ..Default::default() });
                    lines_by.len() - 1
                }
            };
            let x = &mut lines_by[i];
            x.tours += ld.tours;
            x.covered += ld.covered;
            x.dropped += ld.dropped;
            x.km += ld.km;
            x.passengers += ld.passengers;
            if ld.fares == 0 && ld.compensation == 0 {
                x.fares += ld.revenue;
            } else {
                x.fares += ld.fares;
                x.compensation += ld.compensation;
            }
            x.running += ld.running;
            x.penalty += ld.penalty;
            x.unplanned &= ld.unplanned;
        }
    }
    let open = s.report_line.clone();
    let lines: Vec<Value> = lines_by
        .iter()
        .map(|x| {
            let cl = c.lines.iter().find(|cl| cl.name == x.line);
            let text = if x.unplanned && x.tours == 0 { tr("Not in service: its tours did not run") } else { trf("%{c} of %{t} tours  ·  %{d} trips dropped  ·  %{p} passengers", &[("c", &x.covered.to_string()), ("t", &x.tours.to_string()), ("d", &x.dropped.to_string()), ("p", &grouped(x.passengers as f64))]) };
            let net = x.fares + x.compensation - x.running - x.penalty;
            let (t, tone) = signed(net);
            let is_open = open.as_deref() == Some(x.line.as_str());
            let per_km = if x.km > 0.0 { x.compensation as f64 / x.km } else { 0.0 };
            json!({
                "line": x.line, "plate": super::lines::plate(&cl.map(|l| l.number.clone()).unwrap_or_else(|| x.number.clone()), cl.map(|l| l.colour.as_str()).unwrap_or("")),
                "text": text, "net": format!("{t} {}", tr("for the line")), "tone": tone, "open": is_open,
                "rows": is_open.then(|| json!([
                    [trf("Fares: %{p} passengers", &[("p", &grouped(x.passengers as f64))]), signed(x.fares).0, signed(x.fares).1],
                    [trf("The authority's payment: %{km} km at %{rate} a km", &[("km", &grouped(x.km)), ("rate", &eur_cents(per_km))]), signed(x.compensation).0, signed(x.compensation).1],
                    [tr("Running costs: fuel or power, maintenance"), signed(-x.running).0, signed(-x.running).1],
                    [tr("Penalties: trips dropped and late"), signed(-x.penalty).0, signed(-x.penalty).1],
                ])),
                "note": is_open.then(|| tr("Wages, insurance, leases and the depot are booked for the whole company at the month's end.")),
            })
        })
        .collect();
    // the trips dropped, each with why and the way to it in the planning (#96)
    let drops: Vec<Value> = reports.iter().flat_map(|r| r.drops.iter().map(move |d| (r.date.clone(), d))).map(|(date, d)| drop_json(&c, &date, d)).collect();
    let more = drops.len().saturating_sub(DROPS_SHOWN);
    json!({
        "title": title, "figures": figures,
        "drops_caps": tr("Trips dropped").to_uppercase(), "drops_hint": tr("Click one to see it in the planning"),
        "drops": drops.into_iter().take(DROPS_SHOWN).collect::<Vec<_>>(),
        "drops_more": (more > 0).then(|| trf("and %{n} more", &[("n", &more.to_string())])),
        // (late running: the trips with their causes, punctuality, deductions, advice)
        "late": super::late::report_part(&c, &reports),
        "money_caps": tr("Money").to_uppercase(), "money": kinds.iter().map(|(k, a)| { let (t, tone) = signed(*a); json!([tr(k.label()), t, tone]) }).collect::<Vec<_>>(),
        "lines_caps": tr("Lines").to_uppercase(), "lines_hint": tr("Click a line for its money"), "lines": lines,
        "facts_caps": tr("What happened").to_uppercase(),
        "facts": facts.iter().map(|(t, tone, fixes)| json!({ "text": t, "tone": tone, "fixes": fixes.iter().map(|g| g.json()).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "close": tr("Close the report"),
    })
}

/// The report lists this many runs of trips dropped (the rest: "and n more").
const DROPS_SHOWN: usize = 40;

/// A run of trips dropped (`day::Dropped`) as the report lists it: its line, block and times, why
/// (a problem's words where there is one), and where it opens in the planning - RosterFlow's
/// Roster tab on that weekday with the tour or duty chosen, or its Vehicles tab with the block
/// shown (the weekday's next day in the week the planning shows).
fn drop_json(c: &co::Company, date: &str, d: &co::day::Dropped) -> Value {
    use co::day::DropWhy;
    let why = match d.problem {
        Some(p) if p != co::plan::Problem::Unassigned => format!("{}: {}", tr(d.why.label()), tr(p.label())),
        Some(_) => format!("{}: {}", tr(d.why.label()), tr("none planned")),
        // (a bus broken down: when the bus sent for it ran its trips - only those before it came,
        // or after it had to go, are lost)
        None => match d.sent {
            Some((from, Some(until))) => format!("{}: {}", tr(d.why.label()), trf("a bus sent ran its trips from %{from} until %{until}", &[("from", &ck::hhmm(from as i64)), ("until", &ck::hhmm(until as i64))])),
            Some((from, None)) => format!("{}: {}", tr(d.why.label()), trf("a bus sent ran its trips from %{from}", &[("from", &ck::hhmm(from as i64))])),
            None => tr(d.why.label()),
        },
    };
    // (the block as RosterFlow names it: "1/02")
    let tour = if d.tour.len() == 1 && d.tour.chars().all(|c| c.is_ascii_digit()) { format!("0{}", d.tour) } else { d.tour.clone() };
    let block = if d.tour.is_empty() { String::new() } else { format!("{}/{}", d.number, tour) };
    let hm = |m: i32| ck::hhmm(m as i64);
    let words = if block.is_empty() {
        trf("Line %{n}  ·  %{from}–%{to}  ·  %{k} trips", &[("n", &d.number), ("from", &hm(d.from)), ("to", &hm(d.to)), ("k", &d.trips.to_string())])
    } else {
        trf("Line %{n}, vehicle %{block}  ·  %{from}–%{to}  ·  %{k} trips", &[("n", &d.number), ("block", &block), ("from", &hm(d.from)), ("to", &hm(d.to)), ("k", &d.trips.to_string())])
    };
    let wd = co::plan::weekday_of(date);
    let day = (0..7).find(|k| co::plan::weekday_of(&co::dates::add(&c.date, *k)) == wd).unwrap_or(0);
    let link = match (d.why, d.duty) {
        (DropWhy::NoBus, _) => json!({ "view": 0, "day": day, "sel": { "tour": [d.line, d.tour] } }),
        (DropWhy::NoDriver, Some(k)) => json!({ "view": 0, "day": day, "sel": { "duty": [d.line, d.tour, k] } }),
        (DropWhy::NoBlock, _) => json!({ "view": 1, "day": day }),
        _ => json!({ "view": 1, "day": day, "reveal": format!("B|{}|{}", d.line, d.tour) }),
    };
    let tone = if d.why == DropWhy::NoBlock || d.problem == Some(co::plan::Problem::Unassigned) { "warn" } else { "danger" };
    // (its bus and driver, or the one missing that was planned and why it could not come)
    let why = if d.bus.is_some() || d.driver.is_some() { super::late::dropped_text(c, d.why, d.problem, d.bus, d.driver) } else { why };
    let words = if d.trip.trim().is_empty() { words } else { format!("{words}  ·  {}", trf("trip %{id}", &[("id", d.trip.trim())])) };
    let duty = d.duty.map(|k| trf("Duty %{n}", &[("n", &(k + 1).to_string())]));
    json!({ "text": words, "why": why, "tone": tone, "link": link, "date": day_label(date), "duty": duty, "bus": super::late::bus_json(c, d.bus), "driver": super::late::driver_json(c, d.driver) })
}

/// What happened while the player was away (`clock::away_dialog`).
fn away(s: &mut Session) -> Value {
    let (Some(a), Some(c)) = (s.clock.away.clone(), s.company.clone()) else { return Value::Null };
    let k = &a.caught;
    let at = |m: i64| format!("{}  {}", day_label(&ck::date_of(m)), ck::hhmm(m));
    let n = (ck::CATCH_UP_MAX / DAY).to_string();
    let (res, res_tone) = signed(k.result);
    json!({
        "title": tr("While you were away"),
        "ran": trf("The company's time ran on with the real clock: %{span}, from %{from} to %{until}. The dispatcher decided what came up.", &[("span", &span_text(k.to - k.from)), ("from", &at(k.from)), ("until", &at(k.to))]),
        "skipped": (k.skipped > 0).then(|| trf("You were away for longer than %{n} days: the company caught up %{n} days, and the rest was skipped.", &[("n", &n)])),
        "figures": [
            { "label": tr("Days closed"), "value": k.days.to_string(), "under": "", "tone": "text" },
            { "label": tr("Result"), "value": res, "under": "", "tone": if k.days == 0 { "dim" } else { res_tone } },
            { "label": tr("Cash"), "value": super::sandbox::cash(&c), "under": trf("%{amount} while you were away", &[("amount", &signed(c.cash - a.cash_before).0)]), "tone": super::sandbox::cash_tone(&c) },
        ],
        "caps": tr("What happened").to_uppercase(),
        "none": tr("Nothing out of the ordinary: the tours ran as planned. The feed and the inbox have every line."),
        "notable": a.notable.iter().rev().map(|f| json!({ "when": at(f.at), "text": feed_text(f), "tone": feed_tone(f.level) })).collect::<Vec<_>>(),
        "report": (!a.reports.is_empty()).then(|| tr("The days' report")), "go": tr("Go on"),
    })
}

// --- the clock's questions -------------------------------------------------------------------------

fn line_title(t: &DayTour, duty: Option<usize>) -> String {
    let mut s = format!("{} {}  ·  {} {}", tr("Line"), t.tour.number, tr("Tour"), t.tour.tour);
    if let Some(k) = duty {
        s.push_str(&format!("  ·  {} {}", tr("Duty"), k + 1));
    }
    s
}

fn length(minutes: i32) -> String {
    format!("{} h {:02}", minutes / 60, minutes % 60)
}

fn hhmm(minutes: i32) -> String {
    co::clock::hhmm(minutes as i64)
}

fn first_name(c: &Company, id: u32) -> String {
    c.employee(id).map(|e| e.name.split_whitespace().next().unwrap_or(&e.name).to_string()).unwrap_or_else(|| "?".into())
}

fn who_name(c: &Company, w: Option<Who>) -> String {
    match w {
        Some(Who::Staff(id)) => c.employee(id).map(|e| e.name.clone()).unwrap_or_else(|| "?".into()),
        Some(Who::Player) => tr("You"),
        Some(Who::Agency) => tr("Agency driver"),
        Some(Who::Guest(g)) => omsi_launcher_lib::company::together::guest_name(c, g),
        None => tr("Nobody"),
    }
}

/// Why a driver cannot take a duty, in words (`planning::cannot_text`).
fn cannot_text(c: &Company, p: &DayPlan, x: &pl::Cannot) -> String {
    use pl::Cannot as N;
    match x {
        N::NotDriver => tr("Not a driver"),
        N::NotEmployed => tr("Not employed then"),
        N::Ill { until } => trf("Ill until %{date}", &[("date", &day_label(until))]),
        N::Holiday { until } => trf("On holiday until %{date}", &[("date", &day_label(until))]),
        N::WeekDone { days } => trf("Worked %{n} days this week already", &[("n", &days.to_string())]),
        N::Lacks(l) => super::people::lack_text(c, l),
        N::OnDuty { key, from, to } => {
            let mut it = key.split('|');
            let (line, tour) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            let number = p.tours.iter().find(|t| t.tour.line == line && t.tour.tour == tour).map(|t| t.tour.number.clone()).unwrap_or_else(|| line.to_string());
            trf("Drives line %{n} tour %{tour} then (%{from}–%{to})", &[("n", &number), ("tour", tour), ("from", &hhmm(*from)), ("to", &hhmm(*to))])
        }
        N::Rest { minutes } => trf("Only %{n} min between duties: %{min} min rest needed", &[("n", &minutes.to_string()), ("min", &co::staff::REST.to_string())]),
        N::NightRest { from } => trf("Night's rest after yesterday: free from %{time}", &[("time", &hhmm(*from))]),
        N::TooLong { minutes } => trf("Working day would be %{t}: %{max} at most", &[("t", &length(*minutes)), ("max", &length(co::staff::DAY_MAX))]),
    }
}

/// A way to cover an open duty in words (`planning::cover_text`).
fn cover_text(c: &Company, x: &pl::Cover) -> (String, String) {
    match x {
        pl::Cover::Colleague { id, standby, warn } => {
            let name = c.employee(*id).map(|e| e.name.clone()).unwrap_or_default();
            let note = match warn.first() {
                Some(Warn::Overtime(m)) => trf("Overtime %{t}", &[("t", &length(*m))]),
                Some(Warn::Transfer(m)) => trf("%{n} min to change stops", &[("n", &m.to_string())]),
                Some(Warn::Experience) => tr("Little experience for this bus"),
                None if *standby => tr("Standby: free all day"),
                None => tr("Can take it"),
            };
            (name, note)
        }
        pl::Cover::Agency { used } => (trf("Agency driver (%{n} sent today: as many as you ask for)", &[("n", &used.to_string())]), String::new()),
        pl::Cover::Player => (tr("Drive it yourself"), String::new()),
        pl::Cover::Drop { penalty } if *penalty > 0 => (tr("Drop its trips today"), trf("Penalty %{amount}", &[("amount", &eur(*penalty))])),
        pl::Cover::Drop { .. } => (tr("Drop its trips today"), tr("No penalty")),
    }
}

/// The ways to run a tour without its bus today (`planning::tour_covers`).
fn tour_covers(c: &Company, p: &DayPlan, ti: usize) -> Vec<(String, String, Option<Fill>)> {
    let t = &p.tours[ti];
    let mut out = Vec::new();
    for id in pl::spare_buses(c, p, ti) {
        let Some(v) = c.vehicle(id) else { continue };
        let note = if t.tour.wants().is_some_and(|w| w != v.kind.size) { tr("Another size than the tour asks for") } else { tr("Free then") };
        out.push((trf("Spare bus %{bus}", &[("bus", &format!("{} {}", v.number, v.name))]), note, Some(Fill::Bus { id })));
    }
    let (rent, ok) = pl::rental_for(c, p, ti);
    if ok {
        out.push((trf("Rent a bus for today (%{amount})", &[("amount", &eur(rent))]), String::new(), Some(Fill::Rental)));
    } else {
        out.push((trf("No rental bus: no place in the depot or not the cash (%{amount})", &[("amount", &eur(rent))]), String::new(), None));
    }
    let penalty = pl::drop_penalty(c, &t.tour, 0, t.tour.trips.len());
    let note = if penalty > 0 { trf("Penalty %{amount}", &[("amount", &eur(penalty))]) } else { tr("No penalty") };
    out.push((tr("Drop the tour today"), note, Some(Fill::Drop)));
    out
}

fn blocks_without(p: &DayPlan, id: u32, skip: (usize, usize)) -> Vec<Block> {
    p.duties_of(Who::Staff(id))
        .into_iter()
        .filter(|x| *x != skip)
        .map(|(i, k)| {
            let t = &p.tours[i];
            let d = &t.duties[k];
            Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
        })
        .collect()
}

fn block_of(t: &DayTour, k: usize) -> Block {
    let d = &t.duties[k];
    Block { key: pl::duty_key(&t.tour.line, &t.tour.tour, k), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() }
}

/// A duty's problem in words (`planning::problem_text`).
fn problem_text(c: &Company, p: &DayPlan, ti: usize, k: usize, problem: Option<Problem>) -> String {
    let t = &p.tours[ti];
    if matches!(problem, Some(Problem::Away | Problem::Conflict)) {
        let rostered = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten());
        if let Some(e) = rostered.and_then(|w| if let Who::Staff(id) = w { c.employee(id) } else { None }) {
            let work = blocks_without(p, e.id, (ti, k));
            if let Err(x) = pl::can_take(e, &p.date, p.today, &t.bus_of(c), &work, &block_of(t, k)) {
                return format!("{}: {}", first_name(c, e.id), cannot_text(c, p, &x));
            }
        }
    }
    if matches!(problem, Some(Problem::Licence | Problem::NoEndorsement | Problem::NoTypeTraining)) {
        let rostered = pl::roster(c, p.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.duties.get(k).copied().flatten());
        if let Some(e) = rostered.and_then(|w| if let Who::Staff(id) = w { c.employee(id) } else { None }) {
            let (kind, bus) = t.bus_of(c);
            if let Some(x) = co::licences::lack(e, kind, bus.as_deref()) {
                return format!("{}: {}", first_name(c, e.id), super::people::lack_text(c, &x));
            }
        }
    }
    tr(problem.map(Problem::label).unwrap_or("No driver"))
}

/// What fell out suddenly today, with every way to cover each (`planning::morning_rows`; None: the
/// day's timetable is still being read).
fn morning_rows(s: &mut Session, c: &Company) -> Option<Vec<Value>> {
    week_work(s);
    let p = plan_of(s, &c.date)?.ok()?;
    let chosen = |key: &str| c.planning.fills.iter().find(|f| c.planning.date == c.date && f.key == key).map(|f| f.fill);
    let mut out = Vec::new();
    for o in p.open_rows() {
        if o.late || !o.problem.is_some_and(Problem::sudden) || o.problem == Some(Problem::Player) {
            continue;
        }
        let t = &p.tours[o.tour];
        let (title, options): (String, Vec<(String, Option<Fill>)>) = match o.duty {
            None => {
                let title = format!("{}  ·  {}", line_title(t, None), tr(o.problem.map(Problem::label).unwrap_or("No bus")));
                let now = match t.bus {
                    Some(BusOf::Own(id)) if chosen(&o.key).is_none() => c.vehicle(id).map(|v| v.number.clone()).unwrap_or_default(),
                    Some(BusOf::Rental) if chosen(&o.key).is_none() => tr("Rental bus"),
                    _ => tr("Trips dropped"),
                };
                let mut options = vec![(trf("As the central has it: %{what}", &[("what", &now)]), None)];
                for (label, note, fill) in tour_covers(c, &p, o.tour) {
                    if fill.is_some() {
                        options.push((if note.is_empty() { label } else { format!("{label}  ·  {note}") }, fill));
                    }
                }
                (title, options)
            }
            Some(k) => {
                let d = &t.duties[k];
                let title = format!("{}  ·  {}", line_title(t, Some(k)), problem_text(c, &p, o.tour, k, o.problem));
                let now = if d.who.is_some() && chosen(&o.key).is_none() { who_name(c, d.who) } else { tr("Trips dropped") };
                let mut options = vec![(trf("As the central has it: %{what}", &[("what", &now)]), None)];
                for cv in pl::covers_of_duty(c, &p, o.tour, k) {
                    let (label, note) = cover_text(c, &cv);
                    options.push((if note.is_empty() { label } else { format!("{label}  ·  {note}") }, Some(cv.fill())));
                }
                (title, options)
            }
        };
        let at = chosen(&o.key).and_then(|f| options.iter().position(|x| x.1 == Some(f))).unwrap_or(0);
        out.push(json!({ "key": o.key, "title": title, "options": options.iter().map(|(l, f)| json!([l, f])).collect::<Vec<_>>(), "at": at }));
    }
    Some(out)
}

/// A spare bus to send for a breakdown, as its button says it: from when, and until when it
/// can run before its own tour (`clock::spare_offers`).
pub fn spare_label(bus: &str, from: &str, until: Option<i32>) -> String {
    match until {
        Some(u) => trf("Send spare bus %{bus} (from %{time} until %{until}, no extra cost)", &[("bus", bus), ("time", from), ("until", &ck::hhmm(u as i64))]),
        None => trf("Send spare bus %{bus} (from %{time}, no extra cost)", &[("bus", bus), ("time", from)]),
    }
}

/// The question the clock waits for (`clock::ask_dialog`).
fn ask(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let Some(question) = c.clock.ask.clone() else { return Value::Null };
    let when = ck::hhmm(ck::now(&c));
    match &question {
        Ask::Breakdown { index } => {
            let Some(b) = c.clock.today.as_ref().and_then(|x| x.breaks.get(*index)).cloned() else { return json!({ "kind": "gone" }) };
            let cost = ck::rental_cost(&c, *index);
            let (spare_from, rental_from) = ck::arrivals(&c, *index);
            let back = ck::hhmm(ck::moment(&c.date, rental_from as i64));
            let spares = ck::spare_offers(&c, *index);
            let spare_at = ck::hhmm(ck::moment(&c.date, spare_from as i64));
            // (an accident: the same choices, its damage repaired in the workshop)
            let first = if b.accident { trf("It had an accident on line %{n}: its damage is repaired in the workshop.", &[("n", &b.line)]) } else { trf("It was on line %{n}; it goes to the workshop.", &[("n", &b.line)]) };
            let rest = if spares.is_empty() {
                trf("No spare bus of the fleet is free now. A rental bus can take over its trips from %{time} for %{amount}; else its trips are dropped for the rest of the day, with the contract's penalty.", &[("time", &back), ("amount", &eur(cost))])
            } else {
                trf("A spare bus of the fleet can take over its trips from %{spare}; a rental bus from %{time} for %{amount}; else its trips are dropped for the rest of the day, with the contract's penalty.", &[("spare", &spare_at), ("time", &back), ("amount", &eur(cost))])
            };
            let text = format!("{first} {rest}");
            let shown: Vec<Value> = spares.iter().take(6).filter_map(|x| c.vehicle(x.vehicle).map(|v| (x, v))).map(|(x, v)| json!({ "id": v.id, "label": spare_label(&format!("{} {}", v.number, v.name), &spare_at, x.until) })).collect();
            let title = if b.accident { "%{time}  Bus %{bus} had an accident" } else { "%{time}  Bus %{bus} broke down" };
            json!({
                "kind": "breakdown", "icon": "warning", "closable": false, "title": trf(title, &[("time", &when), ("bus", &b.bus)]), "text": text,
                "spares_caps": (!shown.is_empty()).then(|| tr("Spare buses free now").to_uppercase()), "spares": shown,
                "dispatcher": c.clock.dispatcher, "dispatcher_label": tr("From now on the dispatcher decides"),
                "rental": trf("Rental bus, %{amount}", &[("amount", &eur(cost))]), "drop": tr("Drop its trips"),
            })
        }
        Ask::Tender { id, outbid } => {
            let Some(t) = c.concessions.tenders.iter().find(|t| t.id == *id).cloned() else { return json!({ "kind": "gone" }) };
            let left = (t.closes_at - ck::now(&c)).max(0);
            let mut text = trf("Bids until %{time} (%{left} from now), scored 70 % price and 30 % quality. Ask at most %{amount} to lead now; buying it outright: %{buy}.", &[("time", &ck::hhmm(t.closes_at)), ("left", &format!("{}:{:02} h", left / 60, left % 60)), ("amount", &eur(co::concessions::min_bid(&c, &t))), ("buy", &eur(t.buy_out()))]);
            if *outbid {
                if let Some(p) = co::concessions::bids(&c, &t, ck::now(&c)).last() {
                    text = format!("{}  {}", trf("%{who} bids %{amount}.", &[("who", &co::concessions::bidder_name(&c, &t, p.who)), ("amount", &eur(p.amount))]), text);
                }
            }
            json!({
                "kind": "tender", "icon": "payments", "closable": true,
                "title": trf(if *outbid { "%{time}  Your bid for line %{n} is beaten" } else { "%{time}  Line %{n} is out to tender" }, &[("time", &when), ("n", &t.number)]),
                "text": text, "auction": super::pages_of(c.game_mode).contains(&CONCESSIONS_TAB).then(|| tr("To the auction")), "on": tr(if *outbid { "Let it go" } else { "Go on" }),
            })
        }
        Ask::Morning { count } => {
            let rows = morning_rows(s, &c).unwrap_or_default();
            let text = if rows.is_empty() {
                trf("%{n} things fell out this morning: drivers late, buses that did not start. The central fills them as it can; the planning shows them and lets you choose.", &[("n", &count.to_string())])
            } else {
                trf("%{n} things fell out this morning: drivers ill or late, buses that did not start. Choose how to cover each, or leave it to the central.", &[("n", &count.to_string())])
            };
            json!({
                "kind": "morning", "icon": "wb_twilight", "closable": true, "title": trf("%{time}  The morning", &[("time", &when)]), "text": text,
                "rows": rows.into_iter().take(6).collect::<Vec<_>>(), "planning": super::pages_of(c.game_mode).contains(&5).then(|| tr("Open the planning")), "on": tr("Go on"),
            })
        }
        Ask::Notice { text, args } => {
            let item = FeedItem { at: ck::now(&c), text: text.clone(), args: args.clone(), level: Level::Bad };
            json!({ "kind": "notice", "icon": "info", "closable": true, "title": when, "text": feed_text(&item), "on": tr("Go on") })
        }
    }
}

/// The answer given (`clock::finish`): applied, and the simulation goes on where it was going
/// (`on`), unless the clock plays or the player went to look.
fn finish(s: &mut Session, choice: Option<Choice>, on: bool) {
    let Some(c) = s.company.as_mut() else { return };
    let mut run = Run::default();
    let to = ck::answer(c, choice, &mut run);
    super::changed(s);
    if on && s.clock.speed == 0.0 {
        if let Some(to) = to {
            simulate(s, to, false);
        }
    }
}

/// What the dialogs do.
pub fn act(s: &mut Session, what: &str, args: &Value, _p: &dyn crate::Platform) -> Option<Result<Value, String>> {
    let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
    let str_of = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    let done = |r: Result<(), serde_json::Value>| Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})));
    Some(match what {
        // the settings
        "logo" => {
            // (the page's file picker chose it: the window's own, on its thread)
            let picked = args.get("path").and_then(Value::as_str).filter(|p| !p.trim().is_empty()).map(str::to_string);
            match picked {
                Some(p) => done(apply(s, |c| {
                    c.logo = Some(p);
                    Ok(())
                })),
                None => Ok(json!({})),
            }
        }
        "currency" => super::currency::set(s, &str_of("code")),
        "logo_remove" => done(apply(s, |c| {
            c.logo = None;
            Ok(())
        })),
        "rosterflow" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
            match apply(s, |c| Ok(co::autopilot::set_rosterflow(c, on))) {
                Ok(now) => {
                    let t = tr(if now { "RosterFlow is on: the planning is its studio now." } else { "RosterFlow is off: the planning is the day's roster. Its plans are kept." });
                    s.say(t, false);
                    Ok(json!({ "rosterflow": now }))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "realtime" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
            done(apply(s, |c| {
                ck::set_realtime(c, on);
                Ok(())
            }))
        }
        "move_date" => {
            let to = str_of("date");
            match apply(s, |c| ck::move_to(c, &to)) {
                Ok(()) => {
                    s.plan = None;
                    let t = trf("The company is at %{date} now.", &[("date", &day_label(&to))]);
                    s.say(t, false);
                    Ok(json!({ "moved": true }))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // a confirmation answered
        "confirmed" => match str_of("what").as_str() {
            "sell" => done(apply(s, |c| co::market::sell(c, id)).map(|_| ())),
            "dismiss" => match apply(s, |c| co::staff::dismiss(c, id)) {
                Ok(until) => {
                    let t = trf("Their last day is %{date}.", &[("date", &day_label(&until))]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            },
            "remove_line" => {
                let name = str_of("name");
                done(apply(s, |c| {
                    co::network::remove_line(c, &name);
                    Ok(())
                }))
            }
            "game_mode" => match super::mode_of(&str_of("name")) {
                Some(m) => set_mode(s, m),
                None => Ok(json!({})),
            },
            "sandbox" => Ok(super::sandbox::make(s)),
            "holiday" => Ok(super::people::holidays::answer(s, id, true, true)),
            _ => Ok(json!({})),
        },
        // the clock's questions
        "ask_spare" => {
            finish(s, Some(Choice::Spare { vehicle: id }), true);
            Ok(json!({}))
        }
        "ask_rental" => {
            finish(s, Some(Choice::Rental), true);
            Ok(json!({}))
        }
        "ask_drop" => {
            finish(s, Some(Choice::Drop), true);
            Ok(json!({}))
        }
        "ask_on" => {
            finish(s, None, args.get("on").and_then(Value::as_bool).unwrap_or(true));
            Ok(json!({}))
        }
        "ask_dispatcher" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
            if let Some(c) = s.company.as_mut() {
                c.clock.dispatcher = on;
            }
            s.generation += 1;
            Ok(json!({}))
        }
        "morning_fill" => {
            let key = str_of("key");
            let fill: Option<Fill> = args.get("fill").and_then(|v| serde_json::from_value(v.clone()).ok());
            done(apply(s, |c| {
                pl::set_fill(c, &key, fill);
                Ok(())
            }))
        }
        _ => {
            let _ = c;
            return None;
        }
    })
}
