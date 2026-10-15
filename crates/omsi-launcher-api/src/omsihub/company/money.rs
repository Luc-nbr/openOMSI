//! The company's finances (`crates/omsi-launcher-ui/src/company/money.rs`, `bank.rs`, `fares.rs`):
//! cash, debt and what the fleet is worth, a month's bookings by kind, the bank's credit room and
//! the loans (a new one and paying back: the dealer module's loan sheets), the lines' fares and a
//! line's fare dialog, and the ledger.

use super::dealer::{percent, room_tone};
use super::words::{day_label, eur, eur_cents, grouped, month_label, num, signed, tr, trf};
use super::{act as apply, Session};
use omsi_launcher_lib::company::fares as fr;
use omsi_launcher_lib::company::finance::{self as fi, Room};
use omsi_launcher_lib::company::{self as co, BookingKind, Company, CompanyLine};
use serde_json::{json, Value};

fn month_part(c: &Company, chosen: Option<&str>) -> Value {
    let mut months: Vec<String> = c.months.iter().map(|m| m.month.clone()).collect();
    let now = co::dates::month_of(&c.date);
    if !months.contains(&now) {
        months.push(now);
    }
    months.sort();
    months.reverse();
    let k = chosen.and_then(|m| months.iter().position(|x| x == m)).unwrap_or(0);
    let m = c.month(&months[k]);
    let mut rows: Vec<(BookingKind, i64)> = BookingKind::ALL.iter().filter(|x| !x.is_capital()).map(|x| (*x, m.get(*x))).filter(|x| x.1 != 0).collect();
    rows.sort_by_key(|x| std::cmp::Reverse(x.1));
    let capital: Vec<(BookingKind, i64)> = BookingKind::ALL.iter().filter(|x| x.is_capital()).map(|x| (*x, m.get(*x))).filter(|x| x.1 != 0).collect();
    let (res, res_tone) = signed(m.result());
    json!({
        "title": tr("The month").to_uppercase(), "months": months.iter().map(|m| json!([m, month_label(m)])).collect::<Vec<_>>(), "month": months[k],
        "empty": (rows.is_empty() && capital.is_empty()).then(|| tr("Nothing booked this month yet.")),
        "rows": rows.iter().map(|(kind, a)| { let (t, tone) = signed(*a); json!([tr(kind.label()), t, tone]) }).collect::<Vec<_>>(),
        "result_label": tr("Result"), "result": res, "result_tone": res_tone,
        // (the deductions for late running of the days kept: `late::finance_view`)
        "late": (!c.lateness.days.is_empty()).then(|| tr("Deductions for late running")),
        "capital": (!capital.is_empty()).then(|| format!("{}: {}", tr("Investment and financing"), capital.iter().map(|(k, a)| format!("{} {}", tr(k.label()), signed(*a).0)).collect::<Vec<_>>().join("  ·  "))),
    })
}

/// The credit room (`bank::credit_figure`).
fn credit(c: &Company) -> Value {
    let cr = fi::credit(c, 0);
    let room = fi::room(&cr);
    let used = if cr.limit > 0 { (cr.debt as f64 / cr.limit as f64).clamp(0.0, 1.0) } else { 1.0 };
    let rate = if cr.discount > 0.0 { trf("Interest %{rate} a year (%{discount} less for the company's level)", &[("rate", &percent(cr.rate)), ("discount", &percent(cr.discount))]) } else { trf("Interest %{rate} a year", &[("rate", &percent(cr.rate))]) };
    json!({
        "caps": tr("Credit room").to_uppercase(), "tone": room_tone(room),
        "value": if room == Room::None { trf("%{amount} left", &[("amount", &eur(cr.left))]) } else { eur(cr.left) },
        "say": tr(match room { Room::Plenty => "the bank lends this much more", Room::Little => "little left: the bank is careful", Room::None => "used up: the bank lends nothing more" }),
        "bar": { "frac": 1.0 - used, "label": trf("%{debt} of %{limit} used", &[("debt", &eur(cr.debt)), ("limit", &eur(cr.limit))]), "right": format!("{:.0} % {}", (1.0 - used) * 100.0, tr("free")), "tip": tr("What the bank still lends, of all it would lend the company: its base, a share of the fleet's value and six months of the result, less what is owed.") },
        "lines": [
            [trf("Base for a %{difficulty} company", &[("difficulty", &tr(c.game_mode.label()).to_lowercase())]), eur(cr.base)],
            [trf("%{share} of the fleet's %{value}", &[("share", &percent(cr.share)), ("value", &eur(cr.fleet_value))]), eur((cr.share * cr.fleet_value as f64).round() as i64)],
            [trf("6 months of the result (%{amount} a month)", &[("amount", &eur(cr.income))]), eur(fi::INCOME_MONTHS * cr.income)],
            [tr("Owed already"), format!("- {}", eur(cr.debt))],
        ],
        "rate": rate,
    })
}

