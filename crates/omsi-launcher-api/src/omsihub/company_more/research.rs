//! The company's research (`crates/omsi-launcher-ui/src/company/research.rs`), as City Bus
//! Manager's tree: the strip with the cash, the trophies and the rating; the research under way,
//! how far it is and who works on it; the sections with a card a topic - its picture on its
//! section's patterned colour, its stages, its price in money and trophies, and what it waits
//! for, is doing or has done. The rules are `company::research`'s.

use super::super::kit;
use super::super::words::{eur, grouped, num, tr, trf};
use super::super::{act as apply, Session};
use super::{popup_answer, str_arg};
use crate::Answer;
use omsi_launcher_lib::company::research::{self as rs, Need, Section, State, Topic};
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};

/// A section's colour: the ground of its cards' pictures (`research::colour`).
fn colour(s: Section) -> [u8; 3] {
    match s {
        Section::Buses => [36, 150, 140],
        Section::Upgrades => [70, 128, 210],
        Section::Passengers => [52, 98, 178],
        Section::Premises => [52, 150, 82],
        Section::Staff => [124, 84, 196],
        Section::Workshop => [180, 106, 48],
        Section::Area => [30, 134, 172],
        Section::Tools => [216, 170, 54],
    }
}

/// The small sign a section's pattern repeats.
fn motif(s: Section) -> &'static str {
    match s {
        Section::Buses => "directions_bus",
        Section::Upgrades => "star",
        Section::Passengers => "person",
        Section::Premises => "home",
        Section::Staff => "groups",
        Section::Workshop => "construction",
        Section::Area => "location_on",
        Section::Tools => "add",
    }
}

/// What a topic does, in a few words for its tile (the whole of it is its `info`).
fn effect(key: &str) -> &'static str {
    match key {
        "articulated" => "Articulated buses at the dealers",
        "double_deckers" => "Double-deckers at the dealers",
        "fuel" => "−2 % fuel a stage",
        "dirt" => "−10 % dirt a stage",
        "wear" => "−5 % wear a stage",
        "air_conditioning" | "passenger_info" | "park_and_ride" => "+2 % passengers",
        "wlan" => "+2 % passengers, internet contracts",
        "usb" => "+1.5 % passengers",
        "premium_seats" => "+3 % passengers",
        "stops" => "+1.5 % passengers a stage",
        "app" => "+1 % passengers a stage",
        "ticket_machines" => "+1 % passengers, −3 % late trips a stage",
        "priority" => "−10 % late trips a stage",
        "premises" => "+6 bus places a stage",
        "external_depots" => "+10 bus places, repairs out there",
        "auto_cleaning" => "The fleet stays at least 55 % clean",
        "auto_washing" => "+50 % buses washed a night",
        "auto_refuelling" => "+50 % buses refuelled a night",
        "auto_repair" => "Broken buses back a day sooner",
        "ticket_inspectors" => "Ticket inspectors can be hired",
        "inspection" => "+15 % passengers checked a stage",
        "food" => "Happier, calmer staff",
        "parts" => "−10 % spare part prices a stage",
        "repairs" => "−10 % repair time a stage",
        "area" => "+1 concession a stage",
        "shift_schedule" => "Core hours for the staff",
        _ => "",
    }
}

/// How long a stage's work takes, in words: days at today's pace, else researcher-days.
fn work_text(c: &Company, work: f64) -> String {
    let pace = rs::daily_work(c, &c.date);
    if pace > 0.0 {
        let days = (work / pace).ceil().max(1.0);
        if days == 1.0 {
            tr("1 day")
        } else {
            trf("%{n} days", &[("n", &num(days, 0))])
        }
    } else {
        trf("%{n} researcher-days", &[("n", &num(work, 0))])
    }
}

/// What a need asks, in words.
fn need_text(n: Need) -> String {
    match n {
        Need::Done(key) => {
            let Some(t) = rs::topic(key) else { return key.to_string() };
            let name = tr(t.title);
            if t.stages > 1 {
                trf("%{name}, stage %{n}", &[("name", &name), ("n", &rs::stage_of(key).to_string())])
            } else {
                name
            }
        }
        Need::Level(lv) => trf("Company level %{n}", &[("n", &lv.to_string())]),
    }
}

