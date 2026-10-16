//! The Sandbox page (Beta 61): a sandbox company's knobs in their groups - money, unlocks, the
//! economy, the operations and the staff - each a switch or a factor with what it does and
//! whether it is as a Career company has it; the balance and the reputation set by hand; and
//! every knob back to the Career's. Only a sandbox company has it (`company::pages_for`); the
//! rules are `omsi_launcher_lib::company::sandbox`'s, read where each value is used.

use super::words::{eur, money_cents, num, tr, trf, units};
use super::{act as apply, Session};
use crate::Answer;
use omsi_launcher_lib::company::sandbox::{self as sb, Group, Kind, KNOBS};
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};

/// The Sandbox page's tab (`company::TABS`).
pub const SANDBOX_TAB: usize = 17;

/// The balance as the pages show it: ∞ with unlimited money.
pub fn cash(c: &Company) -> String {
    if sb::unlimited(c) {
        "∞".to_string()
    } else {
        eur(c.cash)
    }
}

/// The balance's tone: never in the red with unlimited money.
pub fn cash_tone(c: &Company) -> &'static str {
    if sb::unlimited(c) || c.cash >= 0 {
        "text"
    } else {
        "danger"
    }
}

/// The badge by the company's name (`company::shell`): a sandbox says it is one.
pub fn badge(c: &Company) -> Value {
    if !sb::is_sandbox(c) {
        return Value::Null;
    }
    json!({ "label": tr("Sandbox"), "icon": "sports_esports", "tip": tr("A sandbox company: you set its rules on the Sandbox page."), "tab": SANDBOX_TAB })
}

/// How many knobs differ from the Career's, as the page and the start screen's tile say it.
pub fn changed_text(c: &Company) -> String {
    match KNOBS.iter().filter(|k| !sb::of(c).is_career(k.key)).count() {
        0 => tr("Everything as in a career"),
        1 => tr("1 setting changed"),
        n => trf("%{n} settings changed", &[("n", &n.to_string())]),
    }
}

/// Casual's pages with a sandbox's own (`company::shell`: its crumbs and its name).
pub fn casual_pages(c: &Company, mut pages: Value) -> Value {
    if sb::is_sandbox(c) {
        if let Some(m) = pages.get_mut("pages").and_then(Value::as_object_mut) {
            m.insert(SANDBOX_TAB.to_string(), json!({ "name": tr("Sandbox"), "icon": "sports_esports", "parent": super::HOME_TAB }));
        }
    }
    pages
}

/// The company's mode in its settings (`dialogs::settings`): a career, with the way to turn it
/// into a sandbox; or a sandbox, with the way to its page.
pub fn settings_part(c: &Company) -> Value {
    let mode = c.company_mode;
    json!({
        "caps": tr("Company mode").to_uppercase(), "key": mode.key(), "label": tr(mode.label()), "info": tr(mode.info()),
        "icon": if sb::is_sandbox(c) { "sports_esports" } else { "military_tech" },
        "switch": (!sb::is_sandbox(c)).then(|| tr("Turn into a sandbox")),
        "note": (!sb::is_sandbox(c)).then(|| tr("A sandbox cannot be turned back into a career.")),
        "open": sb::is_sandbox(c).then(|| tr("Open the Sandbox page")), "tab": SANDBOX_TAB,
    })
}

/// The confirmation before a career is turned into a sandbox: its title, its words, its button.
pub fn confirm_text(c: &Company) -> (String, String, String) {
    (
        trf("Turn %{name} into a sandbox?", &[("name", &c.name)]),
        tr("The company becomes a sandbox for good: you set its rules yourself on the Sandbox page - money, unlocks, prices, wages, disruptions and staff. Its buses, people, lines, money and history stay as they are, and every rule starts as it is now. A sandbox cannot be turned back into a career."),
        tr("Turn into a sandbox"),
    )
}

/// Turn the open company into a sandbox (the confirmation answered): to its Sandbox page.
pub fn make(s: &mut Session) -> Value {
    match apply(s, sb::make_sandbox) {
        Ok(()) => {
            s.say(tr("The company is a sandbox now: set its rules on the Sandbox page."), false);
            json!({ "tab": SANDBOX_TAB })
        }
        Err(p) => json!({ "popup": p }),
    }
}

