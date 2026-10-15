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
            let cards: Vec<Value> = rs::TOPICS.iter().filter(|t| t.section == *sec).map(|t| card(&c, t)).collect();
            json!({ "title": tr(sec.label()), "colour": colour(*sec), "motif": motif(*sec), "cards": cards })
        })
        .collect();
    json!({ "strip": strip, "banner": banner(&c), "sections": sections, "info_tip": tr("What it does") })
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
    let Some(run) = c.research.running.clone() else {
        return json!({ "none": tr("No research is running: choose one below and start it."), "staff": staff, "staff_tone": staff_tone });
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
    json!({
        "what": what, "share": run.share(), "tone": if people == 0 { "warn" } else { "accent" },
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
