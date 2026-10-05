//! The bank (the rules are `company::finance`'s): a loan's dialog - how much, over how long,
//! what it costs a month and in all, what it does to the cash and to what the bank still
//! lends - and its contract, signed in the same way as the dealer's (`dealer::parties`,
//! `dealer::signature`) before anything is booked; and paying a loan back early, with its
//! fee. A loan the dealer's quick buy or contract asks for comes here first, and both are
//! signed at once (`finance::with_loan`).

use super::super::theme::*;
use super::super::ui::ButtonKind;
use super::super::Launcher;
use super::dealer::{self, Sheet, Then};
use super::fleet::price_row;
use super::{act, day_label, eur, meter};
use glam::Vec2;
use omsi_launcher_lib::company::dealer as dl;
use omsi_launcher_lib::company::finance::{self as fi, LoanContract, Room};
use omsi_launcher_lib::company::market::Payment;
use omsi_launcher_lib::company::Company;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

/// The colour of the credit room.
pub(super) fn room_colour(r: Room) -> Color {
    match r {
        Room::Plenty => OK,
        Room::Little => WARN,
        Room::None => DANGER.lighten(0.15),
    }
}

/// "%{rate} %" with one decimal ("4.5 %", "0 %").
pub(super) fn percent(rate: f64) -> String {
    let v = rate * 100.0;
    if (v - v.round()).abs() < 0.05 {
        format!("{} %", super::num(v, 0))
    } else {
        format!("{} %", super::num(v, 1))
    }
}

/// Open the loan's dialog from the finances page.
pub(super) fn open_loan(l: &mut Launcher) {
    let Some(c) = l.company.company.as_ref() else { return };
    let left = fi::credit(c, 0).left;
    let amount = (100_000_00i64).min(left).max(fi::SMALLEST_LOAN) / 100;
    dealer::open(l, Sheet::Loan { amount: amount as f32, term: usize::MAX, purpose: String::new(), collateral: 0, fixed: false, then: Then::Nothing });
}

