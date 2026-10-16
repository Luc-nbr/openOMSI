//! The income model pinned on the wave-1 reference company (`econ_uk::reference`): Realistic, in
//! the UK, 30 buses on a typical line - its farebox, concession reimbursement and BSOG a month as
//! the data's operator's, its passengers per bus-hour and their peaks; Normal and Casual earning
//! more by their factors, a German company by its country's; a full bus leaving people behind,
//! a late line losing riders, fares at round prices, older files loading.

use super::*;
use crate::company::currency::Currency;
use crate::company::day;
use crate::company::econ_uk::reference::{reference_from, tours_of};
use crate::company::model::{GameMode, VERSION};

/// The month a reference company ran: its income by line, its passengers and their bus-hours,
/// the passengers by the part of the day.
struct Month {
    c: Company,
    fares: f64,
    reimbursement: f64,
    bsog: f64,
    school: f64,
    passengers: f64,
    bus_hours: f64,
    bands: [f64; 5],
}

fn pounds(cents: f64) -> f64 {
    Currency::Gbp.units(cents)
}

/// June 2024 (school days all through, no bank holiday), after May's last fortnight to settle in.
/// (Its standing held at the middle, where the model is centred: the reference's tours are
/// timed tightly - ten minutes' stand - and the game's punctuality, late from three minutes,
/// would take its reputation down by half a point a day, its passengers with it.)
fn june(mode: GameMode, country: &str) -> Month {
    let mut c = reference_from(mode, country, "2024-05-13");
    c.currency = Currency::Gbp;
    let (mut passengers, mut bus_hours, mut bands) = (0.0, 0.0, [0.0; 5]);
    while c.date.as_str() <= "2024-06-30" {
        c.reputation = 50.0;
        let date = c.date.clone();
        let r = day::close_day(&mut c, tours_of(&date), &[]);
        if date.as_str() >= "2024-06-01" {
            passengers += r.passengers as f64;
            bus_hours += r.bus_hours;
            for l in &r.lines {
                for (b, x) in bands.iter_mut().zip(l.bands) {
                    *b += x as f64;
                }
            }
        }
    }
    let m = c.month("2024-06");
    let get = |k: BookingKind| m.get(k) as f64;
    Month {
        fares: get(BookingKind::Fares) + get(BookingKind::ChildFares) + get(BookingKind::NetworkPasses),
        reimbursement: get(BookingKind::Reimbursement),
        bsog: get(BookingKind::Bsog),
        school: get(BookingKind::SchoolPasses),
        c,
        passengers,
        bus_hours,
        bands,
    }
}

