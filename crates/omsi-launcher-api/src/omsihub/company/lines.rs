//! The company's lines (`crates/omsi-launcher-ui/src/company/lines.rs`): those it runs, with
//! today's tours and how they are covered, a line chosen in public (its title, its buses' depot
//! file, its colour) above its tours, and the lines it can add - the map's and the player's own -
//! with what one needs against what the company has (`add_view`). The line editor and the
//! RosterFlow Timeplan are the game-drawn launcher's until their phase: the page hands over.

use super::words::{day_label, eur, eur_cents, grouped, share_tone, tr, trf};
use super::{act as apply, Session};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_launcher_lib::lines::{own_line_of, OwnLine};
use omsi_launcher_lib::service::ServiceKind;
use serde_json::{json, Value};
use std::collections::HashSet;

fn hhmm(minutes: i32) -> String {
    co::clock::hhmm(minutes as i64)
}

/// A line's plate: its number and colour (the yellow of the timetable's lines when it has none).
pub fn plate(number: &str, colour: &str) -> Value {
    json!({ "number": number, "colour": colour.trim() })
}

/// Letters folded as the bus step compares them (`buspick::fold`).
fn fold(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'ß' => out.push_str("ss"),
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => out.push('a'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' => out.push('i'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
            c if c.is_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

/// The termini a depot file can show (None: not found beside any bus).
fn hof_termini(hof: &str, map: &str) -> Option<HashSet<String>> {
    if hof.trim().is_empty() {
        return None;
    }
    let (bases, _) = core::depot_roots();
    core::linehof::first_depot(hof.trim(), &bases, &core::lines::map_folder(map)).map(|d| {
        let mut set = HashSet::new();
        for t in &d.hof.termini {
            set.insert(fold(&t.texture_id));
            set.insert(fold(&t.strings.join(" ")));
            set.extend(t.strings.iter().map(|s| fold(s)));
        }
        set.remove("");
        set
    })
}

/// A player's line's words with its kind of service after them (`ownlines::with_kind`).
fn with_kind(o: &OwnLine, text: String) -> String {
    if o.service == ServiceKind::Regular {
        return text;
    }
    let kind = tr(o.service.label());
    if text.is_empty() {
        kind
    } else {
        format!("{text} · {kind}")
    }
}

/// The depot files a company on `map` can give its buses (`wizard::depots_for`).
pub fn depots_for(map: &core::MapInfo, vehicles: &[core::VehicleInfo]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    fn push(s: &str, out: &mut Vec<String>) {
        let s = s.trim();
        if !s.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(s)) {
            out.push(s.to_string());
        }
    }
    push(&map.hof, &mut out);
    let folder = map.file.split(['/', '\\']).rev().nth(1).unwrap_or("").to_lowercase();
    let words: Vec<String> = format!("{} {} {}", folder, map.name, map.friendly).to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 4).map(str::to_string).collect();
    let mut all: Vec<&String> = vehicles.iter().flat_map(|v| v.hofs.iter()).collect();
    all.sort_by_key(|h| h.to_lowercase());
    for h in &all {
        let lh = h.to_lowercase();
        if words.iter().any(|w| lh.contains(w.as_str())) {
            push(h, &mut out);
        }
    }
    if out.is_empty() {
        for h in all {
            push(h, &mut out);
        }
    }
    out
}

