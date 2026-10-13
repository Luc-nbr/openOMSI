use super::logic::{self, Act, Freq, Pick};
use super::*;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::market::{self, MarketBus, Payment};
use omsi_launcher_lib::company::plan::Who;
use omsi_launcher_lib::company::{found, BookingKind, Founding, GameMode, Licence, LineTally};
use omsi_launcher_lib::{LineInfo, StopInfo, TourInfo, TripInfo};

/// A line of the map: `tours` tours of four trips an hour apart from 06:00 (as the core's tests
/// make one).
fn line(name: &str, number: &str, tours: usize) -> LineInfo {
    let trip = |k: usize, t: usize| TripInfo {
        name: format!("{name} {k}"),
        index: k + 1,
        line: number.into(),
        from: "A".into(),
        terminus: "B".into(),
        departure: 21_600.0 + k as f64 * 3600.0 + t as f64 * 600.0,
        arrival: 23_400.0 + k as f64 * 3600.0 + t as f64 * 600.0,
        stops: (0..5).map(|s| StopInfo { name: format!("S{s}"), id: s, arr: 0.0, dep: 0.0 }).collect(),
        km: 10.0,
    };
    LineInfo {
        name: name.into(),
        user_allowed: true,
        termini: vec!["A".into(), "B".into()],
        tours: (0..tours).map(|t| TourInfo { number: (t + 1).to_string(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs: true, next_run: None, trips: (0..4).map(|k| trip(k, t)).collect() }).collect(),
    }
}

fn company() -> Company {
    found(&Founding { name: "Leicht".into(), game_mode: GameMode::Casual, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc")
}

/// A Casual company with line 5 (three tours), `buses` buses and `drivers` drivers, its morning
/// run: (the company, the map's lines).
fn running(buses: usize, drivers: usize) -> (Company, Vec<LineInfo>) {
    let lines = vec![line("Linie5", "5", 3), line("Linie7", "7", 2)];
    let mut c = company();
    c.cash += 5_000_000_00;
    co::network::add_line(&mut c, &lines[0], None).unwrap();
    let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    for _ in 0..buses {
        market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    }
    let id = c.id.clone();
    let today = c.date.clone();
    while co::staff::staying(&c, co::Role::Driver, &today) < drivers {
        for a in co::staff::applicants(&c) {
            if co::staff::staying(&c, co::Role::Driver, &today) < drivers && a.licence == Licence::D {
                co::staff::hire(&mut c, &a).unwrap();
            }
        }
        c.id.push('x');
        c.taken = Default::default();
    }
    c.id = id;
    for e in c.staff.iter_mut() {
        e.sick_until = None;
        e.holiday_until = None;
    }
    co::autopilot::morning(&mut c, &today, &lines);
    (c, lines)
}

fn day_plan(c: &Company, lines: &[LineInfo]) -> co::plan::DayPlan {
    let tours = co::network::tours_of_day(c, lines, &c.date);
    co::plan::day_plan(c, &c.date, tours, &[], &[], false)
}

fn ids(tips: &[logic::Tip]) -> Vec<String> {
    tips.iter().map(|t| t.id.clone()).collect()
}

// --- the tips --------------------------------------------------------------------------------------

#[test]
fn a_new_company_is_told_to_take_a_line_and_then_to_buy_a_bus() {
    let mut c = company();
    let t = logic::tips(&c, None);
    assert_eq!(t[0].id, "first-line");
    assert_eq!(t[0].button.2, Act::AddLine);
    let lines = vec![line("Linie5", "5", 3)];
    co::network::add_line(&mut c, &lines[0], None).unwrap();
    let t = logic::tips(&c, None);
    assert_eq!(t[0].id, "first-bus");
    assert_eq!(t[0].button.2, Act::BuyBus);
    assert!(!ids(&t).contains(&"first-line".to_string()));
}

#[test]
fn the_tips_say_what_is_short_full_or_too_much() {
    // three tours, one bus and one driver: runs without a bus, shifts without a driver
    let (c, lines) = running(1, 1);
    let plan = day_plan(&c, &lines).to_plan();
    let t = logic::tips(&c, Some(&plan));
    let got = ids(&t);
    assert!(got.contains(&"short-buses".to_string()), "{got:?}");
    let short = t.iter().find(|x| x.id == "short-buses").unwrap();
    assert_eq!(short.button.2, Act::BuyBus);
    assert!(short.words.args.iter().any(|a| a.0 == "n" && a.1 != "0"));
    // a line often full in the morning: a bus more
    let mut full = c.clone();
    for k in 0..3 {
        full.history.push(co::DayRecord { date: co::dates::add(&c.date, -3 + k), lines: vec![LineTally { line: "Linie5".into(), passengers: 900, crowded: 4, crowded_morning: 3 }], ..Default::default() });
    }
    let t = logic::tips(&full, Some(&plan));
    let tip = t.iter().find(|x| x.id == "full-Linie5").expect("the full line's tip");
    assert_eq!(tip.words.key, "Line %{n} is often full in the morning. Add a bus.");
    assert_eq!(tip.button.2, Act::BuyBus);
    // a fare far above the advice: fewer people ride
    let mut dear = c.clone();
    let (_, hi) = co::fares::band(&dear, &dear.lines[0]);
    co::fares::set_fare(&mut dear, "Linie5", hi).unwrap();
    let t = logic::tips(&dear, Some(&plan));
    let fare = t.iter().find(|x| x.id == "fare-Linie5").expect("the fare's tip");
    assert_eq!(fare.words.key, "Your fare on line %{n} is high: fewer people ride.");
    assert_eq!(fare.button.2, Act::Fare("Linie5".into()));
    // the advised fare raises no tip, and lies in the line's band
    let mut fair = c.clone();
    let advised = logic::advised_fare(&fair, &fair.lines[0]);
    let (lo, hi) = co::fares::band(&fair, &fair.lines[0]);
    assert!((lo..=hi).contains(&advised));
    co::fares::set_fare(&mut fair, "Linie5", advised).unwrap();
    assert!(!ids(&logic::tips(&fair, Some(&plan))).contains(&"fare-Linie5".to_string()));
}

#[test]
fn more_buses_than_needed_and_a_full_depot_are_said() {
    // nine buses for three tours: six stand still
    let (c, lines) = running(9, 4);
    let plan = day_plan(&c, &lines).to_plan();
    assert!(logic::idle_buses(&c, &plan) >= 6);
    let t = logic::tips(&c, Some(&plan));
    let idle = t.iter().find(|x| x.id == "idle").expect("the idle buses' tip");
    assert_eq!(idle.button.2, Act::Open(BUSES));
    // the depot as full as it holds
    let mut packed = c.clone();
    let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    while logic::buses(&packed) < co::depot::places(&packed) {
        if market::buy_new(&mut packed, &bus, Payment::Cash, "").is_err() {
            break;
        }
    }
    if logic::buses(&packed) >= co::depot::places(&packed) {
        let t = logic::tips(&packed, Some(&plan));
        assert_eq!(t.iter().find(|x| x.id == "depot-full").map(|x| x.button.2.clone()), Some(Act::BiggerDepot));
    }
}

#[test]
fn an_important_message_is_a_tip_and_seen_once_its_button_is_pressed() {
    let mut c = company();
    let m = co::inbox::Message::new(co::inbox::Kind::Lines, "The roster was repaired", "x").level(co::clock::Level::Warn);
    co::inbox::post(&mut c, m);
    let id = c.inbox.messages.last().unwrap().id;
    let t = logic::tips(&c, None);
    let tip = t.iter().find(|x| x.id == format!("msg-{id}")).expect("the message's tip");
    assert_eq!(tip.button.2, Act::Seen(id));
    c.inbox.messages.last_mut().unwrap().read = true;
    assert!(!ids(&logic::tips(&c, None)).contains(&format!("msg-{id}")));
}

// --- the shift cards --------------------------------------------------------------------------------

#[test]
fn the_shift_cards_are_todays_with_the_open_ones_first() {
    // three tours, two buses and one driver: some shifts have no driver
    let (mut c, lines) = running(2, 1);
    let p = day_plan(&c, &lines);
    let cards = logic::shift_cards(&p, 0);
    assert!((1..=logic::SHIFT_CARDS).contains(&cards.len()), "{}", cards.len());
    let duties: usize = p.tours.iter().filter(|t| !t.tour.unplanned).map(|t| t.duties.len()).sum();
    assert_eq!(cards.len(), duties.min(logic::SHIFT_CARDS));
    // the open ones before those with a driver, each group by its time
    let first_taken = cards.iter().position(|s| !s.open()).unwrap_or(cards.len());
    assert!(cards[first_taken..].iter().all(|s| !s.open()), "{cards:?}");
    assert!(cards.iter().any(|s| s.open()), "{cards:?}");
    for w in cards.windows(2) {
        if w[0].open() == w[1].open() {
            assert!(w[0].from <= w[1].from);
        }
    }
    // a shift begun is not offered
    let late = logic::shift_cards(&p, 24 * 60);
    assert!(late.is_empty());
    // the player's own comes first, as soon as he picked it
    let pick = cards.iter().find(|s| !s.open()).unwrap_or(&cards[0]).clone();
    co::autopilot::pick_shift(&mut c, &p.date.clone(), &pick.line, &pick.tour_no, pick.duty);
    let p2 = day_plan(&c, &lines);
    let after = logic::shift_cards(&p2, 0);
    assert!(after[0].mine() && after[0].who == Some(Who::Player), "{after:?}");
    assert_eq!((after[0].line.as_str(), after[0].tour_no.as_str(), after[0].duty), (pick.line.as_str(), pick.tour_no.as_str(), pick.duty));
    assert!(shifts::mine_today(&after));
    // more than four shifts: four cards
    let (big, lines) = running(1, 1);
    let mut more = big.clone();
    co::network::add_line(&mut more, &lines[1], None).unwrap();
    co::autopilot::start_lines(&mut more, &big.date);
    let p3 = day_plan(&more, &lines);
    assert!(logic::shift_cards(&p3, 0).len() <= logic::SHIFT_CARDS);
}

// --- the line wizard ----------------------------------------------------------------------------------

#[test]
fn how_often_a_drawn_line_runs_comes_to_its_timetable() {
    let deps = |f: Freq| logic::freq_days(f).iter().map(|d| d.departures().len()).sum::<usize>();
    // a day group each, every one on, in bands of the day
    for f in Freq::ALL {
        let days = logic::freq_days(f);
        assert_eq!(days.len(), core::lines::DAY_GROUPS.len());
        for (k, d) in days.iter().enumerate() {
            assert!(d.on && d.runs() && !d.bands.is_empty(), "{f:?} {k}");
            assert_eq!(d.days, core::lines::DAY_GROUPS[k].1);
            assert!(d.bands.iter().all(|b| b.headway >= 5.0 && b.to > b.from));
        }
    }
    // the busier, the more departures, and the morning rush denser than the quiet hours
    assert!(deps(Freq::Quiet) < deps(Freq::Normal) && deps(Freq::Normal) < deps(Freq::Busy));
    let rush = |f: Freq| logic::freq_days(f)[0].bands.iter().find(|b| b.from <= 7.0 * 60.0 && 7.0 * 60.0 < b.to).map(|b| b.headway).unwrap();
    assert_eq!((rush(Freq::Quiet), rush(Freq::Normal), rush(Freq::Busy)), (30.0, 15.0, 10.0));
    // what a map line's headway is called
    assert_eq!((Freq::of_headway(10.0), Freq::of_headway(20.0), Freq::of_headway(60.0)), (Freq::Busy, Freq::Normal, Freq::Quiet));
}

#[test]
fn the_wizards_last_step_says_what_the_line_comes_to() {
    let (c, lines) = running(1, 1);
    // a line of the map's: its number, the map's timetable kept, what it needs
    let choices = logic::map_choices(&c, &lines);
    assert_eq!(choices.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), vec!["Linie7"], "the company's own line is not offered");
    let o = logic::outcome(&c, &Pick::Map("Linie7".into()), Freq::Busy, &lines);
    assert_eq!(o.number, "7");
    assert!(o.keeps_map_timetable);
    let h = o.headway.expect("its headway from the map");
    assert!(h > 0.0);
    assert_eq!(o.freq, Freq::of_headway(h), "the map's own, not the one chosen");
    assert!(o.buses.is_some_and(|b| b >= 1) && o.drivers.is_some_and(|d| d >= 1));
    // with one bus for line 5 already, line 7 needs buses the company lacks
    assert!(o.short >= 1, "{o:?}");
    // Casual takes a map line on without its auction: no asking price where it may add directly
    assert_eq!(o.price.is_some(), !co::concessions::may_add_directly(&c));
    // a line drawn: the frequency chosen, its needs known once drawn
    let d = logic::outcome(&c, &Pick::Draw, Freq::Quiet, &lines);
    assert_eq!((d.freq, d.keeps_map_timetable, d.buses, d.drivers), (Freq::Quiet, false, None, None));
}

// --- no way to Normal's pages ---------------------------------------------------------------------------

#[test]
fn casual_pages_never_lead_to_a_normal_page() {
    let (c, lines) = running(1, 1);
    let plan = day_plan(&c, &lines).to_plan();
    for (who, plan) in [(company(), None), (c.clone(), Some(&plan))] {
        for a in logic::all_acts(&who, plan) {
            if let Some(tab) = a.lands() {
                assert!(is_casual(tab), "{a:?} -> {tab}");
            }
        }
    }
    // the tiles open their own pages
    let tiles = logic::tiles(&c, Some(&plan), 0);
    assert_eq!(tiles.iter().map(|t| t.page).collect::<Vec<_>>(), vec![BUSES, LINES, DRIVERS, MONEY]);
    assert_eq!(tiles.iter().map(|t| t.action.2.clone()).collect::<Vec<_>>(), vec![Act::BuyBus, Act::AddLine, Act::HireNow, Act::Open(MONEY)]);
    // a way from elsewhere to a page of Normal's lands on Casual's about the same, or the home
    for tab in 0..super::super::TABS.len() {
        assert!(is_casual(page_for(tab)), "{tab}");
    }
    assert_eq!((page_for(1), page_for(2), page_for(3), page_for(4), page_for(6), page_for(super::super::DEPOT_TAB)), (BUSES, DRIVERS, LINES, MONEY, SHIFTS, COMPANY));
    for tab in [0, 5, super::super::CONCESSIONS_TAB, super::super::RESEARCH_TAB, super::super::WORKSHOP_TAB, super::super::CONTRACTS_TAB, super::super::INBOX_TAB] {
        assert_eq!(page_for(tab), HOME_TAB, "{tab}");
    }
    // every step lies under a page of its own, and the pages under the home
    for tab in ALL {
        assert!(is_casual(parent(tab)));
    }
    assert_eq!((parent(SHOP), parent(NEW_LINE), parent(BUSES)), (BUSES, LINES, HOME_TAB));
}

// --- the money, the drivers, the hint -----------------------------------------------------------------

#[test]
fn the_money_by_the_week_and_its_biggest_costs() {
    let mut c = company();
    assert!(logic::weeks(&c, 8).is_empty());
    for k in 0..20 {
        c.history.push(co::DayRecord { date: co::dates::add(&c.date, -20 + k), income: 1_000_00, expenses: 600_00, result: 400_00, ..Default::default() });
    }
    let w = logic::weeks(&c, 8);
    // twenty days: two whole weeks and the six days before them, the newest last
    assert_eq!(w.len(), 3);
    assert_eq!((w[2].income, w[2].expenses), (7_000_00, 4_200_00));
    assert_eq!(w[0].income, 6_000_00);
    assert_eq!(logic::week_result(&c), Some(2_800_00));
    // the costs: the biggest first, capital left out
    c.book(BookingKind::Wages, -5_000_00, "pay", false);
    c.book(BookingKind::Rent, -2_000_00, "rental", false);
    c.book(BookingKind::Purchase, -200_000_00, "bus", false);
    let costs = logic::biggest_costs(&c);
    assert_eq!(costs.iter().map(|x| x.0).collect::<Vec<_>>(), vec![BookingKind::Wages, BookingKind::Rent]);
    // renting for the day costs a lot: a bus of one's own
    assert_eq!(logic::cost_tip(&costs).map(|x| x.1), Some(Act::BuyBus));
    assert_eq!(logic::cost_tip(&[(BookingKind::Wages, 10), (BookingKind::Energy, 10)]), None);
    assert!(logic::cost_tip(&[(BookingKind::Wages, 90), (BookingKind::Energy, 10)]).is_some_and(|x| x.1 == Act::AddLine));
}

#[test]
fn a_driver_has_one_to_five_stars_and_the_hint_comes_after_three_weeks() {
    let (mut c, _) = running(1, 2);
    for e in c.staff.iter_mut() {
        e.skills = co::Skills { driving: 100.0, punctuality: 100.0, service: 100.0 };
        e.experience = 100.0;
    }
    assert!(c.staff.iter().all(|e| logic::stars(e) == 5));
    for e in c.staff.iter_mut() {
        e.skills = co::Skills { driving: 0.0, punctuality: 0.0, service: 0.0 };
        e.experience = 0.0;
    }
    assert!(c.staff.iter().all(|e| logic::stars(e) == 1));
    // the hint to Normal: after three weeks of days, until it is put away
    assert!(!logic::hint_due(&c));
    for k in 0..21 {
        c.history.push(co::DayRecord { date: co::dates::add(&c.date, k), ..Default::default() });
    }
    assert!(logic::hint_due(&c));
    c.autopilot.normal_hint_off = true;
    assert!(!logic::hint_due(&c));
    c.autopilot.normal_hint_off = false;
    co::autopilot::set_game_mode(&mut c, GameMode::Normal);
    assert!(!logic::hint_due(&c), "only in Casual");
}

#[test]
fn the_day_is_said_in_one_sentence() {
    let c = company();
    assert_eq!(logic::day_sentence(&c, None, 0).key, "Your company runs no line yet. Add one to get going.");
    let (c, lines) = running(2, 4);
    let plan = day_plan(&c, &lines).to_plan();
    assert_eq!(logic::day_sentence(&c, Some(&plan), 0).key, "The buses are still in the depot: the first leaves at %{time}.");
    // at 07:00 both buses drive (three tours from 06:00, two of them with a bus)
    let s = logic::day_sentence(&c, Some(&plan), 7 * 60);
    assert!(s.key == "All %{n} buses are on the road." || s.key == "%{k} of %{n} buses are on the road.", "{s:?}");
    assert_eq!(logic::day_sentence(&c, Some(&plan), 23 * 60).key, "The day's work is done: every bus is back in the depot.");
}

// --- every text in every language -------------------------------------------------------------------

/// The string literals of the Casual pages' source that are shown: those with a capital or a
/// space (not an id, an icon's name or a place's name), outside comments and `format!`'s braces.
fn shown_literals() -> Vec<String> {
    let files = [include_str!("mod.rs"), include_str!("home.rs"), include_str!("buses.rs"), include_str!("lines.rs"), include_str!("drivers.rs"), include_str!("money.rs"), include_str!("shifts.rs"), include_str!("logic.rs")];
    let mut out = Vec::new();
    for src in files {
        for line in src.lines() {
            let b: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < b.len() {
                if b[i] == '/' && b.get(i + 1) == Some(&'/') {
                    break;
                }
                if b[i] == '\'' && b.get(i + 2) == Some(&'\'') {
                    i += 3;
                    continue;
                }
                if b[i] == '"' {
                    let mut s = String::new();
                    i += 1;
                    while i < b.len() && b[i] != '"' {
                        if b[i] == '\\' && i + 1 < b.len() {
                            s.push(match b[i + 1] {
                                'n' => '\n',
                                c => c,
                            });
                            i += 2;
                            continue;
                        }
                        s.push(b[i]);
                        i += 1;
                    }
                    i += 1;
                    let rust_format = s.replace("%{", "").contains('{');
                    let shown = s.chars().any(|c| c.is_ascii_uppercase()) || (s.contains(' ') && s.chars().any(char::is_alphabetic));
                    if shown && !rust_format && !s.starts_with("company-") && !out.contains(&s) {
                        out.push(s);
                    }
                    continue;
                }
                i += 1;
            }
        }
    }
    out
}

#[test]
fn every_text_of_casual_is_translated() {
    let mut texts = shown_literals();
    assert!(texts.len() > 150, "{}", texts.len());
    texts.extend(logic::deed_texts().into_iter().map(str::to_string));
    for f in Freq::ALL {
        texts.extend([f.label(), f.words()].map(str::to_string));
    }
    for k in BookingKind::ALL {
        texts.push(logic::cost_words(k).to_string());
    }
    // (the core's words the pages show; the feed's texts they compare with are the clock's)
    for t in ["Bus %{bus} broke down on line %{n}", "Bus %{bus} broke down in the game", "Spare bus %{spare} takes over the trips of bus %{bus} on line %{n} from %{time}", "They are leaving already.", "They do not work here.", "The cash does not cover it."] {
        texts.push(t.to_string());
    }
    let as_is = ["%{text}", "All", "Casual", "Normal"];
    let places = |s: &str| {
        let mut v: Vec<String> = s.match_indices("%{").filter_map(|(i, _)| s[i..].find('}').map(|j| s[i..i + j + 1].to_string())).collect();
        v.sort();
        v
    };
    let mut missing = Vec::new();
    for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
        for t in &texts {
            if as_is.contains(&t.as_str()) && crate::_rust_i18n_try_translate(lang, t).is_none() {
                continue;
            }
            match crate::_rust_i18n_try_translate(lang, t) {
                None => missing.push(format!("{lang}: {t:?}")),
                Some(x) if places(&x) != places(t) => missing.push(format!("{lang}: {t:?} -> {x:?} (places)")),
                Some(_) => {}
            }
        }
    }
    assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
}

#[test]
fn the_game_modes_texts_are_translated() {
    // (what `locale_check` cannot see, kept from the old easy home's check: the core's texts and
    // those of the settings handed on in a constant)
    let mut texts: Vec<&str> = co::autopilot::texts();
    for m in GameMode::ALL {
        texts.extend([m.label(), m.info(), m.pitch()]);
        texts.extend(m.bullets());
    }
    texts.extend([
        "The company runs in Casual now: it sees to its routine by itself.",
        "The company runs in Normal now: every page is open, and the dispatcher helps.",
        "The company runs in Realistic now: real-life rules, and you solve everything yourself.",
        "Switch to %{mode}?",
        "Switch mode",
        super::super::settings::ROSTERFLOW,
        super::super::settings::ROSTERFLOW_NOTE,
        super::super::settings::TO_CASUAL,
        super::super::settings::TO_CASUAL_RF,
        super::super::settings::FROM_CASUAL,
        super::super::settings::TO_NORMAL,
        super::super::settings::TO_REALISTIC,
        super::super::settings::KEPT,
        super::super::NOT_RESEARCHED_BY_ITSELF,
        co::plan::REPAIRED.0,
        co::plan::REPAIRED.1,
        co::plan::REPAIRED_OPEN.0,
        co::plan::REPAIRED_OPEN.1,
    ]);
    for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
        for t in &texts {
            assert!(crate::_rust_i18n_try_translate(lang, t).is_some(), "{lang}: no translation of {t:?}");
        }
    }
}
