//! The duty card (`company::dutycard` of the launcher core): a duty of the planning printed as a
//! receipt printer prints it for the driver - the page's preview at the paper's width
//! (`launcher/ui/js/omsihub/company/dutycard.js`) prints it with the web view's print. The view
//! says everything on it in the page's language: the codes, the times, and every piece with its
//! trips, their IBIS codes and their stops (each marked a timing point or not: the page shows
//! all of them or the timing points alone), and the check code with its QR code.
//!
//! What a trip's stops and codes are is read as the game reads them: the stops and their times
//! from the map's timetable (`omsi_sim::timetable_run::TripTimes`: the trip's profile, the times
//! the map wrote and the rest shared out by the links' lengths), the codes from the depot file
//! the bus carries (`ibis_target`: what the game types into the IBIS itself). A player of "Play
//! together" is shown his own duties' cards (`relayed`), whatever his role.

use super::words::{day_label, tr, trf};
use super::{lines_of, plan_of, week_work, Session};
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::dutycard::{self as dc, Card, CardStop, Driver, Ibis, Item, Lookups};
use omsi_launcher_lib::company::network::PlannedTrip;
use omsi_launcher_lib::company::plan::{DayPlan, Who};
use omsi_launcher_lib::company::together::Perm;
use omsi_launcher_lib::company::{self as co, Company};
use omsi_launcher_lib::lines as reg;
use omsi_timetable::TimetableData;
use omsi_vehicle::Hof;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

fn hhmm(m: i32) -> String {
    co::clock::hhmm(m as i64)
}

/// Minutes as the card writes them: "7 h 05", "45 min".
fn span(m: i32) -> String {
    if m >= 60 {
        format!("{} h {:02}", m / 60, m % 60)
    } else {
        trf("%{n} min", &[("n", &m.max(0).to_string())])
    }
}

/// A file of the game's (a map, a bus) where it is installed: a content root's, else under the
/// game's folder.
fn installed(file: &str) -> PathBuf {
    let root = core::root_dir().unwrap_or_default();
    omsi_cfg::find_in_roots(file).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(&root, file))
}

/// The map's timetable on `date`, its chrono folders as the game takes them (kept a minute: a
/// card opened again, or another of the day).
fn timetable(map: &str, date: &str) -> Option<Arc<TimetableData>> {
    type Kept = Option<(String, String, Instant, Arc<TimetableData>)>;
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some((m, d, at, t)) = kept.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if m == map && d == date && at.elapsed().as_secs() < 60 {
            return Some(t.clone());
        }
    }
    let map_dir = installed(map).parent()?.to_path_buf();
    let code = omsi_map::date_code(date)?;
    let chrono = omsi_map::active_chrono_dirs(&map_dir, code);
    let off = omsi_map::chrono_deactivated_lines(&chrono);
    let t = Arc::new(TimetableData::load_with_chrono(&map_dir, &chrono, &off));
    *kept.lock().unwrap_or_else(|e| e.into_inner()) = Some((map.to_string(), date.to_string(), Instant::now(), t.clone()));
    Some(t)
}