/// The rows of a sheet: what, and the amount.
fn rows(l: &mut Launcher, r: Rect, y: &mut f32, list: &[(String, String, Option<Color>)]) {
    let rh = 27.0;
    for (a, b, colour) in list {
        price_row(&mut l.ui, Rect::new(r.x, *y, r.w, rh), a, b, false);
        if let Some(c) = colour {
            // (the value in its colour over the plain one)
            l.ui.p().rect(Rect::new(r.x + r.w * 0.45, *y + 2.0, r.w * 0.55, rh - 4.0), PANEL);
            l.ui.text_in(b, Rect::new(r.x + r.w * 0.4, *y, r.w * 0.6, rh), 13.0, Weight::Bold, *c, Align::Right);
        }
        *y += rh;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn loan_sheet(l: &mut Launcher, c: &Company, amount: f32, term: usize, purpose: String, collateral: i64, fixed: bool, then: Then, esc: bool) -> Option<Sheet> {
    let inner = super::dialog_panel(l, 780.0, if fixed { 560.0 } else { 640.0 }, "payments", &omsi_ui::tr("A loan from the bank"));
    let cr = fi::credit(c, collateral);
    let terms = fi::terms_offered(c);
    let longest = terms.len().saturating_sub(1);
    let mut term = if term == usize::MAX { longest } else { term.min(longest) };
    let mut amount = amount;
    let mut y = inner.y;
    if fixed {
        let t = omsi_ui::tr("For %{what}: the bank lends what is to be paid, and the buses are its security.").replace("%{what}", &purpose);
        y += l.ui.paragraph(&t, Vec2::new(inner.x, y), inner.w, 13.0, Weight::Regular, TEXT_SOFT) + 8.0;
    }
    // how much
    l.ui.text_in(&omsi_ui::tr("Amount").to_uppercase(), Rect::new(inner.x, y, inner.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    let cents = (amount.round() as i64) * 100;
    l.ui.text_in(&eur(cents), Rect::new(inner.x, y + 14.0, inner.w * 0.5, 40.0), 32.0, Weight::Bold, TEXT, Align::Left);
    let left_t = omsi_ui::tr("The bank lends up to %{amount} more").replace("%{amount}", &eur(cr.left));
    l.ui.text_in(&left_t, Rect::new(inner.x + inner.w * 0.5, y + 24.0, inner.w * 0.5, 24.0), 13.0, Weight::Medium, room_colour(fi::room(&cr)), Align::Right);
    y += 62.0;
    if !fixed {
        let max = (cr.left.max(fi::SMALLEST_LOAN) / 100) as f32;
        l.ui.slider("company-loan-amount", Rect::new(inner.x, y, inner.w, 30.0), &mut amount, (fi::SMALLEST_LOAN / 100) as f32, max, 5_000.0, "", &|v| format!("{:.0}k", v / 1000.0));
        y += 40.0;
        let presets: Vec<i64> = [50_000_00i64, 100_000_00, 250_000_00, 500_000_00].into_iter().filter(|p| *p <= cr.left).collect();
        let mut labels: Vec<String> = presets.iter().map(|p| eur(*p)).collect();
        labels.push(omsi_ui::tr("All that is left").into_owned());
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut k = presets.iter().position(|p| *p == cents).unwrap_or(if cents == cr.left { presets.len() } else { usize::MAX });
        if l.ui.chips("company-loan-presets", Rect::new(inner.x, y, inner.w, 30.0), &mut k, &refs) {
            amount = (presets.get(k).copied().unwrap_or(cr.left) / 100) as f32;
        }
        y += 44.0;
    }
    // over how long
    l.ui.label(Rect::new(inner.x, y, inner.w, 16.0), "Term");
    y += 20.0;
    let labels: Vec<String> = terms.iter().map(|m| omsi_ui::tr("%{n} months").replace("%{n}", &m.to_string())).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    l.ui.chips("company-loan-term", Rect::new(inner.x, y, inner.w, 30.0), &mut term, &refs);
    y += l.ui.chips_height(inner.w, 30.0, &refs) + 16.0;
    // what it costs, and what it does
    let cents = (amount.round() as i64) * 100;
    let months = terms.get(term).copied().unwrap_or(12);
    let k = fi::draft_loan(c, cents, months, &purpose, collateral);
    let after = (cr.left - cents).max(0);
    let after_room = fi::room(&fi::Credit { left: after, ..cr });
    let rate = if cr.discount > 0.0 { omsi_ui::tr("%{rate} a year (%{discount} less for the company's level)").replace("%{rate}", &percent(k.rate)).replace("%{discount}", &percent(cr.discount)) } else { omsi_ui::tr("%{rate} a year").replace("%{rate}", &percent(k.rate)) };
    let list = vec![
        (omsi_ui::tr("Interest").into_owned(), rate, None),
        (omsi_ui::tr("Monthly payment").into_owned(), eur(k.monthly), Some(TEXT)),
        (omsi_ui::tr("Paid back in all").into_owned(), eur(k.total), None),
        (omsi_ui::tr("Of which interest").into_owned(), eur(k.interest()), None),
        (omsi_ui::tr("First payment").into_owned(), day_label(&k.first_rate), None),
        (omsi_ui::tr("Cash afterwards").into_owned(), if fixed { eur(c.cash) } else { eur(c.cash + cents) }, None),
        (omsi_ui::tr("Credit room afterwards").into_owned(), eur(after), Some(room_colour(after_room))),
    ];
    rows(l, Rect::new(inner.x, y, inner.w, 0.0), &mut y, &list);
    let ok = cents >= fi::SMALLEST_LOAN.min(cents.max(1)) && cents > 0 && cents <= cr.left;
    let by = inner.bottom() - 38.0;
    if !ok {
        l.ui.text_in("The bank does not lend that much.", Rect::new(inner.x, by, 320.0, 38.0), 13.0, Weight::Medium, WARN, Align::Left);
    }
    if l.ui.button("company-loan-cancel", Rect::new(inner.right() - 450.0, by, 140.0, 38.0), "Cancel", None, ButtonKind::Normal) || esc {
        return back_from_loan(then);
    }
    if l.ui.button("company-loan-draw-up", Rect::new(inner.right() - 300.0, by, 300.0, 38.0), "Draw up the loan contract", Some("description"), ButtonKind::Primary) && ok {
        return Some(Sheet::LoanContract { contract: k, strokes: Vec::new(), readonly: false, collateral, then });
    }
    Some(Sheet::Loan { amount, term, purpose, collateral, fixed, then })
}

/// Leaving a loan's dialog: back to the purchase that asked for it.
fn back_from_loan(then: Then) -> Option<Sheet> {
    match then {
        Then::Purchase(k) => Some(Sheet::Contract { contract: *k, strokes: Vec::new(), readonly: false }),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn loan_contract_sheet(l: &mut Launcher, c: &Company, contract: LoanContract, strokes: Vec<Vec<Vec2>>, readonly: bool, collateral: i64, then: Then, esc: bool) -> Option<Sheet> {
    let title = if readonly { omsi_ui::tr("Loan contract no. %{no}").replace("%{no}", &contract.no.to_string()) } else { omsi_ui::tr("Loan contract").into_owned() };
    let inner = super::dialog_panel(l, 940.0, 700.0, "description", &title);
    let (mut k, mut strokes) = (contract, strokes);
    let home = if c.map_name.is_empty() { c.depot.clone() } else { format!("{}  Â·  {}", c.map_name, c.depot) };
    let y = dealer::parties(l, inner, ("Lender", &k.lender, "Business bank"), ("Borrower", &k.borrower, &home));
    let gap = 18.0;
    let cw = (inner.w - gap) / 2.0;
    // the terms
    let left = Rect::new(inner.x, y, cw, 0.0);
    let mut ly = y;
    l.ui.text_in(&omsi_ui::tr("The loan").to_uppercase(), Rect::new(left.x, ly, left.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    ly += 18.0;
    let early = if k.early_fee <= 0.0 { omsi_ui::tr("at any time, free of charge").into_owned() } else { omsi_ui::tr("at any time, a fee of %{rate} of what is paid back").replace("%{rate}", &percent(k.early_fee)) };
    let purpose = if k.purpose.trim().is_empty() { omsi_ui::tr("The company's needs").into_owned() } else { k.purpose.clone() };
    let list = vec![
        (omsi_ui::tr("Amount").into_owned(), eur(k.amount), Some(TEXT)),
        (omsi_ui::tr("Purpose").into_owned(), purpose, None),
        (omsi_ui::tr("Interest").into_owned(), omsi_ui::tr("%{rate} a year").replace("%{rate}", &percent(k.rate)), None),
        (omsi_ui::tr("Term").into_owned(), omsi_ui::tr("%{n} months").replace("%{n}", &k.months.to_string()), None),
        (omsi_ui::tr("Monthly payment").into_owned(), eur(k.monthly), None),
        (omsi_ui::tr("Paid back in all").into_owned(), eur(k.total), None),
        (omsi_ui::tr("First payment").into_owned(), day_label(&k.first_rate), None),
    ];
    rows(l, left, &mut ly, &list);
    ly += 8.0;
    let security = omsi_ui::tr("Security: the company's buses, worth %{amount}.").replace("%{amount}", &eur(k.security));
    ly += l.ui.paragraph(&security, Vec2::new(left.x, ly), left.w, 12.0, Weight::Regular, TEXT_SOFT);
    let early = omsi_ui::tr("Paying back early: %{terms}.").replace("%{terms}", &early);
    l.ui.paragraph(&early, Vec2::new(left.x, ly + 2.0), left.w, 12.0, Weight::Regular, TEXT_SOFT);
    // the repayment schedule
    let right = Rect::new(inner.x + cw + gap, y, cw, 0.0);
    l.ui.text_in(&omsi_ui::tr("Repayment").to_uppercase(), Rect::new(right.x, y, right.w, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    let plan = fi::schedule(k.amount, k.rate, k.months);
    let cols = [("Month", 0.12), ("Payment", 0.22), ("Interest", 0.22), ("Repaid", 0.22), ("Owed", 0.22)];
    let mut ry = y + 20.0;
    let mut x = right.x;
    for (h, w) in cols {
        l.ui.text_in(h, Rect::new(x, ry, right.w * w - 6.0, 18.0), 11.0, Weight::Bold, TEXT_DIM, if h == "Month" { Align::Left } else { Align::Right });
        x += right.w * w;
    }
    ry += 22.0;
    let shown: Vec<Option<&fi::Instalment>> = if plan.len() <= 9 { plan.iter().map(Some).collect() } else { plan.iter().take(6).map(Some).chain([None]).chain(plan.iter().rev().take(2).rev().map(Some)).collect() };
    for i in shown {
        let rr = Rect::new(right.x, ry, right.w, 21.0);
        match i {
            Some(i) => {
                let cells = [i.month.to_string(), eur(i.payment), eur(i.interest), eur(i.principal), eur(i.remaining)];
                let mut x = rr.x;
                for (k, (_, w)) in cols.iter().enumerate() {
                    l.ui.text_in(&cells[k], Rect::new(x, ry, right.w * w - 6.0, 21.0), 11.5, Weight::Regular, if k == 0 { TEXT_DIM } else { TEXT_SOFT }, if k == 0 { Align::Left } else { Align::Right });
                    x += right.w * w;
                }
            }
            None => {
                l.ui.text_in("â€¦", rr, 12.0, Weight::Bold, TEXT_FAINT, Align::Center);
            }
        }
        ry += 21.0;
    }
    l.ui.p().rect(Rect::new(right.x, ry + 2.0, right.w, 1.0), HAIRLINE);
    let interest: i64 = plan.iter().map(|i| i.interest).sum();
    l.ui.text_in(&omsi_ui::tr("Interest in all: %{amount}").replace("%{amount}", &eur(interest)), Rect::new(right.x, ry + 6.0, right.w, 20.0), 12.0, Weight::Medium, TEXT_SOFT, Align::Right);
    // the signature
    let day = if readonly { k.signed_at.clone() } else { c.date.clone() };
    let kept = dealer::signature(l, inner, &mut strokes, &mut k.signed_by, &k.strokes.clone(), readonly, &day);
    let by = inner.bottom() - 38.0;
    if readonly {
        if l.ui.button("company-loan-contract-close", Rect::new(inner.right() - 130.0, by, 130.0, 38.0), "Close", None, ButtonKind::Primary) || esc {
            return None;
        }
        return Some(Sheet::LoanContract { contract: k, strokes, readonly, collateral, then });
    }
    if l.ui.button("company-loan-contract-back", Rect::new(inner.right() - 460.0, by, 150.0, 38.0), "Back", None, ButtonKind::Normal) || esc {
        let fixed = !matches!(then, Then::Nothing);
        let term = fi::terms_offered(c).iter().position(|m| *m == k.months).unwrap_or(usize::MAX);
        return Some(Sheet::Loan { amount: (k.amount / 100) as f32, term, purpose: k.purpose.clone(), collateral, fixed, then });
    }
    k.strokes = kept;
    let signed = k.is_signed();
    if !signed {
        l.ui.text_in("Sign the contract first.", Rect::new(inner.x, by, 300.0, 38.0), 13.0, Weight::Medium, TEXT_DIM, Align::Left);
    }
    if l.ui.button("company-loan-sign", Rect::new(inner.right() - 300.0, by, 300.0, 38.0), "Sign the loan contract", Some("check_circle"), ButtonKind::Primary) && signed {
        let listings = l.company.fleet.dealer.listings.clone().unwrap_or_default();
        let now = dl::now_of(c);
        match then {
            Then::Nothing => {
                if let Some(id) = act(l, |c| fi::sign_loan(c, &k, collateral)) {
                    l.state.set_status(omsi_ui::tr("Loan contract %{no} is signed: the bank paid %{amount} into the cash.").replace("%{no}", &id.to_string()).replace("%{amount}", &eur(k.amount)), false);
                    return None;
                }
            }
            Then::Purchase(ref p) => {
                if let Some((_, done)) = act(l, |c| dl::sign_financed(c, p, &k, &listings, &now)) {
                    dealer::signed_status(l, done);
                    return None;
                }
            }
            Then::Quick { ref listing, count, ref livery } => {
                if let Some((_, ids)) = act(l, |c| fi::with_loan(c, &k, collateral, |c| dl::quick_buy(c, listing, count, Payment::Cash, livery))) {
                    dealer::joined(l, &ids);
                    return None;
                }
            }
            Then::QuickOffer { ref offer, count, ref livery } => {
                if let Some((_, ids)) = act(l, |c| fi::with_loan(c, &k, collateral, |c| dl::quick_buy_offer(c, offer, count, Payment::Cash, livery, &listings))) {
                    dealer::joined(l, &ids);
                    return None;
                }
            }
        }
    }
    Some(Sheet::LoanContract { contract: k, strokes, readonly, collateral, then })
}

/// Paying a loan back early: what it costs, and what is left.
pub(super) fn repay_sheet(l: &mut Launcher, c: &Company, id: u32, amount: i64, esc: bool) -> Option<Sheet> {
    let Some(loan) = c.loans.iter().find(|x| x.id == id).cloned() else { return None };
    let inner = super::dialog_panel(l, 540.0, 330.0, "payments", &omsi_ui::tr("Pay back early"));
    let amount = amount.min(loan.remaining);
    let fee = fi::early_fee(c, amount);
    let what = if loan.purpose.is_empty() { omsi_ui::tr("Loan").into_owned() } else { loan.purpose.clone() };
    let mut y = inner.y;
    let fee_label = omsi_ui::tr("Early repayment fee (%{rate})").replace("%{rate}", &percent(fi::early_fee_rate(c.difficulty)));
    let list = vec![
        (omsi_ui::tr("Loan").into_owned(), what, None),
        (omsi_ui::tr("Paid back now").into_owned(), eur(amount), None),
        (fee_label, eur(fee), if fee > 0 { Some(WARN) } else { None }),
        (omsi_ui::tr("From the cash").into_owned(), eur(amount + fee), Some(TEXT)),
        (omsi_ui::tr("Still owed afterwards").into_owned(), eur(loan.remaining - amount), None),
    ];
    rows(l, inner, &mut y, &list);
    y += 8.0;
    let note = if amount >= loan.remaining { "The loan is paid off: no more rates." } else { "The monthly payment stays the same: the loan ends sooner." };
    l.ui.paragraph(note, Vec2::new(inner.x, y), inner.w, 12.5, Weight::Regular, TEXT_DIM);
    let ok = c.cash >= amount + fee;
    let by = inner.bottom() - 38.0;
    if !ok {
        l.ui.text_in("Not enough cash.", Rect::new(inner.x, by, 160.0, 38.0), 13.0, Weight::Medium, WARN, Align::Left);
    }
    if l.ui.button("company-repay-cancel", Rect::new(inner.right() - 380.0, by, 130.0, 38.0), "Cancel", None, ButtonKind::Normal) || esc {
        return None;
    }
    let label = omsi_ui::tr("Pay back %{amount}").replace("%{amount}", &eur(amount + fee));
    if l.ui.button("company-repay-do", Rect::new(inner.right() - 240.0, by, 240.0, 38.0), &label, Some("check_circle"), ButtonKind::Primary) && ok && act(l, |c| fi::repay(c, id, amount)).is_some() {
        l.state.set_status(omsi_ui::tr("Paid back: %{amount}.").replace("%{amount}", &eur(amount + fee)), false);
        return None;
    }
    Some(Sheet::Repay { id, amount })
}

/// The credit room on the finances page: how much more the bank lends, in its colour, with
/// what it is made of and the share used.
pub(super) fn credit_figure(l: &mut Launcher, r: Rect, c: &Company) -> f32 {
    let cr = fi::credit(c, 0);
    let room = fi::room(&cr);
    let colour = room_colour(room);
    let lw = r.w * 0.46;
    l.ui.text_in(&omsi_ui::tr("Credit room").to_uppercase(), Rect::new(r.x, r.y, lw, 14.0), 10.0, Weight::Bold, TEXT_DIM, Align::Left);
    let value = if room == Room::None { omsi_ui::tr("%{amount} left").replace("%{amount}", &eur(cr.left)) } else { eur(cr.left) };
    l.ui.text_in(&value, Rect::new(r.x, r.y + 14.0, lw, 40.0), 32.0, Weight::Bold, colour, Align::Left);
    let say = match room {
        Room::Plenty => omsi_ui::tr("the bank lends this much more"),
        Room::Little => omsi_ui::tr("little left: the bank is careful"),
        Room::None => omsi_ui::tr("used up: the bank lends nothing more"),
    };
    l.ui.text_in(&say, Rect::new(r.x, r.y + 56.0, lw, 18.0), 12.0, Weight::Medium, colour, Align::Left);
    let used = if cr.limit > 0 { (cr.debt as f64 / cr.limit as f64).clamp(0.0, 1.0) } else { 1.0 };
    meter(&mut l.ui, Rect::new(r.x, r.y + 82.0, lw - 10.0, 6.0), 1.0 - used, colour);
    let of = omsi_ui::tr("%{debt} of %{limit} used").replace("%{debt}", &eur(cr.debt)).replace("%{limit}", &eur(cr.limit));
    l.ui.text_in(&of, Rect::new(r.x, r.y + 92.0, lw, 18.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
    // what it is made of
    let rx = r.x + lw + 14.0;
    let rw = r.w - lw - 14.0;
    let mut y = r.y;
    let lines = [
        (omsi_ui::tr("Base for a %{difficulty} company").replace("%{difficulty}", &omsi_ui::tr(c.difficulty.label()).to_lowercase()), eur(cr.base)),
        (omsi_ui::tr("%{share} of the fleet's %{value}").replace("%{share}", &percent(cr.share)).replace("%{value}", &eur(cr.fleet_value)), eur((cr.share * cr.fleet_value as f64).round() as i64)),
        (omsi_ui::tr("6 months of the result (%{amount} a month)").replace("%{amount}", &eur(cr.income)), eur(fi::INCOME_MONTHS * cr.income)),
        (omsi_ui::tr("Owed already").into_owned(), format!("- {}", eur(cr.debt))),
    ];
    for (a, b) in lines {
        l.ui.text_in(&a, Rect::new(rx, y, rw * 0.66, 20.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
        l.ui.text_in(&b, Rect::new(rx + rw * 0.6, y, rw * 0.4, 20.0), 11.5, Weight::Medium, TEXT_SOFT, Align::Right);
        y += 20.0;
    }
    let rate = if cr.discount > 0.0 { omsi_ui::tr("Interest %{rate} a year (%{discount} less for the company's level)").replace("%{rate}", &percent(cr.rate)).replace("%{discount}", &percent(cr.discount)) } else { omsi_ui::tr("Interest %{rate} a year").replace("%{rate}", &percent(cr.rate)) };
    l.ui.text_in(&rate, Rect::new(rx, y + 4.0, rw, 20.0), 11.5, Weight::Medium, TEXT_SOFT, Align::Left);
    114.0
}