fn loans(c: &Company) -> Value {
    let papers: Vec<u32> = c.dealer.loan_contracts.iter().map(|k| k.no).collect();
    let list: Vec<Value> = c
        .loans
        .iter()
        .map(|loan| {
            json!({
                "id": loan.id, "what": if loan.purpose.is_empty() { tr("Loan") } else { loan.purpose.clone() },
                "sub": trf("%{left} of %{principal} left  ·  %{rate} %  ·  %{monthly} a month, %{n} months", &[("left", &eur(loan.remaining)), ("principal", &eur(loan.principal)), ("rate", &num(loan.rate * 100.0, 1)), ("monthly", &eur(loan.monthly)), ("n", &loan.months_left.to_string())]),
                "paper": papers.contains(&loan.id),
            })
        })
        .collect();
    json!({ "title": tr("Loans").to_uppercase(), "credit": credit(c), "list": list, "none": tr("The company owes nothing."), "repay": tr("Pay back…"), "contract": tr("Contract"), "take": tr("Take a loan…") })
}

fn ledger(c: &Company) -> Value {
    let rows: Vec<Value> = c
        .ledger
        .iter()
        .rev()
        .take(400)
        .map(|b| {
            let (t, tone) = signed(b.amount);
            let mark = (!b.kind.is_capital()).then(|| if b.measured { json!([tr("driven"), "accent2", tr("Measured: from a trip really driven")]) } else { json!([tr("modelled"), "dim", tr("Modelled: worked out by the company's day")]) });
            json!({ "date": day_label(&b.date), "kind": tr(b.kind.label()), "text": b.text, "mark": mark, "amount": t, "tone": tone })
        })
        .collect();
    json!({ "title": tr("Ledger").to_uppercase(), "rows": rows })
}

fn fares(c: &Company) -> Value {
    if c.lines.is_empty() {
        return json!({ "title": tr("Ticket prices").to_uppercase(), "empty": tr("The company runs no line yet: each line's single ticket is set here and on the Lines page.") });
    }
    let p0 = fr::association_fare(c);
    let rows: Vec<Value> = c
        .lines
        .iter()
        .map(|cl| {
            let fare = fr::fare_of(c, cl);
            let rel = fare as f64 / p0.max(1) as f64 - 1.0;
            let share = cl.demand.share();
            let (t, tone) = if (share - 1.0).abs() < 0.005 { (tr("passengers as usual"), "dim") } else { (trf("%{p} passengers", &[("p", &format!("{:+.0} %", (share - 1.0) * 100.0))]), if share > 1.0 { "ok" } else { "warn" }) };
            json!({ "name": cl.name, "plate": super::lines::plate(&cl.number, &cl.colour), "fare": eur_cents(fare as f64), "vs": if rel.abs() < 0.005 { tr("the association's") } else { format!("{:+.0} %", rel * 100.0) }, "pax": t, "tone": tone })
        })
        .collect();
    json!({ "title": tr("Ticket prices").to_uppercase(), "rows": rows, "tip": tr("Change the fare: see what it does before it is set") })
}

