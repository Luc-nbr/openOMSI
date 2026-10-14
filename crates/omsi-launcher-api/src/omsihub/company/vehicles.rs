//! Which buses the company works with (`company::vehicles`, Luc for Beta 54): the list of the
//! installed bus models with their category and region - guessed, or as the player corrected
//! them for the whole launcher - and the company's choice: UK buses only, European buses only,
//! all, or its own selection. The same list serves the founding wizard (its choice kept in the
//! page until the company is founded), the company's settings and the launcher's Settings
//! (the corrections alone).

use super::words::{tr, trf};
use super::{act as apply, Session};
use omsi_launcher_lib::company::market::MarketBus;
use omsi_launcher_lib::company::vehicles::{self as vh, Category, Corrections, Region, VehicleChoice, VehicleMode};
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// The corrections as read, and the market corrected by them (for which market and which
/// revision of the corrections).
#[derive(Default)]
pub struct Classes {
    corrections: Option<Corrections>,
    rev: u64,
    fixed: Option<(usize, u64, Arc<Vec<MarketBus>>)>,
}

/// The player's corrections (read the first time).
pub fn corrections(s: &mut Session) -> &Corrections {
    s.classes.corrections.get_or_insert_with(Corrections::load)
}

/// The market's buses as the player has them (`Corrections::apply`), read when they are not yet.
pub fn market(s: &mut Session) -> Option<Arc<Vec<MarketBus>>> {
    s.ask_market();
    let raw = s.market.clone()?;
    let key = (Arc::as_ptr(&raw) as usize, s.classes.rev);
    if let Some((p, r, m)) = &s.classes.fixed {
        if (*p, *r) == key {
            return Some(m.clone());
        }
    }
    let co = corrections(s).clone();
    let fixed: Vec<MarketBus> = raw
        .iter()
        .map(|b| {
            let mut b = b.clone();
            co.apply(&mut b);
            b
        })
        .collect();
    let fixed = Arc::new(fixed);
    s.classes.fixed = Some((key.0, key.1, fixed.clone()));
    Some(fixed)
}

/// A bus of the market by its file, as the player has it.
pub fn market_bus(s: &mut Session, file: &str) -> Option<MarketBus> {
    let k = vh::key_of(file);
    market(s)?.iter().find(|b| vh::key_of(&b.file) == k).cloned()
}

/// A model's category or region corrected (`None`: back to the guess), saved at once.
fn correct(s: &mut Session, file: &str, category: Option<Option<Category>>, region: Option<Option<Region>>) -> Result<(), String> {
    corrections(s);
    let co = s.classes.corrections.as_mut().expect("read above");
    co.set(file, category, region);
    co.save().map_err(|e| format!("{e:#}"))?;
    s.classes.rev += 1;
    s.generation += 1;
    Ok(())
}

/// The modes as the page offers them.
fn modes() -> Vec<Value> {
    VehicleMode::ALL.iter().map(|m| json!([m.key(), tr(m.label())])).collect()
}

/// The region of a map, from what is known of it.
pub fn region_of_map(s: &mut Session, file: &str) -> Region {
    let maps = s.maps();
    match maps.iter().find(|m| m.file.eq_ignore_ascii_case(file)) {
        Some(m) => vh::map_region(m.left_hand, &[&m.name, &m.friendly, &m.file]),
        None => vh::map_region(false, &[file]),
    }
}

/// "38 of 182 models".
fn count_words(n: usize, of: usize) -> String {
    trf("%{n} of %{of} models", &[("n", &n.to_string()), ("of", &of.to_string())])
}

/// The choice of the page (the wizard's: its `choice`) or of the company.
fn choice_of(s: &Session, args: &Value) -> VehicleChoice {
    match args.get("choice") {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone()).unwrap_or_default(),
        _ => s.company.as_ref().map(|c| c.vehicles.clone()).unwrap_or_default(),
    }
}

/// Two or three letters of a name, for its tile.
fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let s: String = if words.len() >= 2 { words.iter().filter_map(|w| w.chars().next()).take(2).collect() } else { name.chars().filter(|c| c.is_alphanumeric()).take(2).collect() };
    s.to_uppercase()
}

