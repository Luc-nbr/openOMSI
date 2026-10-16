//! The company's finances (`crates/omsi-launcher-ui/src/company/money.rs`, `bank.rs`, `fares.rs`):
//! cash, debt and what the fleet is worth, a month's bookings by kind, the bank's credit room and
//! the loans (a new one and paying back: the dealer module's loan sheets), the lines' fares and a
//! line's fare dialog, and the ledger.

use super::dealer::{percent, room_tone};
use super::words::{day_label, eur, fare_text, grouped, month_label, num, signed, tr, trf};
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
    // (the month's passengers: per bus-hour, and those a full bus left at the stop - `income`)
    let days: Vec<&co::model::DayRecord> = c.history.iter().filter(|d| d.date.starts_with(&months[k])).collect();
    let pax: u32 = days.iter().map(|d| d.passengers).sum();
    let hours: f64 = days.iter().map(|d| d.bus_hours).sum();
    let left: u32 = days.iter().map(|d| d.left_behind).sum();
    let pax = (pax > 0).then(|| {
        let mut t = trf("%{n} passengers", &[("n", &grouped(pax as f64))]);
        if hours > 0.0 {
            t.push_str("  ·  ");
            t.push_str(&trf("%{n} a bus-hour", &[("n", &num(pax as f64 / hours, 1))]));
        }
        t.push_str("  ·  ");
        t.push_str(&trf("%{n} left at the stop", &[("n", &grouped(left as f64))]));
        t
    });
    // (Luc 2026-10-16: the month as a profit and loss - its income and its costs by kind, each
    // with its share, and the waterfall from the income to the result)
    let income: Vec<&(BookingKind, i64)> = rows.iter().filter(|x| x.1 > 0).collect();
    let mut costs: Vec<&(BookingKind, i64)> = rows.iter().filter(|x| x.1 < 0).collect();
    costs.sort_by_key(|x| x.1);
    let earned: i64 = income.iter().map(|x| x.1).sum();
    let spent: i64 = -costs.iter().map(|x| x.1).sum::<i64>();
    let bars = |list: &[&(BookingKind, i64)], total: i64, tone: &str| -> Vec<Value> {
        let most = list.iter().map(|x| x.1.abs()).max().unwrap_or(1).max(1);
        list.iter()
            .map(|(kind, a)| json!({ "label": tr(kind.label()), "amount": eur(a.abs()), "frac": a.abs() as f64 / most as f64, "share": if total > 0 { format!("{:.0} %", a.abs() as f64 * 100.0 / total as f64) } else { String::new() }, "tone": tone }))
            .collect()
    };
    let mut fall = vec![json!({ "label": tr("Income"), "amount": eur(earned), "from": 0, "to": earned, "tone": "ok" })];
    let mut at = earned;
    for (k, (kind, a)) in costs.iter().enumerate() {
        // (the five largest costs each a step, the rest one)
        if k == 5 {
            let rest: i64 = costs[5..].iter().map(|x| x.1).sum();
            fall.push(json!({ "label": tr("Other costs"), "amount": eur(-rest), "from": at, "to": at + rest, "tone": "danger" }));
            break;
        }
        fall.push(json!({ "label": tr(kind.label()), "amount": eur(-a), "from": at, "to": at + a, "tone": "danger" }));
        at += a;
    }
    fall.push(json!({ "label": tr("Result"), "amount": signed(m.result()).0, "from": 0, "to": m.result(), "tone": if m.result() >= 0 { "ok" } else { "danger" }, "total": true }));
    json!({
        "pl_title": tr("Profit & loss"), "pl_sub": month_label(&months[k]),
        "income_title": tr("Income"), "income_total": eur(earned), "income": bars(&income, earned, "ok"),
        "costs_title": tr("Costs"), "costs_total": eur(spent), "costs": bars(&costs, spent, "danger"),
        "waterfall": fall, "no_income": tr("No income booked this month."), "no_costs": tr("No costs booked this month."),
        "pax": pax,
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
        "caps": tr("Credit room"), "tone": room_tone(room),
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
    json!({ "title": tr("Loans"), "credit": credit(c), "list": list, "none": tr("The company owes nothing."), "repay": tr("Pay back…"), "contract": tr("Contract"), "take": tr("Take a loan…") })
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
            let cat = if b.kind.is_capital() { "invest" } else if b.amount > 0 { "income" } else { "cost" };
            json!({ "date": day_label(&b.date), "kind": tr(b.kind.label()), "text": b.text, "mark": mark, "amount": t, "tone": tone, "cat": cat })
        })
        .collect();
    json!({
        "title": tr("Ledger"), "rows": rows,
        "filters": [["all", tr("All")], ["income", tr("Income")], ["cost", tr("Costs")], ["invest", tr("Investments")]],
        "search": tr("Search the ledger"), "none": tr("No booking matches."),
        "heads": [tr("Date"), tr("Kind"), tr("What"), "", tr("Amount")],
    })
}