#[test]
fn the_reference_company_takes_what_the_data_says() {
    let m = june(GameMode::Realistic, "gb");
    // (the contracts, wave 2a's, are the other 55-60 % of the data's turnover: the shares below
    // are of that turnover, £312.5k a month)
    let total = uk::gbp(uk::YARDSTICK_TURNOVER_MONTH);
    let share = |x: f64| 100.0 * x / total;
    let text = format!(
        "farebox £{:.0} ({:.1} %), concessions £{:.0} ({:.1} %), BSOG £{:.0} ({:.1} %), school passes £{:.0} ({:.1} %); {:.0} passengers, {:.1} a bus-hour; type {:?}; bands {:?}; reputation {:.1}; late {:.2}",
        pounds(m.fares),
        share(m.fares),
        pounds(m.reimbursement),
        share(m.reimbursement),
        pounds(m.bsog),
        share(m.bsog),
        pounds(m.school),
        share(m.school),
        m.passengers,
        m.passengers / m.bus_hours,
        type_of(&m.c.lines[0]),
        m.bands,
        m.c.reputation,
        m.c.lines[0].stats.late,
    );
    println!("{text}");
    // a typical line
    assert_eq!(type_of(&m.c.lines[0]), LineType::Typical, "{text}");
    // the farebox about £56-66k a month, the concession reimbursement £56-62k, BSOG about £7k
    assert!((56_000.0..=66_000.0).contains(&pounds(m.fares)), "{text}");
    assert!((56_000.0..=62_000.0).contains(&pounds(m.reimbursement)), "{text}");
    assert!((6_500.0..=9_000.0).contains(&pounds(m.bsog)), "{text}");
    // their shares of the whole: farebox 18-21 %, concessions 18-20 %, BSOG about 2 %, school
    // passes about 4 %
    assert!((18.0..=21.0).contains(&share(m.fares)), "{text}");
    assert!((18.0..=20.0).contains(&share(m.reimbursement)), "{text}");
    assert!((1.5..=3.0).contains(&share(m.bsog)), "{text}");
    assert!((2.5..=6.0).contains(&share(m.school)), "{text}");
    // with the contracts' 55-60 % they make the turnover
    let ours = share(m.fares + m.reimbursement + m.bsog + m.school) / 100.0;
    assert!((1.0 - uk::YARDSTICK_CONTRACTS.1 - 0.02..=1.0 - uk::YARDSTICK_CONTRACTS.0 + 0.02).contains(&ours), "{ours} {text}");
    // 9-12 passengers a bus-hour over the day
    let per_hour = m.passengers / m.bus_hours;
    assert!((9.0..=12.0).contains(&per_hour), "{text}");
    // the peaks: the morning's two hours busier a bus-hour than the daytime's six (the reference
    // runs as many buses in every hour), the afternoon's three too
    let (am, day, pm) = (m.bands[1] / 2.0, m.bands[2] / 6.0, m.bands[3] / 3.0);
    assert!(am > 1.5 * day && am < 2.1 * day, "{am} {day}");
    assert!(pm > 1.2 * day && pm < am, "{pm} {day} {am}");
}

#[test]
fn the_hours_have_the_datas_shape() {
    // the bands' averages
    let avg = |a: usize, b: usize| (a..b).map(|h| per_bus_hour(0, h as i32 * 60)).sum::<f64>() / (b - a) as f64;
    assert!((avg(7, 9) - uk::PAX_AM_PEAK).abs() < 1e-9);
    assert!((avg(9, 15) - uk::PAX_INTERPEAK).abs() < 1e-9);
    assert!((avg(15, 18) - uk::PAX_PM_PEAK).abs() < 1e-9);
    assert!((avg(5, 7) - uk::PAX_EARLY).abs() < 1e-9 && (avg(18, 24) - uk::PAX_EVENING).abs() < 1e-9);
    // 07:00 at 2.2 times an interpeak hour
    assert!((per_bus_hour(0, 7 * 60) / uk::PAX_INTERPEAK - 2.2).abs() < 1e-9);
    // a Saturday about 8.5 through its day, a Sunday about 0.35 of a weekday (an assumption)
    let day = |k: usize| (6..20).map(|h| per_bus_hour(k, h as i32 * 60)).sum::<f64>() / 14.0;
    assert!((day(1) - uk::PAX_SATURDAY).abs() < 0.6, "{}", day(1));
    assert!((day(2) / day(0) - uk::SUNDAY_SHARE).abs() < 0.06, "{}", day(2) / day(0));
    // the days: a Monday in June has school, one in August not; Saturdays, Sundays and Easter
    // Monday are what they are
    assert_eq!(day_of("2024-06-03"), DayOf { kind: 0, school: true });
    assert_eq!(day_of("2024-08-05"), DayOf { kind: 0, school: false });
    assert_eq!(day_of("2024-06-08").kind, 1);
    assert_eq!(day_of("2024-06-09").kind, 2);
    assert_eq!(day_of("2024-04-01").kind, 2);
    // without school the pupils stay at home: fewer ride, and none on a school pass
    let t = LineType::Typical;
    let (with, without) = (trip_demand(t, day_of("2024-06-03"), 8 * 60, 50.0), trip_demand(t, day_of("2024-08-05"), 8 * 60, 50.0));
    assert!((without / with - (1.0 - uk::MIX_SCHOOL)).abs() < 1e-9);
    assert_eq!(t.kind().mix.without_school().school, 0.0);
    // a school run carries its pupils on school days only, a shuttle its staff at the shifts
    assert!((trip_demand(LineType::School, day_of("2024-06-03"), 7 * 60 + 30, 30.0) - uk::PER_TRIP_SCHOOL).abs() < 1e-9);
    assert!(trip_demand(LineType::School, day_of("2024-08-05"), 7 * 60 + 30, 30.0) < 5.0);
    assert!(trip_demand(LineType::Shuttle, day_of("2024-06-03"), 7 * 60, 20.0) > 2.0 * trip_demand(LineType::Shuttle, day_of("2024-06-03"), 11 * 60, 20.0));
    // an urban all-day line about 28 a bus-hour, a park & ride trip 3-4
    let urban = (6..22).map(|h| trip_demand(LineType::Urban, day_of("2024-06-04"), h * 60, 50.0)).sum::<f64>() / (16.0 * bus_hours(50.0));
    assert!((24.0..=32.0).contains(&urban), "{urban}");
    let pr = trip_demand(LineType::ParkRide, day_of("2024-06-04"), 11 * 60, 15.0);
    assert!((2.0..=4.5).contains(&pr), "{pr}");
}

