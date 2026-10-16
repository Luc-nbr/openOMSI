//! The country a company plays in (Beta 62, Luc: "at the start users can choose the country they
//! play in, and then it has local names"): chosen in the founding wizard beside the company's
//! name - a searchable list with the flags, the names in the page's language, the map's country
//! proposed - and changed in the company's settings. A change names the people hired from then
//! on; those the company has keep their names. The names are `omsi_launcher_lib::company::names`'.

use super::words::{tr, trf};
use super::{act as apply, Session};
use omsi_launcher_lib::company::names;
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};

/// What the founding and the settings say of the country.
pub const CAPS: &str = "Country";
pub const SEARCH: &str = "Search a country";
pub const NONE_FOUND: &str = "No country of that name.";
pub const WIZARD_NOTE: &str = "Your staff, applicants and passengers get names of this country. The map's country is proposed; you can change it later in the company's settings.";
pub const SETTINGS_NOTE: &str = "The names of the people you hire from now on. The people you have keep their names.";
pub const CHANGED: &str = "Your new people come from %{country} now. The people you have keep their names.";

/// Every text of the country's choice, the countries' names among them (for the translations'
/// check).
pub fn texts() -> Vec<String> {
    let mut v: Vec<String> = [CAPS, SEARCH, NONE_FOUND, WIZARD_NOTE, SETTINGS_NOTE, CHANGED].iter().map(|t| t.to_string()).collect();
    v.extend(names::pools().iter().map(|p| p.name.clone()));
    v
}

/// The countries as the list offers them: International first, the others by their name in the
/// page's language; each with its flag and the words it is found by (its name in English too).
fn options() -> Vec<Value> {
    let mut all: Vec<(String, Value)> = names::pools()
        .iter()
        .map(|p| {
            let label = tr(&p.name);
            let words = names::fold(&format!("{label} {} {}", p.name, p.key));
            let order = if p.key == names::INTERNATIONAL { String::new() } else { names::fold(&label) };
            (order, json!({ "key": p.key, "flag": p.flag, "label": label, "words": words }))
        })
        .collect();
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all.into_iter().map(|x| x.1).collect()
}

fn label_of(key: &str) -> String {
    let p = names::pool(key);
    format!("{} {}", p.flag, tr(&p.name))
}

/// The founding wizard's choice (`chooser::wizard`): the countries, the one chosen (`country`
/// of `args`), else the map's.
pub fn wizard_part(args: &Value, map: &str, map_name: &str) -> Value {
    let proposed = names::guess(map, map_name);
    let chosen = args.get("country").and_then(Value::as_str).filter(|k| names::known(k)).unwrap_or(proposed);
    json!({
        "caps": tr(CAPS), "options": options(), "key": chosen, "proposed": proposed, "label": label_of(chosen),
        "search": tr(SEARCH), "none": tr(NONE_FOUND), "note": tr(WIZARD_NOTE),
    })
}

/// The company's settings' choice (`dialogs::settings`).
pub fn settings_part(c: &Company) -> Value {
    let key = names::country_of(c);
    json!({
        "caps": tr(CAPS).to_uppercase(), "options": options(), "key": key, "label": label_of(key),
        "search": tr(SEARCH), "none": tr(NONE_FOUND), "note": tr(SETTINGS_NOTE),
    })
}

/// The country chosen for the founding (`chooser::found`): the wizard's, else none (the map's).
pub fn founding(args: &Value) -> String {
    args.get("country").and_then(Value::as_str).filter(|k| names::known(k)).map(|k| names::pool(k).key.clone()).unwrap_or_default()
}

/// What the settings' choice does: "country", the boss's own (never from afar, as the logo).
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    (what == "country").then(|| set(s, args))
}

/// The settings' "country": the open company plays in `key` from now on.
pub fn set(s: &mut Session, args: &Value) -> Result<Value, String> {
    let Some(key) = args.get("key").and_then(Value::as_str).filter(|k| names::known(k)).map(|k| names::pool(k).key.clone()) else { return Ok(json!({})) };
    if let Err(p) = apply(s, |c| {
        c.country = key.clone();
        Ok(())
    }) {
        return Ok(json!({ "popup": p }));
    }
    s.say(trf(CHANGED, &[("country", &tr(&names::pool(&key).name))]), false);
    Ok(json!({ "country": key }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Founding, GameMode};

    #[test]
    fn the_country_is_proposed_chosen_and_changed() {
        super::super::words::set_language("en");
        let w = wizard_part(&json!({}), "maps/Grundorf/global.cfg", "Grundorf");
        assert_eq!((w["key"].as_str(), w["proposed"].as_str()), (Some("de"), Some("de")));
        let list = w["options"].as_array().unwrap();
        assert_eq!(list.len(), names::pools().len());
        assert_eq!(list[0]["key"], names::INTERNATIONAL);
        assert!(list.iter().any(|o| o["key"] == "nl" && o["label"] == "Netherlands" && o["flag"] == "🇳🇱"));
        assert_eq!(wizard_part(&json!({ "country": "pl" }), "maps/Grundorf/global.cfg", "")["key"], "pl");
        assert_eq!(founding(&json!({ "country": "PL" })), "pl");
        assert_eq!(founding(&json!({ "country": "atlantis" })), "");
        // (in the page's language, sorted by it)
        super::super::words::set_language("de");
        let o = options();
        let de = o.iter().position(|x| x["key"] == "de").unwrap();
        assert_eq!(o[de]["label"], "Deutschland");
        assert!(o[de]["words"].as_str().unwrap().contains("germany"));
        super::super::words::set_language("en");
        // changed in the settings: the people keep their names
        let mut c = found(&Founding { name: "Land".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        let a = omsi_launcher_lib::company::staff::market(&c)[0].clone();
        omsi_launcher_lib::company::staff::hire(&mut c, &a).unwrap();
        let mut s = Session { company: Some(c), ..Default::default() };
        assert_eq!(settings_part(s.company.as_ref().unwrap())["key"], "de");
        act(&mut s, "country", &json!({ "key": "se" })).unwrap().unwrap();
        assert!(act(&mut s, "something_else", &json!({})).is_none());
        let c = s.company.as_ref().unwrap();
        assert_eq!((c.country.as_str(), c.staff[0].name.as_str()), ("se", a.name.as_str()));
    }

    #[test]
    fn every_country_text_is_translated() {
        let texts = texts();
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsihub-locales/locales/company-names.yml):\n{}", missing.len(), missing.join("\n"));
    }
}
