//! The bus company's start and its founding (`crates/omsi-launcher-ui/src/company/chooser.rs`,
//! `wizard.rs`): how to play (alone; with others comes later), the driver's companies as tiles
//! and a tile to found a new one; the founding with its name, colours, mark, home, game mode
//! (three cards), first day and buying.

use super::words::{day_label, eur, short_map, today, tr, trf};
use super::{data, mark_json, mode_json, switch_company, tile_line, Session, HOME_TAB};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::{self as co, GameMode};
use serde_json::{json, Value};

/// The colours a company can wear (`wizard::PALETTE`).
pub const PALETTE: [&str; 12] = ["#f28c28", "#e03c31", "#c2185b", "#7b1fa2", "#283593", "#1e88e5", "#00897b", "#43a047", "#fdd835", "#6d4c41", "#455a64", "#f5f5f5"];

/// The start: its two play modes and the driver's companies as tiles.
pub fn view(s: &mut Session) -> Value {
    let list = s.companies.clone().unwrap_or_default();
    let n = list.len();
    let tiles: Vec<Value> = list
        .iter()
        .map(|c| {
            let map = if c.map_name.is_empty() { short_map(&c.map) } else { c.map_name.clone() };
            let home = if c.depot.is_empty() { map } else { format!("{map}  ·  {}", c.depot) };
            json!({
                "id": c.id, "name": c.name, "mark": mark_json(c), "mode": mode_json(c.game_mode),
                "clock": tile_line(c).map(|(t, tone)| json!([t, tone])), "home": home,
                "day": format!("{}  ·  {}", day_label(&c.date), super::sandbox::cash(c)), "negative": super::sandbox::cash_tone(c) == "danger", "sandbox": super::sandbox::badge(c),
            })
        })
        .collect();
    json!({
        "modes": {
            "title": tr("How do you want to play?"), "text": tr("Your own company in a time of its own, or one run together with others."),
            "single": { "title": tr("Singleplayer"), "text": tr("Found and run your own companies, in a time of their own that runs while you play."), "foot": if n == 1 { tr("1 company on this computer") } else { trf("%{n} companies on this computer", &[("n", &n.to_string())]) } },
            // (Beta 58: a friend's company joined with its session code - "Play together")
            "multi": { "title": tr("Join a company"), "text": tr("A friend runs a bus company: join it with his session code and drive duties of its day for it, in his world."), "foot": tr("With a session code"), "tip": tr("The boss starts \"Play together\" in his company and sends you the code."), "off": (!super::super::together::enabled()).then(|| tr(super::super::together::BLOCKED)) },
        },
        "companies": {
            "back": tr("Play mode"),
            "title": tr(if list.is_empty() { "Found your first company" } else { "Your companies" }),
            "text": tr(if list.is_empty() { "You have no company yet: give it a name, colours, a map and a depot." } else { "Choose the company to open, or found a new one." }),
            "tiles": tiles, "new": tr("New company"), "new_sub": tr("Its name, colours, map and depot"), "delete_tip": tr("Delete this company"),
            "ask_title": tr("Delete %{name}?"), "ask_text": tr("The company is deleted for good: its buses, staff, lines, money and history. Your service record as a driver stays. This cannot be undone."),
            "ask_yes": tr("Delete for good"), "ask_no": tr("Cancel"),
        },
    })
}

/// The date a map is set in (`wizard::map_date`).
fn map_date(map: &core::MapInfo, chosen_map: &str, chosen_date: &str) -> String {
    if map.file == chosen_map && co::dates::parse(chosen_date).is_some() {
        return chosen_date.to_string();
    }
    let text = format!("{} {} {}", map.file, map.name, map.friendly);
    let year = text.split(|c: char| !c.is_ascii_digit()).filter(|w| w.len() == 4).filter_map(|w| w.parse::<i32>().ok()).find(|y| (1950..=2099).contains(y));
    match year {
        Some(y) => format!("{y}{}", &core::DEFAULT_DATE[4..]),
        None => core::DEFAULT_DATE.to_string(),
    }
}

/// The first company day the wizard's choice comes to (`wizard::first_day`).
fn first_day(when: usize, map: Option<&core::MapInfo>, chosen_map: &str, chosen_date: &str, own: &str) -> String {
    match when {
        1 => map.map(|m| map_date(m, chosen_map, chosen_date)).unwrap_or_else(today),
        2 if co::dates::parse(own).is_some() => own.to_string(),
        _ => today(),
    }
}

fn map_label(m: &core::MapInfo) -> String {
    if m.friendly.trim().is_empty() {
        m.name.clone()
    } else {
        m.friendly.clone()
    }
}

