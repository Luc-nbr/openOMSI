//! The Staff page's Agencies tab (Luc, 2026-10-15: agency drivers hired a day ahead, for a day of
//! the player's choice, as many as he wants - `co::agency`): a calendar of the coming weeks with
//! what is booked, the day picked booked - how many, for a whole day, some hours or some of its
//! open duties, and what it costs - and the bookings with Cancel.

use super::super::words::{day_label, eur, eur_cents, short_date, tr, trf, MONTHS, WEEKDAYS};
use super::super::{act as apply, plan_of, week_work, Session};
use omsi_launcher_lib::company::agency::{self as ag, Summary};
use omsi_launcher_lib::company::plan::{self as pl, DayPlan};
use omsi_launcher_lib::company::staff::Block;
use omsi_launcher_lib::company::studio::{self, PieceRef};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

/// The weeks the calendar shows, from this week's Monday (the agency books six weeks ahead).
const WEEKS: i64 = 7;
/// The ways to book a day: a whole day, some hours, some of its open duties.
const MODES: [&str; 3] = ["A whole day", "From – to", "For duties"];

fn hhmm(m: i32) -> String {
    co::clock::hhmm(m as i64)
}

fn hours_text(m: i32) -> String {
    format!("{} h {:02}", m / 60, m % 60)
}

/// The day picked (`aday`), else tomorrow.
fn picked(c: &Company, ui: &Value) -> String {
    ui.get("aday").and_then(Value::as_str).filter(|d| co::dates::parse(d).is_some()).map(str::to_string).unwrap_or_else(|| co::dates::add(&c.date, 1))
}

/// The plan of the day picked, when it is one of the week the company reads (its open duties to
/// book for).
pub fn plan_for(s: &mut Session, ui: &Value) -> Option<DayPlan> {
    let c = s.company.as_ref()?;
    let date = picked(c, ui);
    let ahead = co::dates::between(&c.date, &date);
    if !(1..7).contains(&ahead) {
        return None;
    }
    week_work(s);
    plan_of(s, &date)?.ok()
}

/// The open duties of a day plan: (its key, the piece, its time) - those of tours in service no
/// driver has.
fn open_duties(p: &DayPlan) -> Vec<(String, PieceRef, Block, String)> {
    let blocks: Vec<co::network::TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
    let mut out = Vec::new();
    for (i, t) in p.tours.iter().enumerate().filter(|(_, t)| !t.by_player && !t.tour.unplanned) {
        for (k, d) in t.duties.iter().enumerate().filter(|(_, d)| d.who.is_none()) {
            let Some(piece) = studio::piece_ref(&blocks, i, k) else { continue };
            let key = pl::duty_key(&t.tour.line, &t.tour.tour, k);
            let block = Block { key: key.clone(), from: d.from, to: d.to, from_stop: t.tour.trips[d.start].from.clone(), to_stop: t.tour.trips[d.end - 1].to.clone() };
            let label = format!("{} {}  ·  {} {}  ·  {} {}", tr("Line"), t.tour.number, tr("Tour"), t.tour.tour, tr("Duty"), k + 1);
            out.push((key, piece, block, label));
        }
    }
    out.sort_by_key(|x| x.2.from);
    out
}

/// What the form asks for (`ui`): the day, how many, the hours (mode 1), the duties (mode 2).
struct Ask {
    date: String,
    count: u32,
    mode: usize,
    hours: Option<(i32, i32)>,
    keys: Vec<String>,
}