/// The list (`scope`: "company", "wizard" - its `choice` in the page - or "launcher", the
/// corrections alone), searched (`search`) and filtered (`cat`, `region`: a key or empty).
pub fn view(s: &mut Session, args: &Value) -> Value {
    let scope = args.get("scope").and_then(Value::as_str).unwrap_or("company").to_string();
    let Some(fixed) = market(s) else {
        return json!({ "reading": tr("Reading the installed buses…") });
    };
    let raw = s.market.clone().unwrap_or_default();
    let vehicles = s.vehicles();
    let makers: HashMap<String, String> = vehicles.iter().map(|v| (vh::key_of(&v.file), v.manufacturer.clone())).collect();
    let guessed: HashMap<String, (Category, Region)> = raw.iter().map(|b| (vh::key_of(&b.file), (vh::category_of(b), b.region))).collect();
    let co = corrections(s).clone();
    let choice = choice_of(s, args);
    let owned: HashMap<String, usize> = s.company.as_ref().map(|c| c.fleet.iter().fold(HashMap::new(), |mut m, v| {
        *m.entry(vh::key_of(&v.bus)).or_insert(0) += 1;
        m
    })).unwrap_or_default();
    let q = args.get("search").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
    let cat = args.get("cat").and_then(Value::as_str).and_then(Category::of_key);
    let region = args.get("region").and_then(Value::as_str).and_then(Region::of_key);
    let mut rows: Vec<(Category, String, String, Value)> = Vec::new();
    for b in fixed.iter() {
        let k = vh::key_of(&b.file);
        let maker = makers.get(&k).cloned().unwrap_or_default();
        let c = vh::category_of(b);
        if cat.is_some_and(|x| x != c) || region.is_some_and(|x| x != b.region) {
            continue;
        }
        if !q.is_empty() && ![&b.name, &maker, &b.file].iter().any(|t| t.to_lowercase().contains(&q)) {
            continue;
        }
        let (gc, gr) = guessed.get(&k).copied().unwrap_or((c, b.region));
        let fix = co.of(&b.file);
        rows.push((
            c,
            maker.to_lowercase(),
            b.name.to_lowercase(),
            json!({
                "file": b.file, "name": b.name, "maker": maker, "initials": initials(&b.name),
                "cat": c.key(), "region": b.region.key(),
                "cat_fixed": fix.category.is_some(), "region_fixed": fix.region.is_some(),
                "guess_cat": gc.key(), "guess_region": gr.key(),
                "guess": trf("Detected: %{cat}, %{region}", &[("cat", &tr(gc.label())), ("region", &tr(gr.label()))]),
                "on": choice.allows(b),
                "owned": owned.get(&k).copied().unwrap_or(0),
            }),
        ));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.2.cmp(&b.2)));
    let on = choice.count(&fixed);
    let mut out = json!({
        "scope": scope,
        "cats": Category::ALL.iter().map(|c| json!([c.key(), tr(c.label())])).collect::<Vec<_>>(),
        "regions": Region::ALL.iter().map(|r| json!([r.key(), format!("{} ({})", tr(r.label()), tr(r.say()))])).collect::<Vec<_>>(),
        "all_cats": tr("Every category"), "all_regions": tr("Every region"),
        "search": tr("Search a bus"),
        "as_guessed": tr("as detected"),
        "rows": rows.into_iter().map(|r| r.3).collect::<Vec<_>>(),
        "shown_of": count_words(0, 0),
        "none": tr("No bus model fits the search."),
        "fixed_tip": tr("Corrected by you: for every company. Choose \"as detected\" to undo it."),
        "owned_tip": tr("The company has buses of this model: they stay, even when the model is switched off."),
    });
    out["shown_of"] = json!(trf("%{n} models shown", &[("n", &out["rows"].as_array().map_or(0, Vec::len).to_string())]));
    if scope == "launcher" {
        out["title"] = json!(tr("Bus categories and regions"));
        out["note"] = json!(tr("What kind of bus each installed model is, and where it is built for: detected from its files (the side its driver sits on), corrected here for every company - the dealer, the market, the shop and the bus company's lists use it."));
        return out;
    }
    let map_region = match (scope.as_str(), s.company.as_ref()) {
        ("company", Some(c)) => {
            let map = c.map.clone();
            region_of_map(s, &map)
        }
        _ => args.get("map").and_then(Value::as_str).map(|m| m.to_string()).map(|m| region_of_map(s, &m)).unwrap_or_default(),
    };
    out["title"] = json!(tr("Your vehicles"));
    out["note"] = json!(tr("The buses the company works with: the dealer, the used market, the day's offers, leasing and renting and the shop offer only these. Buses the company has already stay."));
    out["mode"] = json!(choice.mode.key());
    out["modes"] = json!(modes());
    out["choice"] = serde_json::to_value(&choice).unwrap_or(Value::Null);
    out["count"] = json!(count_words(on, fixed.len()));
    out["quick"] = json!([
        ["uk", tr("UK only")],
        ["europe", tr("Europe only")],
        ["all", tr("All")],
        ["map", trf("Only buses that fit my map (%{region})", &[("region", &tr(map_region.label()))])],
    ]);
    out["switch_on"] = json!(tr("Switch the shown on"));
    out["switch_off"] = json!(tr("Switch the shown off"));
    out
}