#[test]
fn a_line_is_the_kind_its_timetable_shows_or_the_players() {
    let mut l = CompanyLine { name: "x".into(), ..Default::default() };
    assert_eq!(type_of(&l), LineType::Typical);
    // a short line of close stops all day long: urban; far-apart stops: rural
    l.stats = LineStats { trip_km: 8.0, stops: 22.0, trips: 120.0, span: 17.0, first: 5.5, school_hours: 0.2, late: 0.0 };
    assert_eq!(type_of(&l), LineType::Urban);
    l.stats = LineStats { trip_km: 30.0, stops: 10.0, trips: 14.0, span: 12.0, first: 7.0, school_hours: 0.3, late: 0.0 };
    assert_eq!(type_of(&l), LineType::Rural);
    // a few trips in the school's hours: a school run; one from the evening on: evening
    l.stats = LineStats { trip_km: 12.0, stops: 15.0, trips: 4.0, span: 9.0, first: 7.5, school_hours: 1.0, late: 0.0 };
    assert_eq!(type_of(&l), LineType::School);
    l.stats = LineStats { trip_km: 12.0, stops: 15.0, trips: 10.0, span: 5.0, first: 18.5, school_hours: 0.0, late: 0.0 };
    assert_eq!(type_of(&l), LineType::Evening);
    // the player's word wins
    l.line_type = Some(LineType::ParkRide);
    assert_eq!(type_of(&l), LineType::ParkRide);
    // a weekend's fewer trips do not make a line another kind
    let mut s = LineStats::default();
    let day = |n: usize| Seen { trips: (0..n).map(|k| (8.0, 22, 5 * 60 + k as i32 * 8, 5 * 60 + k as i32 * 8 + 30)).collect(), run: n as u32, late: 0 };
    observe(&mut s, &day(130));
    observe(&mut s, &day(20));
    assert!(s.trips > 120.0 && s.span > 16.0, "{s:?}");
}