fn ours(s: &mut Session, c: &Company, selected: Option<&str>) -> Value {
    if c.lines.is_empty() {
        return json!({ "title": tr("The company's lines"), "empty": tr("The company runs no line yet. Add one of the map's lines, or one of your own from the line editor: every tour it has on a day is then the company's to plan and run.") });
    }
    let plan = s.plan.clone();
    let now = co::clock::now(c);
    let easy = co::autopilot::on(c);
    let rf = co::autopilot::rosterflow(c);
    let rows: Vec<Value> = c
        .lines
        .iter()
        .map(|line| {
            let caption = if !line.title.trim().is_empty() { line.title.trim().to_string() } else if line.caption.is_empty() { line.name.clone() } else { line.caption.clone() };
            let pending = line.pending.as_ref().map(|p| trf("New timetable from %{date}", &[("date", &day_label(&p.from))]));
            let own = line.own.then(|| {
                let kind = co::ownline::kind_of(line);
                let t = if kind == ServiceKind::Regular { tr("Own line") } else { tr(kind.label()) };
                let mut tip = tr(if kind == ServiceKind::Regular { "Your own line, made in the line editor: no concession, a licence a month" } else { kind.note() });
                if let Some(v) = line.plan.as_ref().map(|p| &p.vehicles).filter(|v| !v.open()) {
                    tip.push_str(&format!("\n{}: {}", tr("Buses on this line"), v.summary(&|s| tr(s))));
                }
                json!([t, tip])
            });
            let in_service = co::network::in_service(line, now);
            let (covered, all) = plan.as_ref().map(|p| p.planned_of(&line.name)).unwrap_or((0, line.tours as usize));
            let frac = if all > 0 { covered as f64 / all as f64 } else { 0.0 };
            let none_yet = co::ownline::no_timetable(line);
            let (label, right, tip, click) = if none_yet {
                (tr("No timetable yet"), tr("Make it"), tr(if rf { "The line runs no tour until it has a timetable. Click to make it in RosterFlow's Timeplan." } else { "The line runs no tour until it has a timetable. Click to make it in the line editor." }), if rf { "timetable" } else { "edit" })
            } else if !in_service {
                let waits = line.service_from.is_some_and(|s| s > now);
                (
                    if waits {
                        trf("In service from %{when}", &[("when", &line.service_from.map(|s| format!("{} {}", day_label(&co::clock::date_of(s)), co::clock::hhmm(s))).unwrap_or_default())])
                    } else if easy {
                        tr("Not in service yet")
                    } else {
                        trf("Not planned yet: %{c} of %{t} tours planned", &[("c", &covered.to_string()), ("t", &all.to_string())])
                    },
                    if easy { String::new() } else { tr("Plan it") },
                    tr(if easy { "The company puts the line into service by itself, at the latest the next morning." } else { "The line runs only once it is planned and its service is started. Click for the planning." }),
                    "planning",
                )
            } else {
                let open = all - covered;
                (
                    trf("Covered today: %{c} of %{t} tours", &[("c", &covered.to_string()), ("t", &all.to_string())]),
                    if all > 0 { format!("{:.0} %", frac * 100.0) } else { String::new() },
                    if easy {
                        if open == 0 { tr("Every tour in service has a bus and its drivers.") } else { trf("%{n} tours of today lack a bus or a driver: the company hires drivers by itself, buses you get at the dealer.", &[("n", &open.to_string())]) }
                    } else if open == 0 {
                        tr("Every tour of today has its bus and drivers. Click for the planning.")
                    } else {
                        trf("%{n} tours of today lack a bus or a driver: they are dropped, with the contract's penalty. Click for the planning.", &[("n", &open.to_string())])
                    },
                    "planning",
                )
            };
            json!({
                "name": line.name, "plate": plate(&line.number, &line.colour), "caption": caption, "on": selected == Some(line.name.as_str()),
                "fare": format!("{} {}", tr("Fare"), eur_cents(co::fares::fare_of(c, line) as f64)), "fare_tip": tr("The single ticket this line asks: change it, and see what it does to passengers and fares"),
                "pending": pending, "pending_tip": tr("Saved in the line editor: today's tours run as they are; from that day the line runs its new timetable and its changed tours are planned anew."),
                "own": own,
                "timetable": (rf && (line.plan.is_some() || !line.own)).then(|| tr("Timetable: the line's running times and departures in RosterFlow's Timeplan")),
                "edit": if line.plan.is_some() { Some(tr("Edit route: change the line in the line editor")) } else if !line.own { Some(tr(if rf { "Edit the line in the line editor: its route and displays" } else { "Edit the line in the line editor: its route, timetable and displays" })) } else { None },
                "remove_tip": tr("Stop running this line"),
                // (the line editor, handed over: an own line by its id, a line of the map by its name)
                "edit_to": edit_to(line),
                "bar": { "frac": frac, "tone": share_tone(frac, !in_service), "label": label, "right": right, "tip": tip, "click": if easy && click == "planning" { "select" } else { click } },
                "km": trf("%{km} km a day", &[("km", &grouped(line.km))]),
            })
        })
        .collect();
    json!({ "title": tr("The company's lines"), "rows": rows })
}