/// The founding wizard's choice of how the company is played (`chooser::wizard`): its two cards,
/// the one chosen.
pub fn wizard_part(args: &Value) -> Value {
    let chosen = args.get("company_mode").and_then(Value::as_str).and_then(sb::CompanyMode::of_key).unwrap_or_default();
    let cards: Vec<Value> = sb::CompanyMode::ALL
        .iter()
        .map(|m| json!({ "key": m.key(), "label": tr(m.label()), "info": tr(m.info()), "icon": if *m == sb::CompanyMode::Sandbox { "sports_esports" } else { "military_tech" } }))
        .collect();
    json!({
        "caps": tr("Company mode").to_uppercase(), "cards": cards, "key": chosen.key(),
        "note": tr(if chosen == sb::CompanyMode::Sandbox { "Every rule starts as a career company has it: set them on the Sandbox page once the company is founded." } else { "A career can be turned into a sandbox later in the company's settings, but not back." }),
    })
}

/// A factor as the page says it ("×1.5", "×0").
fn times(x: f64) -> String {
    let places = if (x * 10.0).fract().abs() > 1e-9 { 2 } else if x.fract().abs() > 1e-9 { 1 } else { 0 };
    format!("×{}", num(x, places))
}

fn knob_json(c: &Company, k: &sb::Knob) -> Value {
    let s = sb::of(c);
    let value = s.get(k.key).unwrap_or(1.0);
    let career = sb::CAREER.get(k.key).unwrap_or(1.0);
    let changed = !s.is_career(k.key);
    let mut v = json!({
        "key": k.key, "label": tr(k.label), "info": tr(k.info), "changed": changed,
        "career": trf("Career: %{value}", &[("value", &match k.kind {
            Kind::Switch => tr(if career != 0.0 { "On" } else { "Off" }),
            Kind::Factor { .. } => times(career),
        })]),
    });
    match k.kind {
        Kind::Switch => {
            v["switch"] = json!(true);
            v["on"] = json!(value != 0.0);
        }
        Kind::Factor { least, most } => {
            v["switch"] = json!(false);
            v["value"] = json!(value);
            v["least"] = json!(least);
            v["most"] = json!(most);
            v["text"] = json!(times(value));
            v["reset"] = json!(changed.then(|| tr("Reset")));
        }
    }
    v
}

/// The page: the knobs by group, the balance and the reputation.
pub fn view(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    if !sb::is_sandbox(&c) {
        return json!({ "career": tr("This company is a career: turn it into a sandbox in the company's settings to set its rules."), "settings": tr("The company's settings") });
    }
    let groups: Vec<Value> = Group::ALL
        .iter()
        .map(|g| {
            let knobs: Vec<Value> = KNOBS.iter().filter(|k| k.group == *g).map(|k| knob_json(&c, k)).collect();
            json!({ "label": tr(g.label()), "caps": tr(g.label()).to_uppercase(), "icon": g.icon(), "knobs": knobs })
        })
        .collect();
    let changed = KNOBS.iter().filter(|k| !sb::of(&c).is_career(k.key)).count();
    json!({
        "title": tr("Sandbox"),
        "intro": tr("Set the company's rules the way you like them. Every setting starts as a career company has it; change it here at any time."),
        "changed": changed_text(&c),
        "groups": groups,
        "set_caps": tr("Set by hand").to_uppercase(),
        "balance": {
            "label": tr("Balance"), "value": cash(&c), "tone": cash_tone(&c), "euros": units(c.cash).round(),
            "note": tr(if sb::unlimited(&c) { "With unlimited money the balance shows ∞; this is what the books count." } else { "The difference is booked as capital: it is not what running the lines earned." }),
            "booked": eur(c.cash), "set": tr("Set the balance"),
        },
        "reputation": { "label": tr("Reputation"), "value": format!("{} %", num(c.reputation.clamp(0.0, 100.0), 1)), "points": (c.reputation * 10.0).round() / 10.0, "set": tr("Set the reputation") },
        "reset_all": (changed > 0).then(|| tr("Every setting back to the career's")),
        "on": tr("On"), "off": tr("Off"),
    })
}

