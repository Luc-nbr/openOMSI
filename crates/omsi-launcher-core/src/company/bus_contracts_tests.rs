//! The bus contracts' tests: the tender's scores, the cost formula, the payments, the deductions
//! and default points, the term, and older files.

use super::*;
use crate::company::auction;
use crate::company::concessions::{self as cn, Concession, Outcome};
use crate::company::currency::Currency;
use crate::company::model::{CompanyLine, GameMode};
use crate::company::{found, Founding};

fn company(m: GameMode) -> Company {
    found(&Founding { name: "Kontrakt".into(), game_mode: m, date: "2024-03-04".into(), ..Default::default() }, "Luc")
}

/// An all-day line of one bus: 14 hours and 280 km a day, 20 journeys, six days a week.
fn work() -> Work {
    Work { hours: 14.0, km: 280.0, journeys: 20, buses: 1, days: 6 * 52 }
}

/// A contract on "Linie5" from `from` for three years at `year` a year.
fn with_contract(c: &mut Company, kind: Kind, period: Period, year: Cents, from: &str) -> u32 {
    let until = dates::add(from, 3 * 365 - 1);
    let k = make(c, "Linie5", "5", kind, period, year, &work(), from, &until);
    let id = k.id;
    c.concessions.contracts.push(k);
    if !c.lines.iter().any(|l| l.name == "Linie5") {
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), ..Default::default() });
    }
    id
}

/// The day's close of the contracts with these journeys (the line took 100 € in fares and 50 €
/// a km's payment).
fn close(c: &mut Company, journeys: &Journeys) -> (DayReport, LineDay) {
    let mut lines = vec![LineDay { line: "Linie5".into(), number: "5".into(), fares: 100_00, compensation: 50_00, revenue: 150_00, ..Default::default() }];
    let mut r = DayReport::default();
    let date = c.date.clone();
    close_day(c, &date, journeys, &mut lines, &mut r);
    (r, lines.remove(0))
}

/// 20 journeys of the line: one not run, one 12 minutes late, one 7 late, the rest on time.
fn a_day() -> Journeys {
    let mut j = Journeys::default();
    j.missed("Linie5", "1");
    j.ran("Linie5", "1", 12, false, None, None, None);
    j.ran("Linie5", "1", 7, false, None, None, None);
    for _ in 0..17 {
        j.ran("Linie5", "2", 1, false, None, None, None);
    }
    j
}

fn booked(c: &Company, kind: BookingKind) -> Cents {
    let date = c.date.clone();
    c.ledger.iter().filter(|b| b.date == date && b.kind == kind).map(|b| b.amount).sum()
}

#[test]
fn the_price_score_is_the_lowest_bid_over_this_one_of_seventy() {
    // the lowest bid all 70, another lowest ÷ its own of them
    assert_eq!(auction::price_points(200_000_00, 200_000_00), 70.0);
    assert!((auction::price_points(250_000_00, 200_000_00) - 56.0).abs() < 1e-9);
    assert!((auction::price_points(400_000_00, 200_000_00) - 35.0).abs() < 1e-9);
    // equal totals: the better price points first, then the better quality
    use auction::{Bidder, Character, Placed, Who};
    let rival = |q| Bidder { character: Character::Big, floor: 0, quality: q, seed: 1, opening: None, quiet_until: None };
    let placed = [Placed { at: 0, who: Who::Rival(0), amount: 100_000_00 }, Placed { at: 1, who: Who::Rival(1), amount: 100_000_00 }];
    let pts = auction::points(&placed, &[rival(80.0), rival(90.0)], 50.0);
    assert_eq!(pts[0].who, Who::Rival(1), "the same price: the better quality wins");
}