#[test]
fn the_mix_brings_what_each_rider_pays() {
    let mut c = reference_from(GameMode::Realistic, "gb", "2024-06-03");
    c.currency = Currency::Gbp;
    let l = c.lines[0].clone();
    let r = Rates::of(&c, &l, "2024-06-04");
    // a concession ride: 60 % of the adult single; a paying adult the yield; a child 0.55 of it
    assert!((r.reimbursement / r.single - uk::CONCESSION_SHARE).abs() < 1e-9);
    assert!((r.adult / r.single - uk::YIELD_PER_PAYING / uk::MEAN_SINGLE).abs() < 1e-9);
    assert!(r.child < r.adult * 0.6 && r.child > r.adult * 0.5);
    // a typical 10 km trip £2.40 a single; a network pass about £1, a school pass about £1; a
    // concession ride about £1.95 at the data's mean single of £3.13
    let typical = distance_single(&c, LineType::Typical, uk::TYPICAL_TRIP_KM);
    assert!((pounds(typical) - 2.40).abs() < 0.01, "{}", pounds(typical));
    assert!((pounds(r.network) - 1.00).abs() < 0.01 && (pounds(r.school) - 1.00).abs() < 0.03, "{} {}", pounds(r.network), pounds(r.school));
    assert!((pounds(uk::CONCESSION_SHARE * uk::gbp(uk::MEAN_SINGLE)) - uk::CONCESSION_PER_BOARDING / 100.0).abs() < 0.03);
    // rural lines more concessions (up to about 60 %), a shuttle's riders pay nothing at the box
    assert!(LineType::Rural.kind().mix.concession >= 0.5 && LineType::Rural.kind().mix.concession <= uk::CONCESSION_RANGE.1);
    let mut shuttle = l.clone();
    shuttle.line_type = Some(LineType::Shuttle);
    assert_eq!(Rates::of(&c, &shuttle, "2024-06-04").per_boarding(), 0.0);
    // BSOG on live kilometres only: 5.9p a mile
    let t = r.takings(0.0, uk::KM_PER_MILE * 1000.0);
    assert!((pounds(t.bsog) - 59.0).abs() < 0.05, "{}", pounds(t.bsog));
    // the fare table: a return 1.6 singles, a child's 0.55, a cap at £2.90 / £1.50
    let mut capped = l.clone();
    capped.stats.trip_km = 40.0;
    capped.fare_cap = true;
    let tb = table(&c, &capped, crate::company::fares::fare_of(&c, &capped) as f64);
    assert!(tb.ret > 1.55 * tb.single && tb.ret < 1.65 * tb.single);
    let (a, k) = tb.paid();
    assert!((Currency::Gbp.fare_units(a) - 2.90).abs() < 1e-9 && (Currency::Gbp.fare_units(k) - 1.50).abs() < 1e-9, "{a} {k}");
}

#[test]
fn gbp_fares_are_round_pounds_and_pence() {
    let mut c = reference_from(GameMode::Realistic, "gb", "2024-06-03");
    c.currency = Currency::Gbp;
    for km in [2.0, 7.5, 10.0, 13.3, 24.0, 37.7, 61.0, 90.0] {
        let mut l = c.lines[0].clone();
        l.stats.trip_km = km;
        for t in LineType::ALL {
            l.line_type = Some(t);
            let fare = crate::company::fares::fare_of(&c, &l);
            let table = table(&c, &l, fare as f64);
            for x in [fare as f64, table.single, table.ret, table.child, table.day, table.week] {
                // what is shown is a 5p step, and what is kept shows back as it
                let shown = Currency::Gbp.fare_units(x);
                assert!(((shown * 20.0).round() - shown * 20.0).abs() < 1e-9, "{km} {t:?} {shown}");
                assert!((Currency::Gbp.units(x) - shown).abs() < 0.005, "{km} {t:?} {x} {shown}");
            }
        }
    }
    // a fare set by the player: kept at the step too
    let name = c.lines[0].name.clone();
    let set = crate::company::fares::set_fare(&mut c, &name, 281).unwrap();
    assert!((Currency::Gbp.units(set as f64) - Currency::Gbp.fare_units(set as f64)).abs() < 0.005, "{set}");
    assert!((Currency::Gbp.fare_units(set as f64) - 2.40).abs() < 1e-9, "{set}");
}