/// What a click on the page does.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    let done = |r: Result<(), Value>| Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})));
    let number = |k: &str| args.get(k).and_then(Value::as_f64);
    Some(match what {
        "sandbox_knob" => {
            let key = args.get("key").and_then(Value::as_str).unwrap_or("").to_string();
            let value = number("value").or_else(|| args.get("on").and_then(Value::as_bool).map(|b| if b { 1.0 } else { 0.0 })).unwrap_or(1.0);
            done(apply(s, |c| {
                sb::set_knob(c, &key, value)?;
                // (the plan of the day is the one the new rules make)
                Ok(())
            }))
            .inspect(|_| s.plan = None)
        }
        "sandbox_balance" => {
            // (typed in the company's currency: `euros` is of before the currencies)
            let cents = money_cents(number("euros").unwrap_or(0.0));
            match apply(s, |c| sb::set_balance(c, cents)) {
                Ok(_) => {
                    let t = trf("The balance is %{amount} now.", &[("amount", &eur(cents))]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "sandbox_reputation" => {
            let value = number("value").unwrap_or(50.0);
            done(apply(s, |c| sb::set_reputation(c, value)))
        }
        "sandbox_reset" => {
            let r = done(apply(s, sb::reset));
            s.say(tr("Every setting is back to the career's."), false);
            s.plan = None;
            r
        }
        _ => return None,
    })
}

/// Every text the sandbox shows: the core's knobs and modes, and this page's own.
pub fn texts() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in KNOBS {
        out.push(k.label.into());
        out.push(k.info.into());
    }
    for g in Group::ALL {
        out.push(g.label().into());
    }
    for m in sb::CompanyMode::ALL {
        out.push(m.label().into());
        out.push(m.info().into());
    }
    // (the start screen's tile, the founding's words, the page's place in the strip)
    for t in [
        "Set the company's rules",
        "%{name} is founded as a sandbox: set its rules on the Sandbox page.",
        // (the core's refusals, said in a popup)
        "Only a sandbox company has these settings.",
        "There is no such setting.",
        "Choose a number.",
        "The company is a sandbox already.",
    ] {
        out.push(t.into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Founding, GameMode};

    fn session(mode: sb::CompanyMode) -> Session {
        super::super::words::set_language("en");
        let c = found(&Founding { name: "Sand".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), company_mode: mode, ..Default::default() }, "Luc");
        Session { company: Some(c), ..Default::default() }
    }

    #[test]
    fn the_page_sets_the_knobs() {
        let mut s = session(sb::CompanyMode::Sandbox);
        let v = view(&mut s);
        assert_eq!(v["groups"].as_array().map(Vec::len), Some(5));
        assert!(v["reset_all"].is_null());
        act(&mut s, "sandbox_knob", &json!({ "key": "unlimited_money", "on": true })).unwrap().unwrap();
        act(&mut s, "sandbox_knob", &json!({ "key": "income", "value": 2.5 })).unwrap().unwrap();
        let c = s.company.as_ref().unwrap();
        assert!(c.sandbox.unlimited_money && c.sandbox.income == 2.5);
        assert_eq!(cash(c), "∞");
        assert!(badge(c)["label"].is_string());
        let v = view(&mut s);
        assert_eq!(v["balance"]["value"], "∞");
        assert_eq!(v["groups"][2]["knobs"][0]["text"], "×2.5");
        assert!(v["reset_all"].is_string());
        act(&mut s, "sandbox_balance", &json!({ "euros": 12345.0 })).unwrap().unwrap();
        assert_eq!(s.company.as_ref().unwrap().cash, 12345_00);
        act(&mut s, "sandbox_reset", &json!({})).unwrap().unwrap();
        assert_eq!(s.company.as_ref().unwrap().sandbox, sb::CAREER);
        // a career company has no knobs, nor a badge
        let mut k = session(sb::CompanyMode::Career);
        assert!(view(&mut k)["career"].is_string());
        assert!(badge(k.company.as_ref().unwrap()).is_null());
        let r = act(&mut k, "sandbox_knob", &json!({ "key": "income", "value": 3.0 })).unwrap().unwrap();
        assert!(r["popup"].is_object());
        assert_eq!(k.company.as_ref().unwrap().sandbox, sb::CAREER);
    }

    #[test]
    fn every_sandbox_text_is_translated() {
        let mut texts = texts();
        for src in [include_str!("sandbox.rs")] {
            for lead in ["tr(\"", "trf(\""] {
                let mut rest = src;
                while let Some(k) = rest.find(lead) {
                    let after = &rest[k + lead.len()..];
                    rest = after;
                    let t: String = after.chars().take_while(|c| *c != '"').collect();
                    if t.chars().any(char::is_alphabetic) {
                        texts.push(t);
                    }
                }
            }
        }
        assert!(texts.len() > 60, "only {} texts found", texts.len());
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsihub-locales/locales/sandbox.yml):\n{}", missing.len(), missing.join("\n"));
    }
}