fn line_card(s: &mut Session, c: &Company, line: &co::CompanyLine) -> Value {
    let maps = s.maps();
    let vehicles = s.vehicles();
    let depots: Vec<String> = maps.iter().find(|m| m.file.eq_ignore_ascii_case(&c.map)).map(|m| depots_for(m, &vehicles)).unwrap_or_default();
    let mut options = vec![json!(["", trf("The company's: %{hof}", &[("hof", &c.depot)])])];
    options.extend(depots.iter().filter(|d| !d.eq_ignore_ascii_case(&c.depot)).map(|d| json!([d, d])));
    let hof = line.hof_or(&c.depot).to_string();
    let termini: Vec<String> = s.map_lines().and_then(|ls| ls.iter().find(|x| x.name.eq_ignore_ascii_case(&line.name))).map(|x| x.termini.clone()).unwrap_or_default();
    let (words, tone) = match hof_termini(&hof, &c.map) {
        None => (trf("The depot file %{hof} was not found beside any bus.", &[("hof", &hof)]), "warn"),
        Some(set) => {
            let missing: Vec<&String> = termini.iter().filter(|t| !set.contains(&fold(t))).collect();
            if termini.is_empty() {
                (String::new(), "dim")
            } else if missing.is_empty() {
                (tr("It knows every destination of the line."), "ok")
            } else {
                let list = missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                (trf("Not in it: %{list}. Own lines bring theirs when saved in the line editor.", &[("list", &list)]), "warn")
            }
        }
    };
    let mut under: Vec<String> = Vec::new();
    if !line.title.trim().is_empty() && !line.caption.trim().is_empty() {
        under.push(line.caption.clone());
    }
    let kind = co::ownline::kind_of(line);
    if kind != ServiceKind::Regular {
        under.push(tr(kind.label()));
    }
    under.push(c.name.clone());
    let now = co::rosterflow::line_colour(c, &line.name);
    let hex = |x: [u8; 3]| format!("#{:02x}{:02x}{:02x}", x[0], x[1], x[2]);
    json!({
        "title": tr("The line in public").to_uppercase(),
        "public_caps": tr("Public title").to_uppercase(), "public": line.title, "hint": if line.caption.trim().is_empty() { "Shuttleverkehr Altenfeld - Wurzbach".to_string() } else { line.caption.trim().to_string() },
        "save": tr("Save"),
        "hof_caps": tr("Depot file of its buses").to_uppercase(), "hofs": options, "hof": line.hof.trim(), "hof_words": words, "hof_tone": tone,
        "advert_caps": tr("As advertised").to_uppercase(), "advert_colour": if line.colour.trim().is_empty() { c.colours[0].clone() } else { line.colour.clone() },
        "public_name": line.public_name(), "under": under.join("  ·  "), "plate": plate(&line.number, &line.colour),
        "colour_caps": tr("Colour").to_uppercase(), "colour": hex(now), "palette": co::rosterflow::PALETTE.iter().map(|x| hex(*x)).collect::<Vec<_>>(),
        "more_tip": tr("Any colour: the picker"), "automatic": tr("Automatic"), "done": tr("Done"),
    })
}