#[test]
fn normal_and_casual_earn_more_by_their_factors() {
    let real = june(GameMode::Realistic, "gb");
    let normal = june(GameMode::Normal, "gb");
    let casual = june(GameMode::Casual, "gb");
    // (a passenger brings the mode's factor more; how many ride is the days' own)
    let per = |m: &Month| (m.fares + m.reimbursement + m.school) / m.passengers;
    let (r, n, k) = (per(&real), per(&normal), per(&casual));
    assert!((n / r - 1.15).abs() < 0.04, "{}", n / r);
    assert!((k / r - 1.35).abs() < 0.05, "{}", k / r);
    // and in all
    assert!(normal.fares + normal.reimbursement > real.fares + real.reimbursement);
    assert!(casual.fares + casual.reimbursement > normal.fares + normal.reimbursement);
    // BSOG by the income factor too, a kilometre
    let bsog = |m: &Month| m.bsog / m.c.history.iter().filter(|d| d.date.starts_with("2024-06")).map(|d| d.km).sum::<f64>();
    assert!((bsog(&normal) / bsog(&real) - 1.15).abs() < 0.02, "{}", bsog(&normal) / bsog(&real));
}

#[test]
fn a_german_company_takes_by_its_country() {
    let uk_co = reference_from(GameMode::Realistic, "gb", "2024-06-03");
    let de = reference_from(GameMode::Realistic, "de", "2024-06-03");
    let land = uk::country("de");
    let (a, b) = (Rates::of(&uk_co, &uk_co.lines[0], "2024-06-04"), Rates::of(&de, &de.lines[0], "2024-06-04"));
    // the fares by its fares' level (each a round price: within a step), BSOG by its diesel's
    assert!((b.single / a.single - land.fares).abs() < 0.03, "{}", b.single / a.single);
    assert!((b.per_boarding() / a.per_boarding() - land.fares).abs() < 0.03);
    assert!((b.bsog_km / a.bsog_km - land.diesel).abs() < 1e-9);
}

#[test]
fn a_full_bus_leaves_people_behind() {
    use crate::company::market::{self, MarketBus, Payment};
    use crate::company::model::BusSize;
    use crate::company::network::{PlannedTrip, TourOfDay};
    let mut c = reference_from(GameMode::Realistic, "gb", "2024-06-03");
    // a midibus on a busy urban line at the morning's peak, its trips long
    c.fleet.clear();
    // (the reference's model, made a midibus: its drivers know it)
    let bus = MarketBus { file: "Vehicles/SingleDeck/SingleDeck.bus".into(), name: "Single-deck".into(), ..Default::default() };
    let id = market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    c.fleet.iter_mut().find(|v| v.id == id).unwrap().kind.size = BusSize::Midi;
    c.lines[0].line_type = Some(LineType::Urban);
    let trip = |k: i32| PlannedTrip { name: format!("t{k}"), line: "5".into(), from: "A".into(), to: "B".into(), dep: 7 * 60 + k * 100, arr: 7 * 60 + k * 100 + 95, km: 30.0, stops: 40, empty: false, origin: String::new(), last: String::new(), part: None };
    let tour = TourOfDay { line: c.lines[0].name.clone(), number: "5".into(), tour: "1".into(), ai_group: String::new(), trips: vec![trip(0), trip(1)], unplanned: false, cuts: None };
    let r = day::close_day(&mut c, vec![tour], &[]);
    assert!(r.left_behind > 0 && r.crowded > 0, "{} {} {} {} {} {:?}", r.left_behind, r.crowded, r.passengers, r.covered, r.dropped, r.drops.first());
    assert_eq!(r.lines[0].left_behind, r.left_behind);
    // what it took fits the bus at its busiest point
    let cap = crate::company::ownline::capacity(BusSize::Midi) * crate::company::ownline::CRUSH;
    let most = r.passengers as f64 / 2.0 * crate::company::ownline::ON_BOARD * crate::company::ownline::PEAK_DIRECTION;
    assert!(most <= cap + 1.0, "{most} {cap}");
    // and the day's history keeps them
    assert_eq!(c.history.last().unwrap().left_behind, r.left_behind);
}