/// The stops of trip `x` of line `line`, course `kurs`, as the game runs it: when the bus is at
/// each (minutes of the day), whether the map wrote that minute itself; the stations it passes
/// without stopping left out. None: the timetable has no such trip.
pub(crate) fn stops_of(tt: &TimetableData, line: &str, kurs: &str, x: &PlannedTrip) -> Option<Vec<CardStop>> {
    let l = tt.lines.iter().find(|l| l.name.trim().eq_ignore_ascii_case(line.trim()))?;
    let dep = x.whole_dep();
    let is = |e: &&omsi_timetable::TourTrip| e.trip.trim().eq_ignore_ascii_case(x.name.trim()) && e.departure.round() as i32 == dep;
    // (its course's entry first: another tour may run a trip of that name at that minute)
    let entry = l.tours.iter().filter(|t| t.number.trim() == kurs.trim()).flat_map(|t| &t.trips).find(is).or_else(|| l.tours.iter().flat_map(|t| &t.trips).find(is))?;
    let trip = tt.trips.iter().find(|t| t.name.trim().eq_ignore_ascii_case(entry.trip.trim()))?;
    let stations = omsi_sim::timetable_run::trip_stations(trip);
    let profile = trip.profiles.get(entry.profile.max(0) as usize).or(trip.profiles.first());
    let link = |a: i64, b: i64| tt.stn_links.iter().find(|k| k.from_id == a && k.to_id == b).map(|k| k.length);
    let times = omsi_sim::timetable_run::TripTimes::new(&stations, profile, &link);
    let legacy = |id: i64| trip.stations_legacy.iter().find(|r| r.first().and_then(|x| x.trim().parse::<i64>().ok()) == Some(id)).and_then(|r| r.get(2)).map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    let at = |secs: f64| (entry.departure as f64 + secs / 60.0).round() as i32;
    let n = stations.len();
    let out: Vec<CardStop> = stations
        .iter()
        .enumerate()
        .filter(|(i, _)| *i == 0 || *i + 1 == n || times.stops.get(*i).copied().unwrap_or(true))
        .map(|(i, &id)| {
            let (a, d) = times.stations.get(i).copied().unwrap_or_default();
            let name = tt.bus_stops.iter().find(|b| b.object_id == id).map(|b| b.name.trim().to_string()).filter(|n| !n.is_empty()).or_else(|| legacy(id)).unwrap_or_else(|| format!("stop {id}"));
            CardStop { id, name, arr: at(a), dep: at(d), hold: times.holds.get(i).copied().unwrap_or(false), timing: false }
        })
        .collect();
    (!out.is_empty()).then_some(out)
}

/// The IBIS codes of trip `x` (course `kurs`, its stops `stops`) from depot file `hof`, as the
/// game types them (`ibis_target`, as the companion's IBIS card says them: `companion::nav`).
pub(crate) fn ibis_of(hof: &Hof, x: &PlannedTrip, kurs: &str, stops: &[CardStop]) -> Option<Ibis> {
    let names: Vec<&str> = stops.iter().map(|s| s.name.as_str()).collect();
    let t = omsi_sim::timetable_run::ibis_target(hof, x.line.trim(), x.to.trim(), &names, None)?;
    let dest = hof.termini.get(t.terminus_index.max(0) as usize);
    let code = dest.map(|d| d.code).filter(|c| (0..1000).contains(c));
    Some(Ibis {
        line: format!("{:03}", t.line),
        suffix: (t.suffix > 0).then(|| format!("{:02}", t.suffix)),
        route: t.route.map(|r| format!("{}{r:02}", t.line)),
        dest: code.map(|c| format!("{c:03}")),
        dest_text: dest.and_then(|d| d.strings.iter().map(|s| s.trim()).find(|s| !s.is_empty())).unwrap_or(x.to.trim()).to_string(),
        kurs: kurs.trim().to_string(),
    })
}

/// The depot file `name` (with or without `.hof`) as the bus `bus` finds it: its own folder's,
/// else wherever another bus brings it (the game borrows it so).
fn hof_for(bus: &str, name: &str) -> Option<Hof> {
    let name = name.trim();
    let name = name.strip_suffix(".hof").or_else(|| name.strip_suffix(".HOF")).unwrap_or(name);
    let own = (!bus.trim().is_empty()).then(|| installed(bus)).and_then(|p| p.parent().and_then(|d| omsi_vehicle::hof::depot_in(d, name)));
    own.or_else(|| omsi_vehicle::hof::depot_anywhere(name))
}

/// The line editor's line of a company line (its RosterFlow timing points): an own line's, else
/// the map's line as the line editor saved it.
fn design_of(c: &Company, registry: &reg::Registry, line: &str) -> Option<reg::LineDesign> {
    let cl = c.lines.iter().find(|x| x.name.eq_ignore_ascii_case(line));
    let own = cl.filter(|cl| cl.own).and_then(|cl| cl.plan.as_ref()).and_then(|p| registry.line(p.line_id));
    own.or_else(|| registry.map_line(line)).cloned()
}