fn ask_of(c: &Company, ui: &Value) -> Ask {
    let mode = ui.get("amode").and_then(Value::as_u64).unwrap_or(0).min(2) as usize;
    let from = ui.get("afrom").and_then(Value::as_i64).unwrap_or(6 * 60) as i32;
    let to = ui.get("ato").and_then(Value::as_i64).unwrap_or(14 * 60) as i32;
    // (an end at or before the start is past midnight, as the day's late duties run: the time
    // fields have no day, and 18:00 to 01:00 was refused as not lasting an hour)
    let to = to.rem_euclid(1440);
    let to = if to <= from { to + 1440 } else { to };
    let keys: Vec<String> = ui.get("aduties").and_then(Value::as_array).map(|v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    Ask { date: picked(c, ui), count: ui.get("acount").and_then(Value::as_u64).unwrap_or(1).clamp(1, ag::MOST as u64) as u32, mode, hours: (mode == 1).then_some((from, to)), keys }
}

/// The duties asked for, from the day's plan.
fn duties_asked(a: &Ask, plan: Option<&DayPlan>) -> Vec<(PieceRef, Block)> {
    if a.mode != 2 {
        return Vec::new();
    }
    plan.map(|p| open_duties(p).into_iter().filter(|x| a.keys.contains(&x.0)).map(|x| (x.1, x.2)).collect()).unwrap_or_default()
}

/// A booking in a line: how many, and what for.
fn what_text(s: &Summary) -> String {
    let n = if s.count == 1 { tr("1 driver") } else { trf("%{n} drivers", &[("n", &s.count.to_string())]) };
    let what = match s.hours {
        Some((a, z)) => format!("{}–{}", hhmm(a), hhmm(z)),
        None if s.duties > 0 => trf("for %{n} duties", &[("n", &s.duties.to_string())]),
        None => trf("a whole day (%{h})", &[("h", &hours_text(ag::DAY_MINUTES))]),
    };
    format!("{n}  ·  {what}")
}

/// The Agencies tab: the calendar (`aday` picked), the form for the day picked, and the
/// bookings - those to come first, then those of the days just gone.
pub fn part(c: &Company, plan: Option<&DayPlan>, ui: &Value) -> Value {
    let day0 = co::dates::parse(&c.date).unwrap_or(0);
    let monday = day0 - co::dates::weekday(day0) as i64;
    let last = monday + WEEKS * 7 - 1;
    let a = ask_of(c, ui);
    let all = ag::bookings(c);
    let days: Vec<Value> = (monday..=last)
        .map(|d| {
            let date = co::dates::fmt(d);
            let (_, m, day) = co::dates::civil_from_days(d);
            let n: u32 = all.iter().filter(|s| s.date == date).map(|s| s.count).sum();
            let open = ag::bookable(c, &date).is_ok();
            json!({
                "date": date, "day": day, "month": (day == 1 || d == monday).then(|| tr(MONTHS[(m as usize).clamp(1, 12) - 1])),
                "gone": !open && n == 0, "today": d == day0, "on": date == a.date, "count": if n > 0 { n.to_string() } else { String::new() },
                "tone": if n > 0 { "accent2" } else { "faint" }, "frac": (n as f64 / 10.0).min(1.0),
                "tip": if n > 0 { trf("%{n} agency drivers booked", &[("n", &n.to_string())]) } else { String::new() },
            })
        })
        .collect();
    // the day picked: what to book, and what it costs (booked on a copy: the same rules)
    let why = ag::bookable(c, &a.date).err();
    let duties: Option<Vec<Value>> = plan.map(|p| open_duties(p).into_iter().map(|(key, _, b, label)| json!({ "key": key, "label": label, "time": format!("{} – {}", hhmm(b.from), hhmm(b.to)), "on": a.keys.contains(&key) })).collect());
    let asked = duties_asked(&a, plan);
    let mut probe = c.clone();
    let (quote, quote_tone) = match why {
        Some(_) => (String::new(), "dim"),
        None if a.mode == 2 && asked.is_empty() => (tr("Pick the duties they come for."), "dim"),
        None => match ag::book(&mut probe, &a.date, a.count, a.hours, &asked) {
            Ok(m) => {
                let mut t = trf("%{n} × %{rate} an hour: %{amount}", &[("n", &a.count.to_string()), ("rate", &eur_cents(ag::rate(c) as f64)), ("amount", &eur(m.cost))]);
                if m.left > 0 {
                    t = format!("{t}  ·  {}", trf("%{n} of the duties do not fit them: book more drivers", &[("n", &m.left.to_string())]));
                }
                (t, if m.left > 0 { "warn" } else { "ok" })
            }
            Err(e) => (tr(e), "danger"),
        },
    };
    let day = json!({
        "title": day_label(&a.date), "date": a.date, "why": why.map(tr), "count": a.count, "most": ag::MOST,
        "count_title": tr("How many drivers"), "modes": MODES.iter().map(|m| tr(m)).collect::<Vec<_>>(), "mode": a.mode,
        "from": a.hours.map(|h| h.0).unwrap_or(6 * 60), "to": a.hours.map(|h| h.1).unwrap_or(14 * 60),
        "mode_note": match a.mode {
            0 => trf("Each is paid for %{h}, and for more when he drives more.", &[("h", &hours_text(ag::DAY_MINUTES))]),
            1 => trf("Each is paid for these hours (%{h} at least) and drives only within them.", &[("h", &hours_text(ag::MIN_MINUTES))]),
            _ => trf("The duties are given to them by the working-time rules; each is paid for his duties' hours (%{h} at least).", &[("h", &hours_text(ag::MIN_MINUTES))]),
        },
        "duties": duties, "no_duties": tr("No duty of this day is open."),
        "far": tr("The duties of days more than a week ahead are not read yet: book them for a whole day or some hours."),
        "quote": quote, "quote_tone": quote_tone, "book": tr("Book"), "fewer": tr("Fewer"), "more": tr("More"),
    });
    // the bookings
    let row = |s: &Summary| {
        let b = c.planning.agency.iter().find(|b| b.no == s.no);
        let fee = b.map(|b| ag::cancel_fee(c, b)).unwrap_or(0);
        let gone = s.settled || co::dates::between(&c.date, &s.date) < 0;
        let names = match s.drivers.as_slice() {
            [] => String::new(),
            [one] => c.employee(*one).map(|e| trf("Agency driver %{n}", &[("n", &ag::number_of(e).to_string())])).unwrap_or_default(),
            [first, .., last] => {
                let n = |id: &u32| c.employee(*id).map(ag::number_of).unwrap_or(0).to_string();
                trf("Agency drivers %{a} – %{b}", &[("a", &n(first)), ("b", &n(last))])
            }
        };
        let given = s.duties;
        json!({
            "no": s.no, "date": day_label(&s.date), "short": short_date(&s.date), "what": what_text(s), "names": names,
            "given": (given > 0).then(|| if given == 1 { tr("1 duty given") } else { trf("%{n} duties given", &[("n", &given.to_string())]) }),
            "cost": if s.settled { trf("Paid at the close: %{amount} booked", &[("amount", &eur(s.cost))]) } else { eur(s.cost) },
            "cancel": (!gone).then(|| tr("Cancel")), "fee": (!gone && fee > 0).then(|| trf("Cancelled today the agency bills %{amount}", &[("amount", &eur(fee))])),
            "today": s.date == c.date, "gone": gone,
        })
    };
    let coming: Vec<Value> = all.iter().filter(|s| !s.settled && co::dates::between(&c.date, &s.date) >= 0).map(row).collect();
    let lately: Vec<Value> = all.iter().filter(|s| s.settled || co::dates::between(&c.date, &s.date) < 0).rev().map(row).collect();
    json!({
        "kind": "agencies",
        "title": tr("Book agency drivers"),
        "calendar_title": tr("The coming weeks").to_uppercase(), "weekdays": WEEKDAYS.iter().map(|w| tr(w)).collect::<Vec<_>>(), "days": days, "day": day,
        "notes": [
            trf("Booked ahead an agency driver costs %{rate} an hour; one sent on the day itself %{same}.", &[("rate", &eur_cents(ag::rate(c) as f64)), ("same", &eur_cents(pl::agency_hour(c)))]),
            tr("On their day they are among the drivers of the Roster tab: give them duties there, or let the dispatcher and Fill the roster take them first for what is open."),
            tr("A booked driver is paid for what was booked even when he has no duty, as agencies bill. Cancelling is free until the day before; on the day itself the agency bills half."),
            tr("There is no limit to how many you book: only the cash."),
        ],
        "coming_title": trf("Booked: %{n}", &[("n", &coming.len().to_string())]).to_uppercase(), "coming": coming,
        "none": tr("No agency drivers are booked. Book them here for a day ahead, or send one on the day itself from the Roster tab."),
        "lately_title": tr("Days just gone").to_uppercase(), "lately": lately,
    })
}

/// What the Agencies tab's buttons do: book the day picked as the form asks (`agency_book`, the
/// form's state as its arguments), cancel a booking (`agency_cancel`).
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let c = s.company.clone()?;
    Some(Ok(match what {
        "agency_book" => {
            let a = ask_of(&c, args);
            let plan = plan_for(s, args);
            let asked = duties_asked(&a, plan.as_ref());
            if a.mode == 2 && asked.is_empty() {
                s.say(tr("Pick the duties they come for."), true);
                return Some(Ok(json!({})));
            }
            match apply(s, |c| ag::book(c, &a.date, a.count, a.hours, &asked)) {
                Ok(m) => {
                    let mut t = trf("Booked: %{n} agency drivers for %{date} (%{amount}).", &[("n", &a.count.to_string()), ("date", &day_label(&a.date)), ("amount", &eur(m.cost))]);
                    if m.left > 0 {
                        t = format!("{t} {}", trf("%{n} of the duties do not fit them: book more drivers", &[("n", &m.left.to_string())]));
                    }
                    s.say(t, false);
                    json!({ "booked": m.no })
                }
                Err(p) => json!({ "popup": p }),
            }
        }
        "agency_cancel" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            match apply(s, |c| ag::cancel(c, no)) {
                Ok(0) => {
                    s.say(tr("Booking cancelled: it costs nothing."), false);
                    json!({})
                }
                Ok(fee) => {
                    s.say(trf("Booking cancelled on its day: the agency bills %{amount}.", &[("amount", &eur(fee))]), false);
                    json!({})
                }
                Err(p) => json!({ "popup": p }),
            }
        }
        _ => return None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_that_end_past_midnight_are_booked_as_one_stretch() {
        use omsi_launcher_lib::company::{found, Founding};
        let c = found(&Founding { name: "Agency".into(), date: "2026-10-15".into(), ..Default::default() }, "Luc");
        let hours = |from: i32, to: i32| ask_of(&c, &json!({ "amode": 1, "afrom": from, "ato": to })).hours;
        assert_eq!(hours(6 * 60, 14 * 60), Some((360, 840)));
        // 18:00 to 01:00: seven hours, not a refusal
        assert_eq!(hours(18 * 60, 60), Some((1080, 1500)));
        // (as the page sends it back, and with its start changed afterwards)
        assert_eq!(hours(18 * 60, 1500), Some((1080, 1500)));
        assert_eq!(hours(20 * 60, 1500), Some((1200, 1500)));
        assert_eq!(hours(0, 0), Some((0, 1440)));
    }

    /// Every text of the Agencies tab - its own and the core's (`agency::texts`) - in the six
    /// languages.
    #[test]
    fn every_agency_text_is_translated() {
        let src = include_str!("agencies.rs");
        let mut texts: Vec<String> = Vec::new();
        for lead in ["tr(\"", "trf(\""] {
            let mut rest = src;
            while let Some(k) = rest.find(lead) {
                let after = &rest[k + lead.len()..];
                rest = after;
                texts.push(after.chars().take_while(|c| *c != '"').collect());
            }
        }
        assert!(texts.len() > 25, "only {} texts found", texts.len());
        texts.extend(ag::texts().into_iter().chain(MODES).map(String::from));
        texts.extend(["Agencies", "Agency drivers"].map(String::from));
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing (crates/omsihub-locales/locales/company-agencies.yml):\n{}", missing.len(), missing.join("\n"));
    }
}