#[test]
fn a_line_late_often_loses_riders_slowly_and_wins_them_back() {
    let mut s = LineStats::default();
    let late = Seen { trips: Vec::new(), run: 100, late: 60 };
    let on_time = Seen { trips: Vec::new(), run: 100, late: 2 };
    observe(&mut s, &late);
    // gently: a day of it costs next to nothing
    assert!(punctual_factor(&s) > 0.99, "{}", punctual_factor(&s));
    for _ in 0..90 {
        observe(&mut s, &late);
    }
    let low = punctual_factor(&s);
    assert!((1.0 - uk::LATE_MOST..0.9).contains(&low), "{low}");
    // back on time: the riders return, as slowly
    for _ in 0..10 {
        observe(&mut s, &on_time);
    }
    assert!(punctual_factor(&s) > low && punctual_factor(&s) < 1.0);
    for _ in 0..200 {
        observe(&mut s, &on_time);
    }
    assert!((punctual_factor(&s) - 1.0).abs() < 1e-9);
}

#[test]
fn a_gross_cost_line_takes_its_fares_for_the_authority() {
    let mut c = reference_from(GameMode::Realistic, "gb", "2024-06-03");
    c.lines[0].fares_to_authority = true;
    let date = c.date.clone();
    let cash = c.cash;
    let r = day::close_day(&mut c, tours_of(&date), &[]);
    let m = c.month("2024-06");
    // collected and paid over: nothing of it the company's
    assert!(r.takings.for_authority > 0 && r.lines[0].takings.for_authority > 0);
    assert_eq!(m.get(BookingKind::AuthorityFares), 0);
    assert!(c.ledger.iter().any(|b| b.kind == BookingKind::AuthorityFares && b.amount > 0) && c.ledger.iter().any(|b| b.kind == BookingKind::AuthorityFares && b.amount < 0));
    assert_eq!((m.get(BookingKind::Fares), m.get(BookingKind::Reimbursement), m.get(BookingKind::Bsog)), (0, 0, 0));
    assert_eq!(r.takings.income(), 0);
    assert_eq!(c.cash - cash, r.result);
}

#[test]
fn an_old_save_loads_with_its_fares_on_the_new_footing() {
    let mut c = reference_from(GameMode::Realistic, "gb", "2024-06-03");
    c.lines[0].own = true;
    let date = c.date.clone();
    day::close_day(&mut c, tours_of(&date), &[]);
    let mut v = serde_json::to_value(&c).unwrap();
    // a file of version 2: a line's fare a single of the old reference's (Realistic €1.90 the
    // association's, this one 20 % above it), no stats nor kind of line, a history without
    // bus-hours
    v["version"] = serde_json::json!(2);
    v["lines"][0]["fare"] = serde_json::json!(228);
    for k in ["stats", "line_type", "fare_cap", "fares_to_authority"] {
        v["lines"][0].as_object_mut().unwrap().remove(k);
    }
    for d in v["history"].as_array_mut().unwrap() {
        d.as_object_mut().unwrap().remove("bus_hours");
        d.as_object_mut().unwrap().remove("left_behind");
        for l in d["lines"].as_array_mut().unwrap() {
            l.as_object_mut().unwrap().remove("bands");
        }
    }
    v["last_report"].as_object_mut().unwrap().remove("takings");
    let old: Company = serde_json::from_value(v).unwrap();
    let new = crate::company::store::migrate(old);
    assert_eq!(new.version, VERSION);
    assert_eq!(new.lines[0].stats, LineStats::default());
    // 20 % above the line's standard fare now, a round price
    let p0 = crate::company::fares::standard_fare(&new, &new.lines[0]) as f64;
    let fare = new.lines[0].fare.unwrap() as f64;
    assert!((fare / p0 - 1.2).abs() < 0.02, "{fare} {p0}");
    // and it runs a day
    let mut new = new;
    new.planning.auto = true;
    let date = new.date.clone();
    let r = day::close_day(&mut new, tours_of(&date), &[]);
    assert!(r.passengers > 0 && r.takings.income() > 0);
}