/// The Finances page (`money::draw`): the figures, the month, the loans, the ledger, the fares
/// (the easy mode's: the money as it stands, the months and the loans).
pub fn view(s: &mut Session, ui: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let this = c.month(&co::dates::month_of(&c.date));
    let prev_month = co::dates::month_of(&co::dates::add(&format!("{}-01", this.month), -1));
    let prev = c.month(&prev_month);
    let fleet_value = fi::fleet_value(&c);
    let tone = |r: i64| if r >= 0 { "ok" } else { "danger" };
    let easy = co::autopilot::on(&c);
    let mut figures = vec![
        json!({ "label": tr("Cash"), "value": super::sandbox::cash(&c), "under": "", "tone": super::sandbox::cash_tone(&c) }),
        json!({ "label": tr("Debt"), "value": eur(c.debt()), "under": trf("%{n} loans", &[("n", &c.loans.len().to_string())]), "tone": "text" }),
        json!({ "label": tr("Fleet value"), "value": eur(fleet_value), "under": tr("the buses owned"), "tone": "text", "open": "fleet_value", "tip": tr("By age band: what the buses cost new, are worth and lost") }),
        json!({ "label": tr("This month"), "value": signed(this.result()).0, "under": month_label(&this.month), "tone": tone(this.result()) }),
        json!({ "label": tr("Last month"), "value": signed(prev.result()).0, "under": month_label(&prev.month), "tone": tone(prev.result()) }),
    ];
    if easy {
        figures.remove(2);
    }
    let month = ui.get("month").and_then(Value::as_str).map(str::to_string);
    json!({
        "easy": easy, "figures": figures, "month": month_part(&c, month.as_deref()), "loans": loans(&c),
        "ledger": (!easy).then(|| ledger(&c)), "fares": (!easy).then(|| fares(&c)),
    })
}

/// A line's passengers a day at the association's fare, from today's tours (`fares::base_passengers`).
fn base_passengers(s: &Session, c: &Company, line: &CompanyLine) -> Option<f64> {
    let plan = s.plan.as_ref()?;
    let r = co::economy::rules(c.game_mode);
    let mood = co::reputation::demand_factor(c.reputation);
    let mut sum = 0.0;
    let mut any = false;
    for t in plan.tours.iter().filter(|t| t.tour.line.eq_ignore_ascii_case(&line.name)) {
        for trip in t.tour.trips.iter().filter(|x| x.counts()) {
            any = true;
            let own = line.plan.as_ref().and_then(|_| co::ownline::trip_boardings(line, &c.date, trip.dep));
            sum += own.unwrap_or_else(|| co::economy::passengers_for(trip.km, trip.dep, &r)) * mood;
        }
    }
    any.then_some(sum)
}