fn fares(c: &Company) -> Value {
    if c.lines.is_empty() {
        return json!({ "title": tr("Ticket prices"), "empty": tr("The company runs no line yet: each line's single ticket is set here and on the Lines page.") });
    }
    let rows: Vec<Value> = c
        .lines
        .iter()
        .map(|cl| {
            let fare = fr::fare_of(c, cl);
            let rel = fr::offset(c, cl, fare);
            let share = cl.demand.share();
            let (t, tone) = if (share - 1.0).abs() < 0.005 { (tr("passengers as usual"), "dim") } else { (trf("%{p} passengers", &[("p", &format!("{:+.0} %", (share - 1.0) * 100.0))]), if share > 1.0 { "ok" } else { "warn" }) };
            let mut vs = if rel.abs() < 0.005 { tr("standard") } else { format!("{:+.0} %", rel * 100.0) };
            if cl.fare_cap {
                vs = format!("{vs} · {}", tr("capped"));
            }
            json!({ "name": cl.name, "plate": super::lines::plate(&cl.number, &cl.colour), "fare": fare_text(fare as f64), "vs": vs, "pax": t, "tone": tone })
        })
        .collect();
    json!({ "title": tr("Ticket prices"), "rows": rows, "tip": tr("Change the fare: see what it does before it is set") })
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
    // (the gross margin: what is left of the month's income when its running costs are paid)
    let margin = (this.income() > 0).then(|| this.result() as f64 / this.income() as f64);
    let mut figures = vec![
        json!({ "key": "cash", "icon": "payments", "label": tr("Cash"), "value": super::sandbox::cash(&c), "under": tr("in the bank"), "tone": super::sandbox::cash_tone(&c) }),
        json!({ "key": "month", "icon": "trending_up", "label": tr("This month"), "value": signed(this.result()).0, "under": trf("Last month: %{amount}", &[("amount", &signed(prev.result()).0)]), "tone": tone(this.result()), "tip": month_label(&this.month) }),
        json!({ "key": "margin", "icon": "speed", "label": tr("Gross margin"), "value": margin.map_or("–".to_string(), |m| percent(m)), "under": tr("of the month's income"), "tone": match margin { Some(m) if m >= 0.1 => "ok", Some(m) if m >= 0.0 => "warn", Some(_) => "danger", None => "text" }, "tip": tr("The month's result as a share of its income: what is left of the takings once the running costs are paid") }),
        json!({ "key": "debt", "icon": "receipt_long", "label": tr("Debt"), "value": eur(c.debt()), "under": trf("%{n} loans", &[("n", &c.loans.len().to_string())]), "tone": "text" }),
        json!({ "key": "fleet", "icon": "directions_bus", "label": tr("Fleet value"), "value": eur(fleet_value), "under": tr("the buses owned"), "tone": "text", "open": "fleet_value", "tip": tr("By age band: what the buses cost new, are worth and lost") }),
    ];
    if easy {
        figures.remove(4);
    }
    let month = ui.get("month").and_then(Value::as_str).map(str::to_string);
    json!({
        "easy": easy, "figures": figures, "month": month_part(&c, month.as_deref()), "loans": loans(&c),
        "ledger": (!easy).then(|| ledger(&c)), "fares": (!easy).then(|| fares(&c)),
    })
}