#[test]
fn quality_comes_from_the_track_record() {
    // 100 % less 2 % a point: 30 points clean, 27 with 5, none from 50
    assert_eq!(quality_points(0.0), 30.0);
    assert!((quality_points(5.0) - 27.0).abs() < 1e-9);
    assert_eq!(quality_points(50.0), 0.0);
    assert_eq!(quality_points(80.0), 0.0);
    // a newcomer is scored as a fair record
    let mut c = company(GameMode::Realistic);
    assert_eq!(track_record(&c), None);
    assert!((quality(&c) - 100.0 * (1.0 - 0.02 * econ_uk::NEW_OPERATOR_POINTS)).abs() < 1e-9);
    assert!((cn::quality(&c) - quality(&c)).abs() < 1e-9);
    // two contracts, 10 net points and none: an average of 5
    let a = with_contract(&mut c, Kind::Gross, Period::Day, 100_000_00, "2024-01-01");
    with_contract(&mut c, Kind::Net, Period::Month, 50_000_00, "2024-02-01");
    let today = c.date.clone();
    let k = c.concessions.contracts.iter_mut().find(|k| k.id == a).unwrap();
    k.record.push(PointsDay { date: dates::add(&today, -10), default: 14.0, ..Default::default() });
    k.record.push(PointsDay { date: dates::add(&today, -5), quality: 4.0, ..Default::default() });
    assert_eq!(track_record(&c), Some(5.0));
    assert!((quality(&c) - 90.0).abs() < 1e-9);
    // a serious default wipes the quality points out; a year on, the points are gone
    let k = c.concessions.contracts.iter_mut().find(|k| k.id == a).unwrap();
    k.record.push(PointsDay { date: dates::add(&today, -3), serious: 1, default: 20.0, ..Default::default() });
    assert_eq!(k.net_points(&today), 34.0);
    assert_eq!(k.net_points(&dates::add(&today, 400)), 0.0);
}

#[test]
fn the_cost_formula_is_a_uk_operators() {
    // an all-day bus: some £41 a timetabled hour (£29-49), without inflation
    let b = Basis::uk(0.0);
    let year = cost_pence(&work(), Kind::Gross, 3.0, &b);
    let per_hour = year / 100.0 / (work().hours * work().days as f64);
    assert!((35.0..49.0).contains(&per_hour), "£{per_hour:.2} an hour");
    // a school run's coach (about 3 h and 60 km on 190 days): some £300-350 a school day
    let school = Work { hours: 3.0, km: 60.0, journeys: 2, buses: 1, days: 190 };
    let day = cost_pence(&school, Kind::Hire, 3.0, &b) / 100.0 / 190.0;
    assert!((280.0..380.0).contains(&day), "£{day:.0} a day");
    // peak-only work costs more an hour than all-day work; extra miles are cheap
    assert!(day / 3.0 > per_hour);
    let longer = Work { km: 330.0, ..work() };
    assert!(cost_pence(&longer, Kind::Gross, 3.0, &b) / year < 1.1);
    // a net-subsidy bid takes the fares it expects off; inflation over the term raises it
    assert!(cost_pence(&work(), Kind::Net, 3.0, &b) < year);
    assert!(cost_pence(&work(), Kind::Gross, 3.0, &Basis::uk(0.03)) > year);
    // the company's world: a Realistic company in the UK pays the data's, Normal is paid more
    let r = company(GameMode::Realistic);
    let n = company(GameMode::Normal);
    let gbp = |x: f64| Currency::Gbp.units(x);
    let realistic = gbp(price_of(&r, &work(), Kind::Gross, 3.0));
    assert!((realistic / (cost_pence(&work(), Kind::Gross, 3.0, &Basis::uk(economy_inflation(&r))) / 100.0) - econ_uk::factor(&r, Item::Contracts)).abs() < 0.01);
    assert!(price_of(&n, &work(), Kind::Gross, 3.0) > price_of(&r, &work(), Kind::Gross, 3.0));
    // the advice: the same formula at the company's own costs (Normal's cheaper than its price)
    assert!(advice(&n, &work(), Kind::Gross, 3.0) < price_of(&n, &work(), Kind::Gross, 3.0));
    // a timetable's days: "Mon-Sat" six, "daily" seven, days named
    assert_eq!(weekdays("Mon-Sat").count_ones(), 6);
    assert_eq!(weekdays("daily & holidays").count_ones(), 7);
    assert_eq!(weekdays("Sat, Sun, school days"), 0x60);
}

fn economy_inflation(c: &Company) -> f64 {
    crate::company::economy::rules_for(c).inflation
}