fn tours_of(s: &mut Session, c: &Company, name: &str) -> Value {
    let Some(line) = c.lines.iter().find(|x| x.name == name).cloned() else { return Value::Null };
    let card = line_card(s, c, &line);
    let easy = co::autopilot::on(c);
    let mut out = json!({ "card": card, "title": trf("Line %{n} today", &[("n", &line.number)]).to_uppercase(), "planning": (!easy).then(|| tr("Open the planning")), "add": tr("Add lines") });
    let Some(plan) = s.plan.clone() else {
        out["reading"] = json!(tr("Reading the timetable…"));
        return out;
    };
    let tours: Vec<co::day::TourPlan> = plan.tours.into_iter().filter(|t| t.tour.line.eq_ignore_ascii_case(name)).collect();
    if tours.is_empty() {
        out["none"] = json!(tr("The line has no tour on this day (its timetable runs on other days)."));
        return out;
    }
    let rows: Vec<Value> = tours
        .iter()
        .map(|t| {
            let mut when = format!("{} – {}  ·  {:.0} km", hhmm(t.tour.from()), hhmm(t.tour.to()), t.tour.km());
            if let Some(size) = co::ownline::wanted(c, &t.tour) {
                when.push_str("  ·  ");
                when.push_str(&tr(co::BusKind { size, drive: co::Drive::Diesel }.label()));
            }
            let bus = t.bus.and_then(|b| c.fleet.iter().find(|f| f.id == b)).map(|f| trf("Bus %{n}", &[("n", &f.number)]));
            let drivers: Vec<String> = t.duties.iter().filter_map(|d| d.driver.and_then(|id| c.employee(id)).map(|e| e.name.clone())).collect();
            let (what, tone) = if t.by_player {
                (tr("Driven by you"), "accent2")
            } else if t.live {
                (tr("Reported by the game"), "accent2")
            } else if t.tour.unplanned && t.covered() {
                (tr("Planned - the line is not in service yet"), "soft")
            } else if t.tour.unplanned {
                (tr("Not planned - the line is not in service yet"), "dim")
            } else if t.bus.is_none() {
                (tr("No bus planned: dropped"), "danger")
            } else if drivers.len() < t.duties.len() {
                (trf("%{n} duties without a driver", &[("n", &(t.duties.len() - drivers.len()).to_string())]), "warn")
            } else {
                (tr("Covered"), "ok")
            };
            let mut who = bus.unwrap_or_default();
            if !drivers.is_empty() {
                if !who.is_empty() {
                    who.push_str("  ·  ");
                }
                who.push_str(&drivers.join(", "));
            }
            json!({ "head": format!("{} {}", tr("Tour"), t.tour.tour), "when": when, "who": who, "what": what, "tone": tone })
        })
        .collect();
    out["rows"] = json!(rows);
    out
}