/// A line's passengers a day at its standard fare, from today's tours (`income::expected`).
fn base_passengers(s: &Session, c: &Company, line: &CompanyLine) -> Option<f64> {
    let plan = s.plan.as_ref()?;
    let mood = co::reputation::demand_factor(c.reputation);
    let mut sum = 0.0;
    let mut any = false;
    for t in plan.tours.iter().filter(|t| t.tour.line.eq_ignore_ascii_case(&line.name)) {
        for trip in t.tour.trips.iter().filter(|x| x.counts()) {
            any = true;
            let own = line.plan.as_ref().and_then(|_| co::ownline::trip_boardings(line, &c.date, trip.dep));
            sum += own.unwrap_or_else(|| co::income::expected(Some(line), &c.date, trip.dep, trip.arr)) * mood;
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
    let p0 = fr::standard_fare(&c, &cl);
    let best = fr::best_fare(&c, &cl);
    let cents = args.get("fare").and_then(Value::as_i64).unwrap_or_else(|| fr::fare_of(&c, &cl)).clamp(lo, hi);
    // (a round price of the company's currency: `currency::Currency::round_fare`)
    let cents = (c.currency.round_fare(cents as f64).round() as i64).clamp(lo, hi);
    let band = if cl.own { tr("Your own line: its adult single is yours to set, from %{lo} to %{hi}. Its standard fare, by the length of its trips, is %{p0}.") } else { tr("A map line: its adult single may be set from %{lo} to %{hi}, round its standard fare of %{p0} by the length of its trips.") }.replace("%{lo}", &fare_text(lo as f64)).replace("%{hi}", &fare_text(hi as f64)).replace("%{p0}", &fare_text(p0 as f64));
    // (the line's fare table at the fare chosen, and its cap: `income::table`)
    let mut at = cl.clone();
    at.fare = Some(cents);
    let t = co::income::table(&c, &at, fr::fare_of(&c, &at) as f64);
    let cap = co::income::table(&c, &CompanyLine { fare_cap: true, ..cl.clone() }, cents as f64).cap.unwrap_or_default();
    let mut out = json!({
        "title": trf("The fare of line %{n}", &[("n", &cl.number)]), "band": band, "lo": lo, "hi": hi, "fare": cents, "fare_text": fare_text(cents as f64),
        "slider": tr("Adult single"), "p0": p0, "best": best,
        "set": trf("Set %{fare}", &[("fare", &fare_text(cents as f64))]), "cancel": tr("Cancel"), "p0_label": tr("Standard fare"), "best_label": tr("The most takings"),
        "table": [[tr("Return ticket"), fare_text(t.ret)], [tr("Child single"), fare_text(t.child)], [tr("Day ticket"), fare_text(t.day)], [tr("Week ticket"), fare_text(t.week)]],
        "cap": { "on": cl.fare_cap, "label": trf("Fare cap: at most %{adult} for an adult single, %{child} for a child's", &[("adult", &fare_text(cap.0)), ("child", &fare_text(cap.1))]) },
    });
    match base_passengers(s, &c, &cl) {
        Some(base) if base > 0.0 => {
            let n = 48;
            let pts: Vec<Value> = (0..=n)
                .map(|k| {
                    let p = lo + ((hi - lo) as f64 * k as f64 / n as f64).round() as i64;
                    let (pax, rev) = fr::estimate(&c, &cl, base, p);
                    json!([p, pax, rev])
                })
                .collect();
            let (pax, rev) = fr::estimate(&c, &cl, base, cents);
            let (pax0, rev0) = fr::estimate(&c, &cl, base, p0);
            let first = base * fr::short_run_share(fr::felt(&c, &at));
            let change = |a: f64, b: f64| if b > 0.0 { format!("{:+.0} %", (a / b - 1.0) * 100.0) } else { "–".into() };
            out["chart"] = json!({
                "points": pts, "pax": pax, "rev": rev, "lo_text": fare_text(lo as f64), "hi_text": fare_text(hi as f64),
                "pax_label": tr("Passengers a day"), "rev_label": tr("Takings a day"), "p0_mark": tr("Standard fare"), "best_mark": tr("Most takings"),
                "scale": (0..=4).map(|k| { let f = 1.0 - k as f64 / 4.0; json!([grouped(pts.iter().filter_map(|p| p[1].as_f64()).fold(1.0, f64::max) * 1.08 * f), eur((pts.iter().filter_map(|p| p[2].as_f64()).fold(1.0, f64::max) * 1.08 * f).round() as i64)]) }).collect::<Vec<_>>(),
            });
            out["says"] = json!(tr("At %{fare}: about %{pax} passengers a day (%{dp} against the standard fare) and %{rev} of takings a day - fares, passes and the concession reimbursement (%{dr}), once they have got used to it - a few weeks. The first days: about %{first} passengers.").replace("%{fare}", &fare_text(cents as f64)).replace("%{pax}", &grouped(pax)).replace("%{dp}", &change(pax, pax0)).replace("%{rev}", &eur(rev.round() as i64)).replace("%{dr}", &change(rev, rev0)).replace("%{first}", &grouped(first)));
        }
        _ => out["says"] = json!(tr("The line has no tour today: what the fare does is shown on a day it runs.")),
    }
    if fr::reputation_cost(fr::felt(&c, &at)) > 0.0 {
        out["warn"] = json!(tr("Far above the standard fare: every day of it costs reputation as well."));
    }
    let share = cl.demand.share();
    if (share - 1.0).abs() > 0.005 {
        out["now"] = json!(trf("Its passengers now: %{p} of what the standard fare would bring.", &[("p", &format!("{:.0} %", share * 100.0))]));
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
                    let t = trf(t, &[("n", &number), ("fare", &fare_text(done as f64))]);
                    s.say(t, false);
                    Ok(json!({ "done": true }))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "set_cap" => {
            let name = args.get("line").and_then(Value::as_str).unwrap_or("").to_string();
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
            match apply(s, |c| fr::set_cap(c, &name, on)) {
                Ok(_) => Ok(json!({ "done": true })),
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use omsi_launcher_lib::company::BookingKind;

    /// Every line of the books is named in every language, and so are the cost texts of the UK
    /// model (crates/omsihub-locales/locales/company-costs.yml).
    #[test]
    fn every_cost_line_is_translated() {
        let mut texts: Vec<String> = BookingKind::ALL.iter().map(|k| k.label().to_string()).collect();
        let src = [include_str!("dialogs.rs"), include_str!("people.rs")].concat();
        for t in ["Drivers' wages %{amount}", "%{month} is closed: leases", "%{paid} of %{contract} hours paid", "This week"] {
            let start = src.find(t).expect(t);
            texts.push(src[start..].chars().take_while(|c| *c != '"').collect());
        }
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
    }

    /// The income model's texts too (crates/omsihub-locales/locales/company-income.yml): the
    /// kinds of line, the parts of the day, and the money page's, the report's and the lines
    /// page's words of the passengers and their takings.
    #[test]
    fn every_income_text_is_translated() {
        use omsi_launcher_lib::company::income::{LineType, BANDS};
        let mut texts: Vec<String> = LineType::ALL.iter().flat_map(|t| [t.label().to_string(), t.note().to_string()]).collect();
        texts.extend(BANDS.iter().map(|b| b.to_string()));
        // (money.rs without this test: its texts are the page's)
        let money = include_str!("money.rs");
        let src = [&money[..money.find("#[cfg(test)]").unwrap()], include_str!("lines.rs"), include_str!("dialogs.rs")].concat();
        for t in ["%{n} a bus-hour", "%{n} left at the stop", "Your own line: its adult single", "A map line: its adult single", "At %{fare}: about %{pax} passengers a day (%{dp} against the standard", "Fare cap: at most", "A typical adult single costs", "Passengers and takings", "Its last day's takings", "%{p} passengers, %{h} a bus-hour", "%{n} trips were full;", "Fuel grant (BSOG): %{km}", "Tourism grant: %{km}", "Fares collected for the authority (a gross", "Adult tickets", "Children's tickets", "Return ticket", "Week ticket", "Standard fare", "Takings a day"] {
            let start = src.find(t).expect(t);
            texts.push(src[start..].chars().take_while(|c| *c != '"').collect());
        }
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
    }
}