#[test]
fn a_tender_has_a_cap_and_the_rivals_bid_around_the_cost_price() {
    let lines = vec![cn::tests::line("Linie7", "7", 2, true)];
    for (m, (lo, hi)) in [(GameMode::Realistic, (-0.15, 0.15)), (GameMode::Normal, (0.0, 0.20)), (GameMode::Casual, (0.10, 0.30))] {
        let mut c = company(m);
        c.cash = 10_000_000_00;
        c.clock.minute = 9 * 60;
        let id = cn::apply(&mut c, &lines[0]).unwrap();
        let t = c.concessions.tenders.iter().find(|t| t.id == id).unwrap().clone();
        // the cap: the authority's cost price and 20 %
        assert!(t.cost > 0 && (t.reference as f64 / t.cost as f64 - econ_uk::TENDER_CAP).abs() < 0.01, "{m:?}");
        assert!(t.value > 0 && t.years == 3 && t.weeks == 156);
        // every rival's floor within the mode's band around the cost price, by its character
        for b in cn::bidders(&c, &t) {
            let share = b.floor as f64 / t.cost as f64 - 1.0;
            assert!(share >= lo - 0.01 && share <= hi + 0.01, "{m:?}: {share}");
        }
        // the auction: every bid at most the cap (a floor above it never bids), and none below
        // the band
        let placed = cn::bids(&c, &t, t.closes_at);
        assert!(placed.iter().all(|p| t.counts(p.amount) && p.amount as f64 >= t.cost as f64 * (1.0 + lo) - auction::ROUND as f64), "{m:?}");
        // a bid above the cap does not count
        assert_eq!(cn::bid(&mut c, id, t.reference + auction::ROUND), Err("Above the authority's cap: a bid above it does not count."));
        // the advice: the formula at the company's own costs, within the cap
        let a = cn::advice(&c, &t);
        assert!(a > 0 && a <= t.reference);
    }
    // Casual's rivals bid high: its cautious ones over the cap do not bid at all
    let mut c = company(GameMode::Casual);
    c.clock.minute = 9 * 60;
    let id = cn::apply(&mut c, &lines[0]).unwrap();
    let t = c.concessions.tenders.iter().find(|t| t.id == id).unwrap().clone();
    let (lo, _) = cn::floor_range(&c, &t, auction::Character::Cautious);
    assert!(lo >= t.reference as f64 - auction::ROUND as f64);
}

#[test]
fn a_gross_cost_contract_is_paid_by_the_day_less_its_journeys_not_paid() {
    let mut c = company(GameMode::Realistic);
    let today = c.date.clone();
    let year = 312_000_00;
    with_contract(&mut c, Kind::Gross, Period::Day, year, &today);
    assert!(fares_to_authority(&c, "Linie5") && under_contract(&c, "Linie5"));
    let (r, l) = close(&mut c, &a_day());
    // a day's price: the year's over its 312 days; the journey not run and the one 12 minutes
    // late are not paid (each a twentieth of the day), the one 7 late is
    let day = year as f64 / 312.0;
    let deducted = (day / 20.0 * 2.0).round() as Cents;
    assert_eq!(r.contracts.paid, day.round() as Cents);
    assert_eq!((r.contracts.unpaid, r.contracts.deducted), (2, deducted));
    assert_eq!(booked(&c, BookingKind::Contract), day.round() as Cents - deducted);
    // the fares go to the authority, and no payment per km besides the contract
    assert_eq!((booked(&c, BookingKind::Fares), booked(&c, BookingKind::Compensation)), (-100_00, -50_00));
    assert_eq!((l.fares, l.compensation, l.contract, l.penalty), (0, 0, day.round() as Cents, deducted));
    // the points: 10 for the journey not run, 5 for 12 late, 3 for 7 late
    assert_eq!(r.contracts.points, 18.0);
    assert_eq!(r.contracts.lines[0].rag, Rag::Orange);
    // a day the line does not run pays nothing
    let (r, _) = close(&mut c, &Journeys::default());
    assert_eq!(r.contracts.paid, 0);
    // what a journey not paid costs, as the planning reckons it: a twentieth of the day
    assert_eq!(journey_value(&c, "Linie5", None), (day / 20.0).round() as Cents);
    // a line without a contract: nothing of it
    assert_eq!(journey_value(&c, "Linie9", None), 0);
}

#[test]
fn a_weekly_contract_is_paid_on_sundays() {
    let mut c = company(GameMode::Realistic);
    c.date = "2024-03-10".into();
    with_contract(&mut c, Kind::Gross, Period::Week, 52_000_00, "2024-03-04");
    let (r, _) = close(&mut c, &a_day());
    assert_eq!(r.contracts.paid, 1_000_00);
    c.date = "2024-03-11".into();
    let (r, _) = close(&mut c, &a_day());
    assert_eq!(r.contracts.paid, 0, "a Monday");
}