pub fn view(s: &mut Session, _args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let strip = json!([
        { "icon": "payments", "value": super::super::sandbox::cash(&c), "tone": if super::super::sandbox::cash_tone(&c) == "danger" { "danger" } else { "ok" }, "tip": tr("Cash") },
        { "icon": "emoji_events", "value": grouped(c.trophies as f64), "tone": "warn", "tip": tr("Trophies: earned with goals and contracts, spent on research") },
        { "icon": "star", "value": format!("{} %", num(c.reputation, 1)), "tone": "accent2", "tip": tr("Rating: how the authority and the passengers see the company") },
    ]);
    let sections: Vec<Value> = Section::ALL
        .iter()
        .map(|sec| {
            let topics: Vec<&Topic> = rs::TOPICS.iter().filter(|t| t.section == *sec).collect();
            let cards: Vec<Value> = topics.iter().map(|t| card(&c, t)).collect();
            let done = topics.iter().filter(|t| rs::state(&c, t) == State::Done).count();
            json!({ "title": tr(sec.label()), "colour": colour(*sec), "motif": motif(*sec), "cards": cards,
                "count": trf("%{done} of %{n} researched", &[("done", &done.to_string()), ("n", &topics.len().to_string())]) })
        })
        .collect();
    json!({
        "strip": strip, "banner": banner(&c), "sections": sections, "info_tip": tr("What it does"),
        "title": tr("Research"), "detail_caps": tr("Details").to_uppercase(), "pick": tr("Choose a research to see what it does."),
        "cost_caps": tr("Next stage").to_uppercase(), "needs_caps": tr("Needs first").to_uppercase(),
        "words": { "money": tr("Money"), "trophies": tr("Trophies"), "work": tr("Work"), "done": tr("Researched"), "locked": tr("Locked") },
    })
}

/// The research under way, or none, and who works on it (`research::banner`).
fn banner(c: &Company) -> Value {
    let people = rs::researchers(c, &c.date).count();
    let pace = rs::daily_work(c, &c.date);
    let staff = if people == 0 {
        tr("No researchers at work: a research stands still until you hire researchers on the Staff page.")
    } else {
        trf("Researchers at work: %{n}, %{work} researcher-days of work a day.", &[("n", &people.to_string()), ("work", &num(pace, 1))])
    };
    let staff_tone = if people == 0 { "warn" } else { "soft" };
    let crew = match people {
        0 => tr("No researchers"),
        1 => tr("1 researcher"),
        n => trf("%{n} researchers", &[("n", &n.to_string())]),
    };
    let Some(run) = c.research.running.clone() else {
        return json!({ "none": tr("No research is running: choose one below and start it."), "staff": staff, "staff_tone": staff_tone, "crew": crew });
    };
    let Some(t) = rs::topic(&run.topic) else { return Value::Null };
    let what = if t.stages > 1 {
        trf("Researching: %{name}, stage %{stage} of %{stages}", &[("name", &tr(t.title)), ("stage", &run.stage.to_string()), ("stages", &t.stages.to_string())])
    } else {
        trf("Researching: %{name}", &[("name", &tr(t.title))])
    };
    let left = match rs::days_left(c) {
        Some(d) => trf("%{share} % done, about %{n} days to go at today's pace", &[("share", &num(run.share() * 100.0, 0)), ("n", &num(d, 0))]),
        None => trf("%{share} % done: nobody works on it", &[("share", &num(run.share() * 100.0, 0))]),
    };
    let days = if run.paused {
        tr("Paused")
    } else {
        match rs::days_left(c) {
            Some(d) if d <= 1.0 => tr("1 day left"),
            Some(d) => trf("%{n} days left", &[("n", &num(d, 0))]),
            None => tr("Stands still"),
        }
    };
    json!({
        "name": tr(t.title), "key": t.key,
        "stage": (t.stages > 1).then(|| trf("Stage %{stage} of %{stages}", &[("stage", &run.stage.to_string()), ("stages", &t.stages.to_string())])),
        "pct": format!("{} %", num(run.share() * 100.0, 0)), "days": days, "crew": crew, "effect": tr(effect(t.key)),
        "paused": run.paused, "pause": tr("Pause"), "resume": tr("Resume"),
        "pause_tip": tr("Pausing keeps the work done: nobody works on it until you resume it."),
        "what": what, "share": run.share(), "tone": if run.paused || people == 0 { "warn" } else { "accent" },
        "line": format!("{left}  ·  {staff}"), "staff_tone": staff_tone,
        "icon": t.icon, "colour": colour(t.section), "motif": motif(t.section),
        "stop": tr("Stop the research"), "again": tr("Click again to stop"),
        "stop_tip": tr("Stopping gives back the trophies and half the money; the work done is lost."),
    })
}

