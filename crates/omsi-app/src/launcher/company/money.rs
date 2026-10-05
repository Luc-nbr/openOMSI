//! The company's finances: cash, debt and what the fleet is worth, a month's bookings by kind
//! (what running the lines earned apart from buying, selling and financing), the loans and a
//! new one, and the ledger - every booking marked measured (driven) or modelled.

use super::super::theme::*;
use super::super::ui::ButtonKind;
use super::super::Launcher;
use super::{act, day_label, eur, figure, month_label, section};
use glam::Vec2;
use omsi_launcher_lib::company::{self as co, BookingKind, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

#[derive(Default)]
pub struct MoneyView {
    /// The month shown (0: the company's current one, 1 the one before, ...).
    month: usize,
    /// The loan asked for (an index into `AMOUNTS`).
    amount: usize,
}

const AMOUNTS: [i64; 5] = [50_000_00, 100_000_00, 250_000_00, 500_000_00, 1_000_000_00];

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let gap = 12.0;
    let fw = (area.w - 4.0 * gap) / 5.0;
    let this = c.month(&co::dates::month_of(&c.date));
    let prev_month = co::dates::month_of(&co::dates::add(&format!("{}-01", this.month), -1));
    let prev = c.month(&prev_month);
    let fleet_value = co::finance::fleet_value(&c);
    let figures = [
        ("Cash", eur(c.cash), String::new(), if c.cash >= 0 { TEXT } else { DANGER.lighten(0.2) }),
        ("Debt", eur(c.debt()), omsi_ui::tr("%{n} loans").replace("%{n}", &c.loans.len().to_string()), TEXT),
        ("Fleet value", eur(fleet_value), omsi_ui::tr("the buses owned").into_owned(), TEXT),
        ("This month", eur(this.result()), month_label(&this.month), if this.result() >= 0 { OK } else { DANGER.lighten(0.2) }),
        ("Last month", eur(prev.result()), month_label(&prev.month), if prev.result() >= 0 { OK } else { DANGER.lighten(0.2) }),
    ];
    for (k, (label, value, under, colour)) in figures.iter().enumerate() {
        figure(&mut l.ui, Rect::new(area.x + k as f32 * (fw + gap), area.y, fw, 84.0), label, value, under, *colour);
    }
    let y = area.y + 84.0 + gap;
    let h1 = ((area.bottom() - y - gap) * 0.56).max(200.0);
    let half = (area.w - gap) / 2.0;
    month_overview(l, Rect::new(area.x, y, half, h1), &c);
    loans(l, Rect::new(area.x + half + gap, y, half, h1), &c);
    let y2 = y + h1 + gap;
    ledger(l, Rect::new(area.x, y2, area.w, (area.bottom() - y2).max(100.0)), &c);
}