#[test]
fn a_net_subsidy_is_paid_by_the_month_and_its_fares_are_the_companys() {
    let mut c = company(GameMode::Realistic);
    c.date = "2024-03-15".into();
    let id = with_contract(&mut c, Kind::Net, Period::Month, 120_000_00, "2024-03-01");
    assert!(!fares_to_authority(&c, "Linie5") && under_contract(&c, "Linie5"));
    let mut on_time = Journeys::default();
    on_time.ran("Linie5", "1", 0, false, None, None, None);
    let (r, l) = close(&mut c, &on_time);
    assert_eq!(r.contracts.paid, 0, "mid-month");
    assert_eq!(l.fares, 100_00, "the fares are the company's");
    c.date = "2024-03-31".into();
    let (r, _) = close(&mut c, &on_time);
    assert_eq!(r.contracts.paid, 10_000_00, "a twelfth of the year");
    assert_eq!(c.concessions.contracts.iter().find(|k| k.id == id).unwrap().paid.last().map(|p| p.1), Some(10_000_00 * 1 + 0));
    // a school run's subsidy is paid in eleven months, none in August
    let mut s = company(GameMode::Realistic);
    s.date = "2024-08-31".into();
    let id = with_contract(&mut s, Kind::Net, Period::Month, 110_000_00, "2024-08-01");
    s.concessions.contracts.iter_mut().find(|k| k.id == id).unwrap().paid_months = 11;
    assert_eq!(close(&mut s, &on_time).0.contracts.paid, 0);
    s.date = "2024-09-30".into();
    assert_eq!(close(&mut s, &on_time).0.contracts.paid, 10_000_00);
    // its fares may rise by 15 % a year at most
    c.date = "2024-04-02".into();
    let base = c.concessions.contracts.iter().find(|k| k.kind == Kind::Net).unwrap().fare_base.unwrap();
    assert_eq!(fare_cap(&c, "Linie5"), Some((base as f64 * 1.15).floor() as Cents));
    let before = crate::company::fares::set_fare(&mut c, "Linie5", base * 2).unwrap();
    assert!(before <= fare_cap(&c, "Linie5").unwrap());
}

#[test]
fn normal_and_casual_are_milder() {
    let mut n = company(GameMode::Normal);
    let today = n.date.clone();
    with_contract(&mut n, Kind::Gross, Period::Day, 312_000_00, &today);
    let (r, _) = close(&mut n, &a_day());
    // Normal: a journey 12 minutes late is paid (unpaid from 15), the points halved
    assert_eq!(r.contracts.unpaid, 1);
    assert_eq!(r.contracts.points, 9.0);
    let mut c = company(GameMode::Casual);
    with_contract(&mut c, Kind::Gross, Period::Day, 312_000_00, &today);
    let (r, _) = close(&mut c, &a_day());
    // Casual: nothing deducted, no points
    assert_eq!((r.contracts.unpaid, r.contracts.deducted, r.contracts.points), (0, 0, 0.0));
    assert_eq!(r.contracts.paid, 1_000_00);
    assert_eq!(journey_value(&c, "Linie5", None), 0);
    // and the early and late journeys by the mode
    assert!(unpaid_at(&company(GameMode::Realistic), -5) && unpaid_at(&company(GameMode::Realistic), 10));
    assert!(!unpaid_at(&n, 14) && unpaid_at(&n, 15) && unpaid_at(&n, -5));
    assert!(!unpaid_at(&c, 60) && !unpaid_at(&c, -10));
    assert_eq!([time_points(-6), time_points(-3), time_points(-1), time_points(4), time_points(5), time_points(10), time_points(11)], [5.0, 3.0, 0.0, 0.0, 3.0, 3.0, 5.0]);
}