fn to_add(s: &mut Session, c: &Company, mine_chosen: bool) -> Value {
    let rf = co::autopilot::rosterflow(c);
    let mut out = json!({
        "title": tr("Add a line"), "make": tr("Make a new line"),
        "make_tip": tr(if rf { "Draw a line of the company's own in the line editor: you confirm and pay for its route there, then make its timetable in RosterFlow's Timeplan" } else { "Draw a line of the company's own in the line editor: it shows what the line costs and brings, and you confirm and pay for it there" }),
    });
    let Some(today) = s.today.as_ref() else {
        out["reading"] = json!(tr("Reading the timetable…"));
        return out;
    };
    if let Some(e) = &today.error {
        out["error"] = json!(e);
        return out;
    }
    let all: Vec<core::LineInfo> = today.lines.clone();
    let own = s.own.clone();
    let (maps, mine): (Vec<&core::LineInfo>, Vec<&core::LineInfo>) = all.iter().partition(|l| own_line_of(&l.name, &own).is_none());
    let showing_mine = mine_chosen && !mine.is_empty();
    out["switch"] = json!({ "labels": [trf("Map lines (%{n})", &[("n", &maps.len().to_string())]), trf("My lines (%{n})", &[("n", &mine.len().to_string())])], "mine": showing_mine, "none": mine.is_empty(), "none_tip": tr("Make one in Editor → Line editor"), "mine_tip": tr("The lines you made in the line editor") });
    let list: Vec<&core::LineInfo> = if showing_mine { mine } else { maps };
    let list: Vec<&core::LineInfo> = list.into_iter().filter(|x| !c.lines.iter().any(|y| y.name.eq_ignore_ascii_case(&x.name))).collect();
    let n = list.len();
    let list: Vec<&core::LineInfo> = list.into_iter().filter(|x| showing_mine || co::concessions::is_line(c, x)).collect();
    let hidden = n - list.len();
    if hidden > 0 {
        out["hidden"] = json!(trf("%{n} timetables of the map are not the company's to run: depot and empty runs (they go with the tours that need them), specials and other operators' traffic.", &[("n", &hidden.to_string())]));
    }
    if list.is_empty() {
        out["all_run"] = json!(tr("The company runs all of them already."));
        return out;
    }
    let direct = co::concessions::may_add_directly(c);
    let depot = c.depot.clone();
    let bids: Vec<(String, Option<i64>)> = c.concessions.tenders.iter().filter(|t| t.open()).map(|t| (t.line.to_lowercase(), t.offers.last().map(|o| o.1))).collect();
    let rows: Vec<Value> = list
        .iter()
        .map(|line| {
            let o = own_line_of(&line.name, &own);
            let (p, caption) = match &o {
                Some(o) => {
                    let cap = o.caption();
                    (plate(&o.number, &o.colour), with_kind(o, if cap.is_empty() { line.termini.join(" · ") } else { cap }))
                }
                None => (plate(&co::specials::number_of(line, &[&depot]), ""), format!("{}  ·  {}", line.name, co::specials::caption_of(line, &[&depot]))),
            };
            let (label, icon) = if direct || o.is_some() {
                ("Add…", "add")
            } else if bids.iter().find(|b| b.0 == line.name.to_lowercase()).is_some_and(|t| t.1.is_some()) {
                ("Bid made", "receipt_long")
            } else {
                ("Apply…", "receipt_long")
            };
            let runs = line.tours.iter().filter(|t| t.runs).count();
            let km: f64 = line.tours.iter().filter(|t| t.runs).flat_map(|t| t.trips.iter()).map(|t| t.km).sum();
            json!({
                "name": line.name, "plate": p, "caption": caption, "label": tr(label), "icon": icon,
                "sub": trf("%{n} tours today  ·  %{km} km", &[("n", &runs.to_string()), ("km", &format!("{km:.0}"))]),
                "tip": tr(if !direct && o.is_none() { "A line of the map is run under a concession: bid in its tender, and the line is the company's if the bid wins" } else { "See what the line needs against what the company has, then add it" }),
            })
        })
        .collect();
    out["rows"] = json!(rows);
    out
}

/// The Lines page: the company's lines on the left; on the right the line chosen, or the lines
/// to add; what a line earns under both.
pub fn view(s: &mut Session, ui: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let r = co::economy::rules_for(&c);
    let pay = co::economy::compensation_per_km(&r, c.reputation, c.contract_index);
    let foot = trf("A single ticket of the fare association costs %{single} (a passenger pays %{fare} on average, with day and season tickets); the authority pays %{km} for every kilometre run, more the better your reputation.", &[("single", &eur_cents(co::fares::association_fare(&c) as f64)), ("fare", &eur_cents(r.fare as f64)), ("km", &eur_cents(pay))]);
    let selected = ui.get("selected").and_then(Value::as_str).filter(|n| c.lines.iter().any(|x| x.name == *n)).map(str::to_string);
    let mine = ui.get("mine").and_then(Value::as_bool).unwrap_or(false);
    let right = match &selected {
        Some(name) => json!({ "tours": tours_of(s, &c, name) }),
        None => json!({ "add": to_add(s, &c, mine) }),
    };
    json!({ "foot": foot, "ours": ours(s, &c, selected.as_deref()), "right": right, "selected": selected })
}