/// The launcher's remembered duty (`launcher-duty.json`): its map and date.
fn chosen() -> (String, String) {
    let v: Value = std::fs::read_to_string(data().join("launcher-duty.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    (v.get("map").and_then(Value::as_str).unwrap_or("").to_string(), v.get("date").and_then(Value::as_str).unwrap_or("").to_string())
}

/// The founding's choices made (`map` its index, `when` 0 today / 1 the map's / 2 own `date`):
/// the maps and their depots, the game mode cards, the first day, the buying.
pub fn wizard(s: &mut Session, args: &Value) -> Result<Value, String> {
    let maps = s.maps();
    let vehicles = s.vehicles();
    let (chosen_map, chosen_date) = chosen();
    let k = args.get("map").and_then(Value::as_u64).map(|x| x as usize).unwrap_or_else(|| maps.iter().position(|m| m.file == chosen_map).unwrap_or(0)).min(maps.len().saturating_sub(1));
    let depots = maps.get(k).map(|m| super::lines::depots_for(m, &vehicles)).unwrap_or_default();
    let when = args.get("when").and_then(Value::as_u64).unwrap_or(0) as usize;
    let own = args.get("date").and_then(Value::as_str).map(str::to_string).unwrap_or_else(today);
    let first = first_day(when, maps.get(k), &chosen_map, &chosen_date, &own);
    let name = args.get("name").and_then(Value::as_str).unwrap_or("");
    let suggested = co::short_of(name);
    let mode = args.get("mode").and_then(Value::as_str).and_then(super::mode_of).unwrap_or_else(|| s.company.as_ref().map_or(GameMode::Casual, |c| c.game_mode));
    let cards: Vec<Value> = GameMode::ALL
        .iter()
        .map(|m| json!({ "mode": mode_json(*m), "pitch": tr(m.pitch()), "capital": trf("Starting capital %{amount}", &[("amount", &eur(co::economy::rules(*m).start_capital))]), "bullets": m.bullets().iter().map(|b| tr(b)).collect::<Vec<_>>() }))
        .collect();
    let has = s.companies.as_ref().is_some_and(|c| !c.is_empty());
    let mut v = json!({
        "title": tr("Found your bus company"),
        "intro": tr("Run your own transport company on a map: buy, lease or rent buses, hire drivers and run the map's lines or your own. The company has its own clock: simulate its time by the hour or by days, and what happens is told as it comes; what you drive yourself counts as it was driven."),
        "company_caps": tr("The company").to_uppercase(), "name_label": tr("Name"), "name_hint": "Stadtbus Grundorf",
        "short_label": tr("Short name (plates and logo)"), "short_hint": if suggested.is_empty() { "SG".to_string() } else { suggested },
        "colours": [tr("Main colour"), tr("Second colour")], "palette": PALETTE,
        "no_logo": tr("No logo picture: the short name is the mark."), "choose_logo": tr("Choose a logo picture"), "remove_logo": tr("Remove the picture"),
        "map_label": tr("Home map"), "depot_label": tr("Depot (the buses' depot file)"),
    });
    v["maps"] = json!(maps.iter().map(map_label).collect::<Vec<_>>());
    v["map"] = json!(k);
    v["depots"] = json!(depots);
    v["no_maps"] = json!(tr("No maps found: the company needs a map to be at home on."));
    v["no_depot"] = json!(tr("The map has no depot file."));
    v["mode_caps"] = json!(tr("Game mode").to_uppercase());
    v["cards"] = json!(cards);
    v["mode"] = json!(super::mode_key(mode));
    v["mode_note"] = json!(tr(if mode.rules().rosterflow { "Switch the game mode at any time in the company's settings. RosterFlow, the professional scheduling studio, can be switched on there as an add-on." } else { "Switch the game mode at any time in the company's settings: nothing is lost." }));
    // (Beta 61: a career, or a sandbox whose rules the player sets)
    v["company_mode"] = super::sandbox::wizard_part(args);
    v["first_label"] = json!(tr("First company day"));
    v["whens"] = json!([tr("Today"), tr("The map's date"), tr("A date of my own")]);
    v["when"] = json!(when);
    v["date"] = json!(own);
    v["begins"] = json!(trf("The company begins on %{date}.", &[("date", &day_label(&first))]));
    // (Omsi-Hub 6/6: the logo picture chosen, as the mark will show it - the preview's own)
    v["logo"] = json!(args.get("logo").and_then(Value::as_str).filter(|l| !l.is_empty()).and_then(super::logo_picture));
    v["buying_label"] = json!(tr("Buying buses"));
    v["casual_buying"] = json!(tr("Casual buys at the list price, in one click: the buses are yours at once."));
    v["buyings"] = json!(co::dealer::BuyingMode::ALL.iter().map(|m| tr(m.label())).collect::<Vec<_>>());
    v["buying_says"] = json!([tr("A model, a number, the list price: the buses are yours at once."), tr("Haggle with the dealer, agree on extras and sign a contract; new buses are delivered.")]);
    // (the buses the company works with: the map's region proposed - Beta 54)
    v["vehicles"] = super::vehicles::wizard_part(s, &maps.get(k).map(|m| m.file.clone()).unwrap_or_default());
    v["map_file"] = json!(maps.get(k).map(|m| m.file.clone()).unwrap_or_default());
    v["cancel"] = json!(has.then(|| tr("Cancel")));
    v["found"] = json!(tr("Found the company"));
    v["need_name"] = json!(tr("Give the company a name."));
    v["popup_name"] = json!({ "icon": "badge", "title": tr("A name first"), "text": tr("The company needs a name: it is on its buses, its papers and its contracts."), "unlock": tr("Type one in the field \"Name\"; the short name is made of it."), "go": null });
    v["popup_map"] = json!({ "icon": "map", "title": tr("No map"), "text": tr("The company needs a map to be at home on: none was found in the OMSI folder."), "unlock": tr("Install a map, or choose the OMSI folder under Setup."), "go": null });
    Ok(v)
}

/// Found the company the wizard says (`wizard::draw`'s "Found the company").
pub fn found(s: &mut Session, args: &Value) -> Result<Value, String> {
    let maps = s.maps();
    let vehicles = s.vehicles();
    let name = args.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if name.is_empty() || maps.is_empty() {
        return Ok(json!({ "refused": if maps.is_empty() { "map" } else { "name" } }));
    }
    let (chosen_map, chosen_date) = chosen();
    let k = args.get("map").and_then(Value::as_u64).unwrap_or(0) as usize;
    let m = &maps[k.min(maps.len() - 1)];
    let depots = super::lines::depots_for(m, &vehicles);
    let depot = args.get("depot").and_then(Value::as_u64).unwrap_or(0) as usize;
    let colours: Vec<usize> = args.get("colours").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_else(|| vec![0, 4]);
    let when = args.get("when").and_then(Value::as_u64).unwrap_or(0) as usize;
    let own = args.get("date").and_then(Value::as_str).unwrap_or("").to_string();
    let mode = args.get("mode").and_then(Value::as_str).and_then(super::mode_of).unwrap_or(GameMode::Casual);
    let buying = args.get("buying").and_then(Value::as_u64).unwrap_or(1).min(1) as usize;
    let f = co::Founding {
        name: name.clone(),
        short: args.get("short").and_then(Value::as_str).unwrap_or("").chars().take(4).collect(),
        colours: [PALETTE[colours.first().copied().unwrap_or(0).min(11)].to_string(), PALETTE[colours.get(1).copied().unwrap_or(4).min(11)].to_string()],
        logo: args.get("logo").and_then(Value::as_str).filter(|l| !l.is_empty()).map(str::to_string),
        map: m.file.clone(),
        map_name: map_label(m),
        depot: depots.get(depot).cloned().unwrap_or_default(),
        date: first_day(when, Some(m), &chosen_map, &chosen_date, &own),
        game_mode: mode,
        buying: co::dealer::BuyingMode::ALL[buying],
        // (the wizard's choice, else the map's region)
        vehicles: args.get("vehicles").filter(|v| !v.is_null()).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_else(|| co::vehicles::VehicleChoice::for_region(super::vehicles::region_of_map(s, &m.file))),
        company_mode: args.get("company_mode").and_then(Value::as_str).and_then(co::sandbox::CompanyMode::of_key).unwrap_or_default(),
    };
    let mut c = co::found(&f, &core::load_config().profile);
    c.id = co::store::unused_id(&data(), &c.name);
    let id = c.id.clone();
    switch_company(s, Some(c));
    super::changed(s);
    let t = if f.company_mode == co::sandbox::CompanyMode::Sandbox {
        trf("%{name} is founded as a sandbox: set its rules on the Sandbox page.", &[("name", &f.name)])
    } else {
        trf("%{name} is founded. Add a line, buy a bus and hire drivers.", &[("name", &f.name)])
    };
    s.say(t, false);
    Ok(json!({ "open": id, "tab": HOME_TAB }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_day_is_today_or_the_maps() {
        let t = today();
        assert_eq!(first_day(0, None, "", "", ""), t);
        let bh = core::MapInfo { name: "Bad_Huegelsdorf_2020".into(), friendly: String::new(), file: "maps/Bad_Huegelsdorf_2020/global.cfg".into(), description: String::new(), entry_points: Vec::new(), hof: String::new(), installed: false, left_hand: false };
        assert_eq!(map_date(&bh, "maps/Bad_Huegelsdorf_2020/global.cfg", "2005-10-12"), "2005-10-12");
        assert_eq!(map_date(&bh, "maps/Other/global.cfg", "2005-10-12"), "2020-05-30");
        assert_eq!(first_day(2, None, "", "", "1999-01-02"), "1999-01-02");
        assert_eq!(first_day(2, None, "", "", "nonsense"), t);
    }
}