/// The card of duty `r` (None: the day is still being read, or the plan no longer has the duty).
fn card_of(s: &mut Session, c: &Company, r: &DutyRef) -> Option<(Card, DayPlan)> {
    week_work(s);
    let p = plan_of(s, &r.date)?.ok()?;
    let lines = lines_of(s, &r.date);
    let board = lines.as_ref().map(|l| co::rosterflow::board(&co::rosterflow::view(c), l, &r.date));
    let tt = timetable(&c.map, &r.date);
    let registry = reg::load_registry(&reg::registry_path(&reg::map_folder(&c.map)));
    // (the depot files read once, by the line's: the bus of the duty's tour carries it)
    let bus_of = |line: &str| {
        p.tours.iter().filter(|t| t.tour.line.eq_ignore_ascii_case(line)).find_map(|t| match t.bus {
            Some(co::plan::BusOf::Own(id)) => c.vehicle(id).map(|v| v.bus.clone()),
            _ => None,
        })
    };
    let hofs: RefCell<HashMap<String, Option<Hof>>> = RefCell::new(HashMap::new());
    let stops = |x: &PlannedTrip, line: &str, kurs: &str| tt.as_ref().and_then(|tt| stops_of(tt, line, kurs, x));
    let ibis = |x: &PlannedTrip, line: &str, kurs: &str, st: &[CardStop]| {
        let name = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)).map(|l| l.hof_or(&c.depot).to_string()).unwrap_or_else(|| c.depot.clone());
        let key = line.to_lowercase();
        if !hofs.borrow().contains_key(&key) {
            let h = hof_for(&bus_of(line).unwrap_or_default(), &name);
            hofs.borrow_mut().insert(key.clone(), h);
        }
        hofs.borrow().get(&key).and_then(|h| h.as_ref()).and_then(|h| ibis_of(h, x, kurs, st))
    };
    let design = |line: &str| design_of(c, &registry, line);
    let look = Lookups { stops: &stops, ibis: &ibis, design: &design };
    let player = core::load_config().profile;
    let card = dc::card(c, &p, board.as_ref(), r, co::clock::now(c), &player, &look)?;
    Some((card, p))
}

/// The words of a day group (`lines::DAY_GROUPS`).
fn day_type(k: usize) -> String {
    tr(["Mon - Fri", "Saturday", "Sunday"][k.min(2)])
}