/// Taking a line on (`lines::add_dialog`): what it needs against what the company has.
pub fn add_view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let name = args.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    let Some(all) = s.today.as_ref().map(|t| t.lines.clone()) else { return json!({ "gone": true }) };
    let Some(line) = all.iter().find(|x| x.name == name).cloned() else { return json!({ "gone": true }) };
    let own = own_line_of(&line.name, &s.own);
    let direct = co::concessions::may_add_directly(&c) || own.is_some();
    let easy = co::autopilot::on(&c);
    let asking = (easy && !direct).then(|| co::concessions::asking_price_of(&c, &line));
    let number = own.as_ref().map(|o| o.number.clone()).unwrap_or_else(|| co::specials::number_of(&line, &[&c.depot]));
    let n = co::network::needs(&c, &line, &all, &c.date);
    let intro = match (easy, direct) {
        (true, true) => "What the line asks of the company on a day like today, against what it has. In the easy mode it runs at once: the company fills its roster every morning.",
        (true, false) => "What the line asks of the company on a day like today, against what it has. Its concession is taken at the asking price, without an auction: the line is the company's from tomorrow and runs by itself.",
        (false, true) => "What the line asks of the company on a day like today, against what it has. It comes in not planned: it runs once its tours are planned and its service is started on the Planning page.",
        (false, false) => "What the line asks of the company on a day like today, against what it has. Its concession is auctioned: if your bid wins, the line comes in not planned - it runs once its tours are planned and its service is started.",
    };
    let (short_b, short_d) = n.short();
    let runs = line.tours.iter().filter(|t| t.runs).count();
    let free_b = n.have_buses.saturating_sub(n.busy_buses);
    let free_d = n.have_drivers.saturating_sub(n.busy_drivers);
    let mut rows = vec![json!([tr("Tours today"), runs.to_string(), "", true]), json!([tr("Buses at its busiest"), n.buses.to_string(), trf("%{n} of %{all}", &[("n", &free_b.to_string()), ("all", &n.have_buses.to_string())]), short_b == 0])];
    for (size, k) in &n.sizes {
        let have = c.fleet.iter().filter(|v| v.held_on(&c.date) && v.kind.size == *size).count();
        rows.push(json!([format!("    {}", tr(co::BusKind { size: *size, drive: co::Drive::Diesel }.label())), k.to_string(), have.to_string(), have >= *k]));
    }
    rows.push(json!([trf("Drivers (%{n} duties, about one and a half a duty)", &[("n", &n.duties.to_string())]), n.drivers.to_string(), trf("%{n} of %{all}", &[("n", &free_d.to_string()), ("all", &n.have_drivers.to_string())]), short_d == 0]));
    let short = (short_b > 0 || short_d > 0).then(|| trf(if easy { "Short for this line: %{b} buses and %{d} drivers. Get the buses at the dealer; the company hires the drivers by itself, at night." } else { "Short for this line: %{b} buses and %{d} drivers. Buy or rent buses and hire drivers before you start its service." }, &[("b", &short_b.to_string()), ("d", &short_d.to_string())]));
    let fine = (short.is_none() && n.buses > 0).then(|| tr("The company has the buses and drivers for it."));
    json!({
        "name": name, "title": trf(if direct || easy { "Add line %{n}" } else { "Apply for line %{n}" }, &[("n", &number)]),
        "intro": tr(intro), "needs_caps": tr("The line needs").to_uppercase(), "need_caps": tr("Needs").to_uppercase(), "have_caps": tr("The company has free").to_uppercase(),
        "rows": rows, "short": short, "fine": fine,
        "asking": asking.map(|p| json!({ "text": trf("Its concession at the asking price: %{amount} (the company has %{cash}).", &[("amount", &eur(p)), ("cash", &super::sandbox::cash(&c))]), "ok": c.can_pay(p) })),
        "main": match asking { Some(p) => trf("Take it on for %{amount}", &[("amount", &eur(p))]), None if direct => tr("Add the line"), None => tr("Apply for its concession") },
        "main_icon": if direct { "add" } else { "receipt_long" }, "cancel": tr("Cancel"),
        "dealer": (short_b > 0).then(|| tr("To the dealer")), "hire": (short_d > 0).then(|| tr("Hire drivers")),
    })
}