/// What the list's controls do (`what`: mode, model, models, quick - the company's or, for the
/// wizard, its `choice` handed back - and class, the launcher's corrections).
pub fn act(s: &mut Session, args: &Value) -> Result<Value, String> {
    let what = args.get("what").and_then(Value::as_str).unwrap_or("");
    let str_of = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if what == "class" {
        let file = str_of("file");
        // ("" back to the guess; absent: unchanged)
        let category = args.get("cat").and_then(Value::as_str).map(Category::of_key);
        let region = args.get("region").and_then(Value::as_str).map(Region::of_key);
        correct(s, &file, category, region)?;
        return Ok(json!({}));
    }
    let Some(fixed) = market(s) else { return Ok(json!({})) };
    let mut choice = choice_of(s, args);
    match what {
        "mode" => {
            let m = VehicleMode::of_key(&str_of("mode")).unwrap_or_default();
            choice.set_mode(m, &fixed);
        }
        "model" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
            choice.set_model(&str_of("file"), on, &fixed);
        }
        "models" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
            let files: Vec<String> = args.get("files").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
            let refs: Vec<&str> = files.iter().map(String::as_str).collect();
            choice.set_models(&refs, on, &fixed);
        }
        "quick" => {
            let m = match str_of("which").as_str() {
                "uk" => VehicleMode::Uk,
                "europe" => VehicleMode::Europe,
                "map" => {
                    let map = s.company.as_ref().map(|c| c.map.clone()).unwrap_or_else(|| str_of("map"));
                    VehicleMode::of_region(region_of_map(s, &map))
                }
                _ => VehicleMode::All,
            };
            choice = VehicleChoice { mode: m, off: Default::default() };
        }
        _ => return Ok(json!({})),
    }
    if args.get("choice").is_some_and(|v| !v.is_null()) || s.company.is_none() {
        return Ok(json!({ "choice": choice }));
    }
    let put = choice.clone();
    let _ = apply(s, |c: &mut Company| {
        c.vehicles = put;
        Ok(())
    });
    Ok(json!({ "choice": choice }))
}

/// The company's settings' part: the mode, how many models it sees.
pub fn settings_part(s: &mut Session) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let n = market(s).map(|m| (c.vehicles.count(&m), m.len()));
    json!({
        "caps": tr("Vehicles").to_uppercase(),
        "mode": c.vehicles.mode.key(), "modes": modes(),
        "count": n.map(|(a, b)| count_words(a, b)),
        "choose": tr("Choose the models…"),
        "say": tr("The dealer, the market and the shop offer only the buses the company works with."),
    })
}

/// The founding wizard's part: the region the map proposes (`map`: its file).
pub fn wizard_part(s: &mut Session, map: &str) -> Value {
    let r = region_of_map(s, map);
    json!({
        "caps": tr("Your vehicles").to_uppercase(),
        "proposed": VehicleMode::of_region(r).key(),
        "modes": modes(),
        "choose": tr("Choose the models…"),
        "say": trf("The map looks %{region}: the company proposes its buses. Change it at any time in the company's settings.", &[("region", &tr(match r { Region::Uk => "British", Region::Europe => "European" }))]),
    })
}

/// What kind of bus a market bus is, in words: a coach as one (`Category::Coach`), the others by
/// their kind ("Electric solo bus").
pub fn kind_label(b: &MarketBus) -> String {
    match vh::category_of(b) {
        Category::Coach => tr(Category::Coach.label()),
        _ => tr(b.kind.label()),
    }
}

/// The bus files of the company's own fleet whose model it does not work with any more.
pub fn left_out(s: &mut Session, c: &Company) -> std::collections::HashSet<String> {
    let Some(m) = market(s) else { return Default::default() };
    c.fleet.iter().filter(|v| vh::left_out(c, &v.bus, &m)).map(|v| vh::key_of(&v.bus)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_of_a_bus() {
        assert_eq!(initials("[MS] Masterbus"), "MM");
        assert_eq!(initials("Citaro"), "CI");
        assert_eq!(initials("MAN Lion's City"), "ML");
    }
}