#[test]
fn a_smaller_bus_an_unroadworthy_one_and_a_driver_without_the_licence_cost_points() {
    let mut c = company(GameMode::Realistic);
    let today = c.date.clone();
    with_contract(&mut c, Kind::Gross, Period::Day, 312_000_00, &today);
    let bus = crate::company::market::MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    c.cash += 1_000_000_00;
    crate::company::market::buy_new(&mut c, &bus, crate::company::market::Payment::Cash, "").unwrap();
    let id = c.fleet[0].id;
    c.fleet[0].condition = 20.0;
    let mut j = Journeys::default();
    // (a solo bus on a tour that wants a double-decker, twice: once a tour)
    j.ran("Linie5", "1", 0, false, Some(id), None, Some(BusSize::Double));
    j.ran("Linie5", "1", 0, false, Some(id), None, Some(BusSize::Double));
    let (r, _) = close(&mut c, &j);
    // 10 for the wrong bus, 20 for running it unroadworthy
    assert_eq!(r.contracts.points, 30.0);
    assert_eq!(r.contracts.lines[0].rag, Rag::Red);
}

#[test]
fn points_make_a_contract_orange_or_red_no_extension_and_an_end() {
    let mut c = company(GameMode::Realistic);
    c.date = "2024-01-01".into();
    let id = with_contract(&mut c, Kind::Gross, Period::Day, 312_000_00, "2024-01-01");
    let until = c.concessions.contracts[0].until.clone();
    c.concessions.held.push(Concession { line: "Linie5".into(), number: "5".into(), from: "2024-01-01".into(), until: until.clone(), price: 0.9, direct: false });
    // green at its decision: a year longer, the concession with it
    c.date = dates::add(&until, -30);
    let ev = extend_due(&mut c);
    let k = c.concessions.contracts.iter().find(|k| k.id == id).unwrap().clone();
    assert!(matches!(&ev[..], [Event::Extended { .. }]));
    assert_eq!((k.extended, k.extensions_left), (1, econ_uk::TERM_LOCAL.1 - 1));
    assert_eq!(cn::of_line(&c, "Linie5").unwrap().until, k.until);
    assert!(extend_due(&mut c).is_empty(), "decided once");
    // orange (16 points) at the next: no extension, none after
    let today = dates::add(&c.date, 10);
    c.concessions.contracts[0].record.push(PointsDay { date: today.clone(), default: 17.0, ..Default::default() });
    assert_eq!(c.concessions.contracts[0].rag(&today), Rag::Orange);
    c.date = dates::add(&c.concessions.contracts[0].until, -30);
    let ev = extend_due(&mut c);
    assert!(matches!(&ev[..], [Event::NotExtended { .. }]));
    assert_eq!(c.concessions.contracts[0].extensions_left, 0);
    // red (26) at a month's end: ended by the authority, the company paying for it
    let mut c = company(GameMode::Realistic);
    c.date = "2024-03-31".into();
    with_contract(&mut c, Kind::Gross, Period::Day, 312_000_00, "2024-03-01");
    c.concessions.held.push(Concession { line: "Linie5".into(), number: "5".into(), from: "2024-03-01".into(), until: "2027-02-28".into(), price: 1.0, direct: false });
    c.concessions.contracts[0].record.push(PointsDay { date: "2024-03-20".into(), default: 30.0, ..Default::default() });
    let (r, _) = close(&mut c, &a_day());
    assert!(r.notes.iter().any(|n| matches!(n, Note::Terminated { .. })));
    assert_eq!(c.concessions.contracts[0].end, Some(End::Terminated));
    assert!(booked(&c, BookingKind::Penalty) < 0);
    assert_eq!(cn::of_line(&c, "Linie5").unwrap().until, "2024-03-31", "the concession ends tonight");
    // Normal never ends one
    let mut n = company(GameMode::Normal);
    n.date = "2024-03-31".into();
    with_contract(&mut n, Kind::Gross, Period::Day, 312_000_00, "2024-03-01");
    n.concessions.contracts[0].record.push(PointsDay { date: "2024-03-20".into(), default: 60.0, ..Default::default() });
    close(&mut n, &a_day());
    assert!(n.concessions.contracts[0].live());
}

#[test]
fn quality_points_offset_the_defaults_at_the_months_end() {
    let mut c = company(GameMode::Realistic);
    c.date = "2024-03-31".into();
    with_contract(&mut c, Kind::Gross, Period::Day, 312_000_00, "2024-03-01");
    let mut j = Journeys::default();
    for _ in 0..50 {
        j.ran("Linie5", "1", 0, false, None, None, None);
    }
    close(&mut c, &j);
    // a month without complaints and on time: 2 + 4 (and 5 more when inspected)
    let q: f64 = c.concessions.contracts[0].record.iter().map(|d| d.quality).sum();
    assert!(q == 6.0 || q == 11.0, "{q}");
}