/// What the Lines page and its dialog do.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
    let name = args.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    let done = |r: Result<(), Value>| Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})));
    Some(match what {
        "line_title" => {
            let text = args.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
            done(apply(s, |c| {
                if let Some(x) = c.lines.iter_mut().find(|x| x.name == name) {
                    x.title = text;
                }
                Ok(())
            }))
        }
        "line_hof" => {
            let h = args.get("hof").and_then(Value::as_str).unwrap_or("").to_string();
            done(apply(s, |c| {
                if let Some(x) = c.lines.iter_mut().find(|x| x.name == name) {
                    x.hof = h;
                }
                Ok(())
            }))
        }
        "line_colour" => {
            let colour = args.get("colour").and_then(Value::as_str).and_then(|h| {
                let h = h.trim().trim_start_matches('#');
                (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok()).flatten().map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
            });
            done(apply(s, |c| {
                co::rosterflow::set_line_colour(c, &name, colour);
                Ok(())
            }))
        }
        "add_line" => {
            let Some(all) = s.today.as_ref().map(|t| t.lines.clone()) else { return Some(Ok(json!({}))) };
            let Some(line) = all.iter().find(|x| x.name == name).cloned() else { return Some(Ok(json!({}))) };
            let own = own_line_of(&line.name, &s.own);
            let direct = co::concessions::may_add_directly(&c) || own.is_some();
            let easy = co::autopilot::on(&c);
            let number = own.as_ref().map(|o| o.number.clone()).unwrap_or_else(|| co::specials::number_of(&line, &[&c.depot]));
            if easy && !direct {
                return Some(match apply(s, |c| co::autopilot::take_line(c, &line)) {
                    Ok(_) => {
                        let t = trf("The concession for line %{n} is the company's: the line runs from tomorrow.", &[("n", &number)]);
                        s.say(t, false);
                        Ok(json!({ "done": true }))
                    }
                    Err(p) => Ok(json!({ "popup": p })),
                });
            }
            if easy {
                let date = c.date.clone();
                return Some(match apply(s, |c| {
                    co::network::add_line(c, &line, own.as_ref())?;
                    // (the player's own line: linked to its line of the line editor's at once)
                    super::link_own_lines(c, &all);
                    co::autopilot::start_lines(c, &date);
                    Ok(())
                }) {
                    Ok(()) => {
                        let t = trf("Line %{n} is the company's and runs from now on: its roster fills itself.", &[("n", &number)]);
                        s.say(t, false);
                        Ok(json!({ "done": true, "selected": line.name }))
                    }
                    Err(p) => Ok(json!({ "popup": p })),
                });
            }
            if !direct {
                // (its auction opens now: on the Concessions page, the game-drawn launcher's for now)
                return Some(match apply(s, |c| co::concessions::apply(c, &line)) {
                    Ok(_) => Ok(json!({ "done": true, "hand_over": super::CONCESSIONS_TAB.to_string() })),
                    Err(p) => Ok(json!({ "popup": p })),
                });
            }
            match apply(s, |c| {
                co::network::add_line(c, &line, own.as_ref())?;
                // (the player's own line: linked to its line of the line editor's at once, so
                // RosterFlow's Timeplan opens it right away)
                super::link_own_lines(c, &all);
                Ok(())
            }) {
                Ok(()) => {
                    let n = s.company.as_ref().and_then(|c| c.lines.last()).map(|x| x.number.clone()).unwrap_or_default();
                    if co::autopilot::rosterflow(&c) {
                        let t = trf("Line %{n} is the company's: here is its timetable. Plan its tours and start its service on the Roster tab.", &[("n", &n)]);
                        s.say(t, false);
                        return Some(Ok(json!({ "done": true, "selected": line.name, "hand_over": format!("timetable={}", line.name) })));
                    }
                    let t = trf("Line %{n} is the company's: plan its tours and start its service on the Planning page.", &[("n", &n)]);
                    s.say(t, false);
                    Ok(json!({ "done": true, "selected": line.name }))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "remove_line" => done(apply(s, |c| {
            co::network::remove_line(c, &name);
            Ok(())
        })),
        _ => return None,
    })
}

/// The line editor on a line of the company (handed over: an own line by its id, a line of the
/// map by its name), or None for an own line the line editor does not have.
pub(super) fn edit_to(line: &co::CompanyLine) -> Option<String> {
    match line.plan.as_ref() {
        Some(p) => Some(format!("line-editor={}", p.line_id)),
        None if !line.own => Some(format!("map-line={}", line.name)),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn letters_are_folded_as_the_bus_step_folds_them() {
        assert_eq!(super::fold("Straße Süd"), "strassesud");
        assert_eq!(super::fold("Grundorf, Kirche"), "grundorfkirche");
    }
}