/// A topic's card (`research::card`).
fn card(c: &Company, t: &Topic) -> Value {
    let state = rs::state(c, t);
    let done = rs::stages_of(c, t);
    let running = c.research.running.as_ref().filter(|x| x.topic == t.key);
    let part = running.map(|x| x.share()).unwrap_or(0.0);
    let mut out = json!({
        "key": t.key, "title": tr(t.title), "icon": t.icon,
        "state": match state { State::Done => "done", State::Running => "running", State::Locked => "locked", State::Open => "open" },
        "done": done, "stages": t.stages,
        "frac": ((done as f64 + part) / t.stages as f64).clamp(0.0, 1.0),
        "effect": tr(effect(t.key)), "info": tr(t.info), "section": tr(t.section.label()), "colour": colour(t.section),
        "stage": (t.stages > 1 && done < t.stages).then(|| trf("Stage %{stage} of %{stages}", &[("stage", &(done + 1).to_string()), ("stages", &t.stages.to_string())])),
    });
    match state {
        State::Done => out["said"] = json!(tr("Researched")),
        State::Running => {
            let share = running.map(|r| r.share()).unwrap_or(0.0);
            out["said"] = json!(trf("Researching: %{share} %", &[("share", &num(share * 100.0, 0))]));
            out["left"] = json!(match rs::days_left(c) {
                Some(d) => trf("about %{n} days to go", &[("n", &num(d, 0))]),
                None => tr("nobody works on it"),
            });
        }
        State::Locked | State::Open => {
            let k = rs::cost(c, t, done + 1);
            let short = k.trophies - c.trophies;
            out["work"] = json!(work_text(c, (k.work - part * k.work).max(0.0)));
            out["work_all"] = json!(trf("%{n} researcher-days of work", &[("n", &num(k.work, 0))]));
            out["money"] = json!(eur(k.money));
            out["money_tone"] = json!(if !c.can_pay(k.money) { "danger" } else { "text" });
            out["trophies"] = json!(if short > 0 { trf("%{n} (%{short} missing)", &[("n", &grouped(k.trophies as f64)), ("short", &grouped(short as f64))]) } else { grouped(k.trophies as f64) });
            out["trophies_tone"] = json!(if short > 0 { "danger" } else { "text" });
            if state == State::Open {
                out["start"] = json!(tr("Start research"));
                out["primary"] = json!(c.research.running.is_none());
            } else {
                let needs: Vec<String> = rs::unmet(c, t).into_iter().map(need_text).collect();
                out["needs"] = json!(trf("Needs: %{what}", &[("what", &needs.join(", "))]));
                out["needs_list"] = json!(needs);
            }
        }
    }
    out
}

/// What a topic does, its stages, its price and what it needs, in a popup (`research::info`).
fn info(c: &Company, key: &str) -> Option<Value> {
    let t = rs::topic(key)?;
    let done = rs::stages_of(c, t);
    let mut text = tr(t.info);
    text.push_str("\n\n");
    if done >= t.stages {
        text.push_str(&tr("Researched: it works for the company."));
    } else {
        let k = rs::cost(c, t, done + 1);
        text.push_str(&trf(
            "Stage %{stage} of %{stages} costs %{money} and %{trophies} trophies, and %{work} researcher-days of work.",
            &[("stage", &(done + 1).to_string()), ("stages", &t.stages.to_string()), ("money", &eur(k.money)), ("trophies", &grouped(k.trophies as f64)), ("work", &num(k.work, 0))],
        ));
    }
    let needs: Vec<String> = rs::unmet(c, t).into_iter().map(need_text).collect();
    let unlock = if needs.is_empty() { String::new() } else { trf("It needs first: %{what}.", &[("what", &needs.join(", "))]) };
    Some(kit::Popup::new(t.icon, t.title, text, unlock, None).json(c.game_mode.casual()))
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    let c = s.company.clone()?;
    let casual = c.game_mode.casual();
    Some(match what {
        "research_info" => Ok(info(&c, &str_arg(args, "key")).map(|p| json!({ "popup": p })).unwrap_or(json!({}))),
        // a topic's next stage: what is in the way said first, else started (`research::start`)
        "research_start" => {
            let key = str_arg(args, "key");
            let Some(t) = rs::topic(&key) else { return Some(Ok(json!({}))) };
            let k = rs::cost(&c, t, rs::stages_of(&c, t) + 1);
            if c.research.running.is_some() {
                let p = kit::Popup::new("search", "A research is running", tr("Your researchers work on one research at a time: wait until it is done, or stop it at the top of the page."), "", None);
                return Some(popup_answer(p.json(casual)));
            }
            if k.trophies > c.trophies {
                let text = trf("The company has %{have} trophies; this research needs %{need}: %{short} more.", &[("have", &grouped(c.trophies as f64)), ("need", &grouped(k.trophies as f64)), ("short", &grouped((k.trophies - c.trophies) as f64))]);
                let p = kit::Popup::new("emoji_events", "Not enough trophies", text, tr("Trophies are earned with goals and contracts: see the Contracts page and the inbox."), None);
                return Some(popup_answer(p.json(casual)));
            }
            if !c.can_pay(k.money) {
                return Some(popup_answer(kit::no_cash(&c, k.money).json(casual)));
            }
            match apply(s, |c| rs::start(c, t.key)) {
                Ok(stage) => {
                    let text = if t.stages > 1 {
                        trf("The research has begun: %{name}, stage %{stage}.", &[("name", &tr(t.title)), ("stage", &stage.to_string())])
                    } else {
                        trf("The research has begun: %{name}.", &[("name", &tr(t.title))])
                    };
                    s.say(text, false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        "research_pause" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
            match apply(s, |c| rs::pause(c, on)) {
                Ok(()) => {
                    s.say(tr(if on { "The research is paused: the work done is kept." } else { "The research goes on." }), false);
                    Ok(json!({}))
                }
                Err(p) => popup_answer(p),
            }
        }
        "research_stop" => match apply(s, rs::stop) {
            Ok(_) => {
                s.say(tr("The research was stopped: the trophies and half the money came back."), false);
                Ok(json!({}))
            }
            Err(p) => popup_answer(p),
        },
        _ => return None,
    })
}