/// A month's bookings by kind: what came in, what went out, the result; and apart from it
/// the money of buying, selling and financing.
fn month_overview(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "The month");
    // the months there are, the newest first
    let mut months: Vec<String> = c.months.iter().map(|m| m.month.clone()).collect();
    let now = co::dates::month_of(&c.date);
    if !months.contains(&now) {
        months.push(now);
    }
    months.sort();
    months.reverse();
    let names: Vec<String> = months.iter().map(|m| month_label(m)).collect();
    let mut k = l.company.money.month.min(names.len().saturating_sub(1));
    if l.ui.select("company-month", Rect::new(r.right() - 196.0, r.y + 6.0, 180.0, 28.0), &mut k, &names) {
        l.company.money.month = k;
    }
    let m = c.month(&months[k]);
    let mut rows: Vec<(BookingKind, i64)> = BookingKind::ALL.iter().filter(|x| !x.is_capital()).map(|x| (*x, m.get(*x))).filter(|x| x.1 != 0).collect();
    rows.sort_by_key(|x| std::cmp::Reverse(x.1));
    let capital: Vec<(BookingKind, i64)> = BookingKind::ALL.iter().filter(|x| x.is_capital()).map(|x| (*x, m.get(*x))).filter(|x| x.1 != 0).collect();
    let mut y = inner.y;
    let rh = 22.0;
    if rows.is_empty() && capital.is_empty() {
        l.ui.text_in("Nothing booked this month yet.", Rect::new(inner.x, y, inner.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        return;
    }
    let bottom = inner.bottom() - 30.0;
    for (kind, amount) in &rows {
        if y + rh > bottom {
            break;
        }
        l.ui.text_in(kind.label(), Rect::new(inner.x, y, inner.w * 0.6, rh), 13.0, Weight::Regular, TEXT_SOFT, Align::Left);
        l.ui.text_in(&eur(*amount), Rect::new(inner.x + inner.w * 0.5, y, inner.w * 0.5, rh), 13.0, Weight::Medium, if *amount >= 0 { OK } else { TEXT }, Align::Right);
        y += rh;
    }
    l.ui.p().rect(Rect::new(inner.x, y + 2.0, inner.w, 1.0), HAIRLINE);
    y += 6.0;
    l.ui.text_in("Result", Rect::new(inner.x, y, inner.w * 0.6, rh + 2.0), 14.0, Weight::Bold, TEXT, Align::Left);
    l.ui.text_in(&eur(m.result()), Rect::new(inner.x + inner.w * 0.5, y, inner.w * 0.5, rh + 2.0), 15.0, Weight::Bold, if m.result() >= 0 { OK } else { DANGER.lighten(0.2) }, Align::Right);
    y += rh + 10.0;
    if !capital.is_empty() && y + rh < inner.bottom() + 10.0 {
        let text = capital.iter().map(|(k, a)| format!("{} {}", omsi_ui::tr(k.label()), eur(*a))).collect::<Vec<_>>().join("  ·  ");
        let line = format!("{}: {}", omsi_ui::tr("Investment and financing"), text);
        l.ui.paragraph(&line, Vec2::new(inner.x, y), inner.w, 11.5, Weight::Regular, TEXT_DIM);
    }
}

/// The loans, and a new one.
fn loans(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Loans");
    let mut y = inner.y;
    let mut repay: Option<(u32, i64)> = None;
    if c.loans.is_empty() {
        l.ui.text_in("The company owes nothing.", Rect::new(inner.x, y, inner.w, 20.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        y += 30.0;
    }
    for loan in &c.loans {
        if y + 44.0 > inner.bottom() - 110.0 {
            break;
        }
        let what = if loan.purpose.is_empty() { omsi_ui::tr("Loan").into_owned() } else { loan.purpose.clone() };
        l.ui.text_in(&what, Rect::new(inner.x, y, inner.w * 0.55, 20.0), 13.5, Weight::Bold, TEXT, Align::Left);
        let sub = omsi_ui::tr("%{left} of %{principal} left  ·  %{rate} %  ·  %{monthly} a month, %{n} months").replace("%{left}", &eur(loan.remaining)).replace("%{principal}", &eur(loan.principal)).replace("%{rate}", &format!("{:.1}", loan.rate * 100.0)).replace("%{monthly}", &eur(loan.monthly)).replace("%{n}", &loan.months_left.to_string());
        l.ui.text_in(&sub, Rect::new(inner.x, y + 20.0, inner.w - 150.0, 18.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
        let amount = loan.remaining.min(c.cash.max(0));
        if amount > 0 {
            let label = if amount >= loan.remaining { omsi_ui::tr("Pay it off").into_owned() } else { omsi_ui::tr("Repay %{amount}").replace("%{amount}", &eur(amount)) };
            if l.ui.button(&format!("company-repay-{}", loan.id), Rect::new(inner.right() - 140.0, y + 4.0, 140.0, 30.0), &label, None, ButtonKind::Normal) {
                repay = Some((loan.id, amount));
            }
        }
        y += 46.0;
    }
    // a new loan
    let by = inner.bottom() - 100.0;
    l.ui.p().rect(Rect::new(inner.x, by - 8.0, inner.w, 1.0), HAIRLINE);
    let left = co::finance::credit_left(c, 0);
    let t = omsi_ui::tr("The bank lends %{amount} more.").replace("%{amount}", &eur(left));
    l.ui.text_in(&t, Rect::new(inner.x, by, inner.w, 20.0), 13.0, Weight::Medium, TEXT_SOFT, Align::Left);
    let labels: Vec<String> = AMOUNTS.iter().map(|a| eur(*a)).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut k = l.company.money.amount;
    if l.ui.chips("company-loan-amount", Rect::new(inner.x, by + 26.0, inner.w, 30.0), &mut k, &refs) {
        l.company.money.amount = k;
    }
    let amount = AMOUNTS[l.company.money.amount.min(AMOUNTS.len() - 1)];
    let (monthly, months, rate) = co::finance::loan_terms(c, amount);
    let terms = omsi_ui::tr("%{monthly} a month for %{n} months at %{rate} %").replace("%{monthly}", &eur(monthly)).replace("%{n}", &months.to_string()).replace("%{rate}", &format!("{:.1}", rate * 100.0));
    l.ui.text_in(&terms, Rect::new(inner.x, by + 64.0, inner.w - 170.0, 30.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
    if l.ui.button("company-take-loan", Rect::new(inner.right() - 160.0, by + 62.0, 160.0, 32.0), "Take the loan", Some("payments"), ButtonKind::Normal) && act(l, |c| co::finance::take_loan(c, amount, "Loan", 0)).is_some() {
        l.state.set_status(omsi_ui::tr("The bank paid %{amount} into the cash.").replace("%{amount}", &eur(amount)), false);
    }
    if let Some((id, amount)) = repay {
        act(l, |c| co::finance::repay(c, id, amount));
    }
}

/// Every booking, the newest first.
fn ledger(l: &mut Launcher, r: Rect, c: &Company) {
    let inner = section(&mut l.ui, r, "Ledger");
    let list: Vec<co::Booking> = c.ledger.iter().rev().cloned().collect();
    if list.is_empty() {
        return;
    }
    l.ui.scroll_area("company-ledger", Rect::new(inner.x - 4.0, inner.y - 4.0, inner.w + 8.0, inner.h + 12.0), &mut |ui, v| {
        let rh = 24.0;
        for (k, b) in list.iter().enumerate() {
            let y = v.y + 4.0 + k as f32 * rh;
            let row = Rect::new(v.x + 4.0, y, v.w - 20.0, rh);
            if !ui.rect_visible(row) {
                continue;
            }
            ui.text_in(&day_label(&b.date), Rect::new(row.x, y, 150.0, rh), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
            ui.text_in(b.kind.label(), Rect::new(row.x + 156.0, y, 150.0, rh), 12.5, Weight::Medium, TEXT_SOFT, Align::Left);
            ui.text_in(&b.text, Rect::new(row.x + 312.0, y, row.w - 312.0 - 250.0, rh), 12.5, Weight::Regular, TEXT, Align::Left);
            // (measured or modelled says something only of what running the lines brought)
            if !b.kind.is_capital() {
                let (mark, colour) = if b.measured { ("driven", accent_2()) } else { ("modelled", TEXT_FAINT) };
                ui.text_in(mark, Rect::new(row.right() - 240.0, y, 90.0, rh), 11.0, Weight::Medium, colour, Align::Left);
            }
            ui.text_in(&eur(b.amount), Rect::new(row.right() - 150.0, y, 150.0, rh), 12.5, Weight::Bold, if b.amount >= 0 { OK } else { TEXT }, Align::Right);
        }
        list.len() as f32 * rh + 8.0
    });
}