/// What the card says, in the page's language (`p`: the plan of its day, for the blocks' IDs).
pub(crate) fn card_json(c: &Company, card: &Card, p: &DayPlan) -> Value {
    let driver = match &card.driver {
        Driver::Nobody => Value::Null,
        Driver::Player(name) => json!({ "name": if name.trim().is_empty() { tr("You") } else { name.clone() } }),
        Driver::Staff(id, name) => json!({ "name": name, "staff": format!("{id:04}") }),
        Driver::Agency => json!({ "name": tr("Agency driver") }),
        Driver::Guest(name) => json!({ "name": name }),
    };
    let block = |line: &str, tour: &str| p.tours.iter().find(|t| t.tour.line.eq_ignore_ascii_case(line) && t.tour.tour.trim() == tour.trim()).map(|t| super::super::rosterflow::block_label(&t.tour));
    let mut tours: Vec<String> = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    for x in &card.pieces {
        let t = format!("{} {}", x.number.trim(), x.tour.trim());
        if !tours.contains(&t) {
            tours.push(t);
        }
        if let Some(b) = block(&x.line, &x.tour).filter(|b| !blocks.contains(b)) {
            blocks.push(b);
        }
    }
    let buses: Vec<String> = {
        let mut v: Vec<String> = Vec::new();
        for b in card.pieces.iter().filter_map(|x| x.bus.as_ref()) {
            let text = [b.number.trim(), b.plate.trim()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" · ");
            if !text.is_empty() && !v.contains(&text) {
                v.push(text);
            }
        }
        v
    };
    let depots: Vec<String> = card.depots().iter().map(|h| h.strip_suffix(".hof").unwrap_or(h).to_string()).collect();
    // (the codes known: what is unknown is left off)
    let mut codes: Vec<Value> = Vec::new();
    let mut code = |label: &str, value: String| {
        if !value.trim().is_empty() {
            codes.push(json!([tr(label), value]));
        }
    };
    code("Duty no.", card.number.clone().unwrap_or_else(|| format!("{}", card.duty.duty + 1)));
    code("Duty type", card.kind.as_deref().filter(|k| !k.trim().is_empty()).map(tr).unwrap_or_default());
    code("Tour", tours.join(", "));
    code("Block", blocks.join(", "));
    code("Lines", card.lines.join(", "));
    code("Day type", day_type(card.day_group));
    code("Depot", depots.join(", "));
    code("Bus", buses.join(", "));
    let mut times: Vec<Value> = vec![
        json!([tr("Sign on"), hhmm(card.sign_on), card.sign_on_at]),
        json!([tr("Sign off"), hhmm(card.sign_off), card.sign_off_at]),
        json!([tr("Length"), span(card.length()), ""]),
        json!([tr("Driving time"), span(card.driving), ""]),
        json!([tr("Breaks"), span(card.breaks), ""]),
    ];
    if let Some(paid) = card.paid {
        times.push(json!([tr("Paid"), span(paid), ""]));
    }
    let pieces: Vec<Value> = card
        .pieces
        .iter()
        .enumerate()
        .map(|(n, x)| {
            let rows: Vec<Value> = x
                .items
                .iter()
                .map(|it| match it {
                    Item::SignOn { at, time, relief } => json!({ "kind": "on", "time": hhmm(*time), "text": tr(if *relief { "Take the bus over (relief)" } else { "Sign on" }), "at": at }),
                    Item::SignOff { at, time, relief } => json!({ "kind": "off", "time": hhmm(*time), "text": tr(if *relief { "Hand the bus over (relief)" } else { "Sign off" }), "at": at }),
                    Item::Break { at, from, to } => {
                        let mins = to - from;
                        json!({ "kind": "break", "time": hhmm(*from), "until": hhmm(*to), "text": format!("{} · {}", tr(if mins >= 15 { "Break" } else { "Layover" }), trf("%{n} min", &[("n", &mins.to_string())])), "at": at })
                    }
                    Item::Trip { dep, arr, line, from, to, empty, kurs, stops, ibis } => json!({
                        "kind": "trip", "time": hhmm(*dep), "until": hhmm(*arr), "empty": empty,
                        "line": if *empty { tr("Deadhead") } else { line.clone() }, "route": format!("{from} → {to}"), "kurs": kurs,
                        "ibis": ibis.as_ref().map(|i| json!({
                            "line_kurs": match kurs.trim().parse::<u32>() { Ok(k) => format!("{}/{k:02}", i.line), Err(_) => format!("{}/{}", i.line, kurs.trim()) },
                            "suffix": i.suffix, "route": i.route, "dest": i.dest, "dest_text": i.dest_text,
                        })),
                        "stops": stops.iter().map(|s| json!({ "time": hhmm(if s.dep > s.arr { s.dep } else { s.arr }), "arr": (s.dep > s.arr).then(|| hhmm(s.arr)), "name": s.name, "timing": s.timing })).collect::<Vec<_>>(),
                    }),
                })
                .collect();
            let after = x.after.as_ref().map(|g| {
                json!({
                    "time": hhmm(g.from), "until": hhmm(g.to), "text": format!("{} · {}", tr("Break"), span(g.to - g.from)), "at": g.at,
                    "travel": g.travel.iter().map(|(what, a, b)| format!("{} {}–{}", tr(what), hhmm(*a), hhmm(*b))).collect::<Vec<_>>(),
                })
            });
            let bus = x.bus.as_ref().map(|b| [b.number.trim(), b.name.trim(), b.plate.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")).filter(|b| !b.is_empty());
            json!({
                "head": trf("Piece %{n}", &[("n", &(n + 1).to_string())]),
                "what": format!("{} {}  ·  {} {}{}", tr("Line"), x.number.trim(), tr("Tour"), x.tour.trim(), block(&x.line, &x.tour).map(|b| format!("  ·  {} {b}", tr("Block"))).unwrap_or_default()),
                "bus": bus, "rows": rows, "after": after,
            })
        })
        .collect();
    let qr_text = format!("OMSIHUB-DUTY:{}:{}:{}:{}:{}:{}", card.company, card.date, card.duty.line, card.duty.tour, card.duty.duty + 1, card.code);
    let qr = core::qr::encode(&qr_text, core::qr::Ecc::M).or_else(|| core::qr::encode(&qr_text, core::qr::Ecc::L)).map(|q| json!({ "size": q.size, "bits": q.bits() }));
    json!({
        "name": c.name, "mark": super::mark_json(c),
        "title": tr("Duty card"), "date": day_label(&card.date), "iso": card.date,
        "driver_caps": tr("Driver"), "driver": driver, "nobody": tr("Not planned"),
        "codes_caps": tr("Duty"), "codes": codes,
        "times_caps": tr("Times"), "times": times,
        "ibis_caps": tr("IBIS"), "ibis_words": { "line_kurs": tr("Line/course"), "route": tr("Route"), "dest": tr("Destination") },
        "pieces": pieces,
        "code_caps": tr("Check code"), "code": card.code, "qr": qr,
        "printed": tr("Printed"), "by": tr("Printed by Omsi-Hub"),
        "words": {
            "preview": tr("Print duty card"), "paper": tr("Paper"), "stops": tr("Stops"), "timing": tr("Timing points only"), "all": tr("All stops"),
            "show_ibis": tr("Show the IBIS codes"), "print": tr("Print"), "image": tr("Save as image"), "pdf": tr("To save it as a PDF, choose \"Save as PDF\" as the printer."),
            "close": tr("Close"), "a4": "A4",
        },
    })
}

/// The card's view (`co_view` "dutycard", `duty` the duty's day, line, tour and number).
pub fn view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let Ok(r) = serde_json::from_value::<DutyRef>(args.get("duty").cloned().unwrap_or(Value::Null)) else { return json!({ "error": tr("This duty is no longer in the planning") }) };
    match card_of(s, &c, &r) {
        Some((card, p)) => card_json(&c, &card, &p),
        None if plan_of(s, &r.date).is_none() => json!({ "reading": tr("Reading the planning…") }),
        None => json!({ "error": tr("This duty is no longer in the planning") }),
    }
}

/// The card for a player of "Play together" (`company::relayed`): his own duty's whatever his
/// role, another's only when he may see the planning (`relay::RF_VIEW`). Read-only: nothing of
/// it changes the company.
pub fn relayed(s: &mut Session, args: &Value, guest: u32, perms: &[Perm]) -> Result<Value, String> {
    if super::super::together::relay::RF_VIEW.met(perms) {
        return Ok(view(s, args));
    }
    let r: Option<DutyRef> = serde_json::from_value(args.get("duty").cloned().unwrap_or(Value::Null)).ok();
    let mine = r.as_ref().is_some_and(|r| {
        week_work(s);
        matches!(plan_of(s, &r.date), Some(Ok(p)) if r.find(&p).is_some_and(|(t, k)| p.tours[t].duties[k].who == Some(Who::Guest(guest))))
    });
    if !mine {
        return Err(tr(omsi_launcher_lib::company::together::host::NOT_ALLOWED));
    }
    Ok(view(s, args))
}

/// The card as the page drew it (`png`: a PNG in base64) saved where the player chooses: what
/// the page says then.
pub fn save_image(args: &Value) -> Value {
    let bytes = match args.get("png").and_then(Value::as_str).map(super::super::bus3d::photo::base64) {
        Some(Ok(b)) if !b.is_empty() => b,
        _ => return json!({ "error": tr("Could not save the file") }),
    };
    let name = args.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).unwrap_or("duty-card.png");
    let Some(path) = core::save_file_as(&tr("Save the duty card as a picture"), name, "PNG", &["png"]) else { return json!({}) };
    match std::fs::write(&path, &bytes) {
        Ok(()) => json!({ "saved": trf("Saved: %{file}", &[("file", &path.display().to_string())]) }),
        Err(e) => json!({ "error": format!("{}: {e}", tr("Could not save the file")) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::market::{self, MarketBus, Payment};
    use omsi_launcher_lib::company::model::{CompanyLine, GameMode};
    use omsi_launcher_lib::company::network::TourOfDay;
    use omsi_launcher_lib::company::plan::{day_plan, set_bus, set_driver, weekday_of};
    use omsi_launcher_lib::company::{found, Founding};

    fn trip(name: &str, dep: i32, arr: i32, from: &str, to: &str, empty: bool) -> PlannedTrip {
        PlannedTrip { name: name.into(), line: if empty { String::new() } else { "130".into() }, from: from.into(), to: to.into(), dep, arr, km: 8.0, stops: 3, empty, ..Default::default() }
    }

    /// The duty card's view: the codes, the times of signing on and off, the pieces with their
    /// trips, the IBIS codes and the stops marked as timing points, the check code and its QR.
    #[test]
    fn the_duty_card_view_says_the_codes_the_timing_points_and_the_sign_on_and_off() {
        super::super::words::set_language("en");
        let mut c = found(&Founding { name: "Connexion Nederland".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        c.depot = "Grundorf.hof".into();
        c.lines.push(CompanyLine { name: "Linie130".into(), number: "130".into(), numbers: vec!["130".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
        c.cash += 5_000_000_00;
        market::buy_new(&mut c, &MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() }, Payment::Cash, "").unwrap();
        let tours = vec![TourOfDay {
            line: "Linie130".into(),
            number: "130".into(),
            tour: "1".into(),
            trips: vec![trip("Depot-A", 350, 358, "Depot", "Station", true), trip("130 A-B", 360, 390, "Station", "Zentrum", false), trip("B-Depot", 395, 405, "Zentrum", "Depot", true)],
            ..Default::default()
        }];
        let wd = weekday_of(&c.date);
        let id = c.fleet[0].id;
        set_bus(&mut c, wd, "Linie130", "1", Some(id));
        set_driver(&mut c, wd, "Linie130", "1", 0, Some(Who::Player));
        let p = day_plan(&c, &c.date, tours, &[], &[], false);
        let r = DutyRef::of(&p, 0, 0).unwrap();
        let stops = |x: &PlannedTrip, _: &str, _: &str| Some(vec![
            CardStop { id: 1, name: "Station".into(), arr: x.dep, dep: x.dep, hold: true, ..Default::default() },
            CardStop { id: 2, name: "Markt".into(), arr: x.dep + 12, dep: x.dep + 12, hold: false, ..Default::default() },
            CardStop { id: 3, name: "Kerk".into(), arr: x.dep + 20, dep: x.dep + 21, hold: true, ..Default::default() },
            CardStop { id: 4, name: "Zentrum".into(), arr: x.arr, dep: x.arr, hold: true, ..Default::default() },
        ]);
        let ibis = |_: &PlannedTrip, _: &str, kurs: &str, _: &[CardStop]| Some(Ibis { line: "130".into(), route: Some("13001".into()), dest: Some("042".into()), dest_text: "Zentrum".into(), kurs: kurs.into(), suffix: None });
        let look = Lookups { stops: &stops, ibis: &ibis, design: &|_| None };
        let card = dc::card(&c, &p, None, &r, 0, "Luc", &look).unwrap();
        let v = card_json(&c, &card, &p);
        assert_eq!(v["name"], "Connexion Nederland");
        assert_eq!(v["date"], "Mon 4 Mar 2024");
        assert_eq!(v["driver"]["name"], "Luc");
        // the codes: the duty, its tour and block, its line, the day type, the depot, the bus
        let codes: HashMap<String, String> = v["codes"].as_array().unwrap().iter().map(|x| (x[0].as_str().unwrap().to_string(), x[1].as_str().unwrap().to_string())).collect();
        assert_eq!(codes["Duty no."], "1");
        assert_eq!(codes["Tour"], "130 1");
        assert_eq!(codes["Block"], "130/01");
        assert_eq!(codes["Lines"], "130");
        assert_eq!(codes["Day type"], "Mon - Fri");
        assert_eq!(codes["Depot"], "Grundorf");
        assert!(codes["Bus"].contains(c.fleet[0].number.trim()));
        // signing on and off with their places and times
        assert_eq!(v["times"][0], json!(["Sign on", "05:50", "Depot"]));
        assert_eq!(v["times"][1], json!(["Sign off", "06:45", "Depot"]));
        assert_eq!(v["times"][2], json!(["Length", "55 min", ""]));
        let rows = v["pieces"][0]["rows"].as_array().unwrap();
        assert_eq!((rows[0]["kind"].as_str(), rows[0]["time"].as_str()), (Some("on"), Some("05:50")));
        assert_eq!((rows.last().unwrap()["kind"].as_str(), rows.last().unwrap()["time"].as_str()), (Some("off"), Some("06:45")));
        // the trip with passengers: its IBIS codes, its stops with the timing points marked
        let t = rows.iter().find(|r| r["kind"] == "trip" && r["empty"] == false).unwrap();
        assert_eq!(t["ibis"]["line_kurs"], "130/01");
        assert_eq!((t["ibis"]["route"].as_str(), t["ibis"]["dest"].as_str()), (Some("13001"), Some("042")));
        let marked: Vec<(&str, bool)> = t["stops"].as_array().unwrap().iter().map(|s| (s["name"].as_str().unwrap(), s["timing"].as_bool().unwrap())).collect();
        assert_eq!(marked, vec![("Station", true), ("Markt", false), ("Kerk", true), ("Zentrum", true)]);
        assert_eq!((t["stops"][2]["arr"].as_str(), t["stops"][2]["time"].as_str()), (Some("06:20"), Some("06:21")));
        // the deadheads: no codes, no stops
        assert!(rows.iter().filter(|r| r["empty"] == true).all(|r| r["ibis"].is_null() && r["stops"].as_array().unwrap().is_empty()));
        // the check code, and the QR code of the duty
        assert_eq!(v["code"].as_str().map(str::len), Some(9));
        assert!(v["qr"]["size"].as_u64().unwrap() >= 21);
    }

    /// A trip's stops as the game times them: the minutes the map wrote, the rest shared out by
    /// the links' lengths; a station passed without stopping left out.
    #[test]
    fn a_trips_stops_are_timed_as_the_game_times_them() {
        use omsi_timetable::{BusStopEntry, Line, StnLink, Tour, TourTrip, Trip, TripProfile};
        let stop = |id: i64, name: &str| BusStopEntry { name: name.into(), object_id: id, ..Default::default() };
        let link = |a: i64, b: i64, m: f64| StnLink { from_id: a, to_id: b, length: m, ..Default::default() };
        let tt = TimetableData {
            bus_stops: vec![stop(1, "Station"), stop(2, "Markt"), stop(3, "Kerk"), stop(4, "Zentrum"), stop(5, "Depotweg")],
            stn_links: vec![link(1, 2, 1000.0), link(2, 3, 1000.0), link(3, 5, 500.0), link(5, 4, 500.0)],
            // (a 30-minute profile: Kerk at minute 20, Depotweg passed)
            trips: vec![Trip { name: "130 A-B".into(), line: "130".into(), stations: vec![1, 2, 3, 5, 4], profiles: vec![TripProfile { factor: 30.0, man_dep_time: vec![(2, 20.0)], other_stopping: vec![(3, 2)], ..Default::default() }], ..Default::default() }],
            lines: vec![Line { name: "Linie130".into(), tours: vec![Tour { number: "1".into(), trips: vec![TourTrip { trip: "130 A-B".into(), profile: 0, departure: 360.0 }], ..Default::default() }], ..Default::default() }],
            ..Default::default()
        };
        let x = trip("130 A-B", 360, 390, "Station", "Zentrum", false);
        let stops = stops_of(&tt, "Linie130", "1", &x).unwrap();
        let got: Vec<(&str, i32, bool)> = stops.iter().map(|s| (s.name.as_str(), s.dep, s.hold)).collect();
        assert_eq!(got, vec![("Station", 360, false), ("Markt", 370, false), ("Kerk", 380, true), ("Zentrum", 390, false)]);
        // a trip the timetable does not have at that minute: none
        assert!(stops_of(&tt, "Linie130", "1", &trip("130 A-B", 361, 391, "Station", "Zentrum", false)).is_none());
    }
}