#[test]
fn handing_a_contract_back_costs_a_months_payment() {
    let mut c = company(GameMode::Realistic);
    let today = c.date.clone();
    with_contract(&mut c, Kind::Gross, Period::Day, 120_000_00, &today);
    assert_eq!(hand_back_cost(&c, "Linie5"), Some(10_000_00));
    let cash = c.cash;
    assert_eq!(hand_back(&mut c, "Linie5"), Some(10_000_00));
    assert_eq!(cash - c.cash, 10_000_00);
    assert_eq!(c.concessions.contracts[0].end, Some(End::HandedBack));
    assert_eq!(hand_back(&mut c, "Linie5"), None, "once");
}

#[test]
fn an_own_school_line_runs_as_a_private_hire() {
    let mut c = company(GameMode::Realistic);
    c.lines.push(CompanyLine { name: "oo_1".into(), number: "S1".into(), own: true, tours: 2, plan: Some(crate::company::ownline::OwnPlan { service: crate::service::ServiceKind::School, ..Default::default() }), ..Default::default() });
    assert!(ensure_hires(&mut c));
    assert!(!ensure_hires(&mut c), "once");
    let k = of_line(&c, "oo_1").unwrap();
    assert_eq!((k.kind, k.period, k.days_a_year), (Kind::Hire, Period::Day, SCHOOL_DAYS));
    // two coaches at the data's £333-352 a school day each
    let day = Currency::Gbp.units(day_value(&c, k));
    assert!((666.0..704.0).contains(&day), "£{day}");
}

#[test]
fn an_older_file_gets_its_contracts() {
    let mut c = company(GameMode::Realistic);
    c.lines.push(CompanyLine { name: "Linie7".into(), number: "7".into(), tours: 2, km: 300.0, ..Default::default() });
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), tours: 1, km: 100.0, ..Default::default() });
    // a concession won at 90 % of the reference, and one taken on directly
    c.concessions.held.push(Concession { line: "Linie7".into(), number: "7".into(), from: "2024-01-01".into(), until: "2024-12-29".into(), price: 0.9, direct: false });
    c.concessions.held.push(Concession { line: "Linie5".into(), number: "5".into(), from: "2024-01-01".into(), until: "2024-12-29".into(), price: 1.0, direct: true });
    c.concessions.rivals.push(cn::Rival { id: 1, name: "Talbus".into(), character: auction::Character::Small, quality: 40.0 });
    c.version = 2;
    let mut json: serde_json::Value = serde_json::to_value(&c).unwrap();
    // (a file of before: no contracts kept)
    json["concessions"].as_object_mut().unwrap().remove("contracts");
    json["concessions"].as_object_mut().unwrap().remove("contract_counter");
    let old: Company = serde_json::from_value(json).unwrap();
    assert!(old.concessions.contracts.is_empty());
    let c = crate::company::store::migrate(old);
    assert_eq!(c.version, crate::company::model::VERSION);
    // the won one a gross-cost contract for the rest of its term at its winning price (90 % of
    // the cap the work's cost gives now); the direct one is commercial
    assert_eq!(c.concessions.contracts.len(), 1);
    let k = &c.concessions.contracts[0];
    assert_eq!((k.line.as_str(), k.kind, k.period, k.until.as_str()), ("Linie7", Kind::Gross, Period::Day, "2024-12-29"));
    let w = rough_work(2, 300.0);
    let cap = price_of(&c, &w, Kind::Gross, 1.0) * econ_uk::TENDER_CAP;
    assert!((k.year_price as f64 / (cap * 0.9) - 1.0).abs() < 0.01, "{} against {}", k.year_price, cap * 0.9);
    assert!(of_line(&c, "Linie5").is_none());
    // the rivals' quality on the new scale
    let (lo, hi) = auction::Character::Small.quality();
    assert!((lo..=hi).contains(&c.concessions.rivals[0].quality));
    // read again, the same
    let again: Company = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
    assert_eq!(again.concessions.contracts, c.concessions.contracts);
    let _ = Outcome::NoBid { winner: String::new(), amount: 0 };
}