/// A line's fare (`fares::dialog`): its band, the chart of what each price does, what the fare
/// chosen (`fare`, cents) comes to.
pub fn fare_view(s: &mut Session, args: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let name = args.get("line").and_then(Value::as_str).unwrap_or("").to_string();
    let Some(cl) = c.lines.iter().find(|x| x.name == name).cloned() else { return json!({ "gone": true }) };
    let (lo, hi) = fr::band(&c, &cl);
    let p0 = fr::association_fare(&c);
    let best = fr::best_fare(&c, &cl);
    let cents = args.get("fare").and_then(Value::as_i64).unwrap_or_else(|| fr::fare_of(&c, &cl)).clamp(lo, hi);
    let cents = ((cents as f64 / 10.0).round() * 10.0) as i64;
    let band = if cl.own { tr("Your own line: its fare is yours to set, from %{lo} to %{hi}. The fare association's single ticket is %{p0}.") } else { tr("A concession line: the fare association allows %{lo} to %{hi} round its single ticket of %{p0}.") }.replace("%{lo}", &eur_cents(lo as f64)).replace("%{hi}", &eur_cents(hi as f64)).replace("%{p0}", &eur_cents(p0 as f64));
    let mut out = json!({
        "title": trf("The fare of line %{n}", &[("n", &cl.number)]), "band": band, "lo": lo, "hi": hi, "fare": cents, "fare_text": eur_cents(cents as f64),
        "slider": tr("Single ticket"), "p0": p0, "best": best,
        "set": trf("Set %{fare}", &[("fare", &eur_cents(cents as f64))]), "cancel": tr("Cancel"), "p0_label": tr("The association's fare"), "best_label": tr("The most fares"),
    });
    match base_passengers(s, &c, &cl) {
        Some(base) if base > 0.0 => {
            let n = 48;
            let pts: Vec<Value> = (0..=n)
                .map(|k| {
                    let p = lo + ((hi - lo) as f64 * k as f64 / n as f64).round() as i64;
                    let (pax, rev) = fr::estimate(&c, base, p);
                    json!([p, pax, rev])
                })
                .collect();
            let (pax, rev) = fr::estimate(&c, base, cents);
            let (pax0, rev0) = fr::estimate(&c, base, p0);
            let first = base * fr::short_run_share(fr::offset(&c, cents));
            let change = |a: f64, b: f64| if b > 0.0 { format!("{:+.0} %", (a / b - 1.0) * 100.0) } else { "–".into() };
            out["chart"] = json!({
                "points": pts, "pax": pax, "rev": rev, "lo_text": eur_cents(lo as f64), "hi_text": eur_cents(hi as f64),
                "pax_label": tr("Passengers a day"), "rev_label": tr("Fares a day"), "p0_mark": tr("Association"), "best_mark": tr("Most fares"),
                "scale": (0..=4).map(|k| { let f = 1.0 - k as f64 / 4.0; json!([grouped(pts.iter().filter_map(|p| p[1].as_f64()).fold(1.0, f64::max) * 1.08 * f), eur((pts.iter().filter_map(|p| p[2].as_f64()).fold(1.0, f64::max) * 1.08 * f).round() as i64)]) }).collect::<Vec<_>>(),
            });
            out["says"] = json!(tr("At %{fare}: about %{pax} passengers a day (%{dp} against the association's fare) and %{rev} of fares a day (%{dr}), once they have got used to it - a few weeks. The first days: about %{first} passengers.").replace("%{fare}", &eur_cents(cents as f64)).replace("%{pax}", &grouped(pax)).replace("%{dp}", &change(pax, pax0)).replace("%{rev}", &eur(rev.round() as i64)).replace("%{dr}", &change(rev, rev0)).replace("%{first}", &grouped(first)));
        }
        _ => out["says"] = json!(tr("The line has no tour today: what the fare does is shown on a day it runs.")),
    }
    if fr::reputation_cost(fr::offset(&c, cents)) > 0.0 {
        out["warn"] = json!(tr("Far above the association's fare: every day of it costs reputation as well."));
    }
    let share = cl.demand.share();
    if (share - 1.0).abs() > 0.005 {
        out["now"] = json!(trf("Its passengers now: %{p} of what the association's fare would bring.", &[("p", &format!("{:.0} %", share * 100.0))]));
    }
    out
}

/// What the finances do (the fare set; a loan's and a repayment's sheets are the dealer's).
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    Some(match what {
        "set_fare" => {
            let name = args.get("line").and_then(Value::as_str).unwrap_or("").to_string();
            let cents = args.get("fare").and_then(Value::as_i64).unwrap_or(0);
            let number = s.company.as_ref().and_then(|c| c.lines.iter().find(|x| x.name == name)).map(|x| x.number.clone()).unwrap_or_default();
            let casual = args.get("casual").and_then(Value::as_bool).unwrap_or(false);
            match apply(s, |c| fr::set_fare(c, &name, cents)) {
                Ok(done) => {
                    let t = if casual { "Line %{n}: a ticket costs %{fare} from now on." } else { "Line %{n}: the single ticket costs %{fare} from now on." };
                    let t = trf(t, &[("n", &number), ("fare", &eur_cents(done as f64))]);
                    s.say(t, false);
                    Ok(json!({ "done": true }))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        _ => return None,
    })
}
