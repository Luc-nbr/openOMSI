//! Loans: the bank lends against a fixed amount and the fleet's value (and the bus a loan
//! buys), at the difficulty's interest (none on Easy), paid back in equal monthly rates.

use super::economy;
use super::market;
use super::model::{BookingKind, Cents, Company, Loan};

/// What the owned fleet is worth.
pub fn fleet_value(c: &Company) -> Cents {
    c.fleet.iter().map(|v| market::value_of(c, v)).sum()
}

/// How much the bank lends in all, with `collateral` (the value of what the loan buys).
pub fn credit_limit(c: &Company, collateral: Cents) -> Cents {
    let r = economy::rules(c.difficulty);
    (r.credit_base as f64 * c.price_index + r.credit_share * (fleet_value(c) + collateral) as f64).round() as Cents
}

/// How much more the bank lends.
pub fn credit_left(c: &Company, collateral: Cents) -> Cents {
    (credit_limit(c, collateral) - c.debt()).max(0)
}

/// What a loan of `amount` costs a month, and over how many months.
pub fn loan_terms(c: &Company, amount: Cents) -> (Cents, u32, f64) {
    let r = economy::rules(c.difficulty);
    (economy::annuity(amount, r.loan_rate, r.loan_months), r.loan_months, r.loan_rate)
}

/// Take a loan: the money is in the cash at once. Returns its id.
pub fn take_loan(c: &mut Company, amount: Cents, purpose: &str, collateral: Cents) -> Result<u32, &'static str> {
    if amount <= 0 {
        return Err("Choose an amount.");
    }
    if amount > credit_left(c, collateral) {
        return Err("The bank does not lend that much.");
    }
    let (monthly, months, rate) = loan_terms(c, amount);
    c.counters.loan += 1;
    let id = c.counters.loan;
    c.loans.push(Loan { id, taken: c.date.clone(), principal: amount, remaining: amount, rate, monthly, months_left: months, purpose: purpose.to_string() });
    c.book(BookingKind::Loan, amount, purpose.to_string(), false);
    Ok(id)
}

/// Pay back `amount` of a loan early, from the cash (the rate stays, the term shortens).
pub fn repay(c: &mut Company, id: u32, amount: Cents) -> Result<(), &'static str> {
    let Some(i) = c.loans.iter().position(|l| l.id == id) else { return Err("There is no such loan.") };
    let amount = amount.min(c.loans[i].remaining);
    if amount <= 0 {
        return Err("Choose an amount.");
    }
    if c.cash < amount {
        return Err("Not enough cash.");
    }
    c.loans[i].remaining -= amount;
    let purpose = c.loans[i].purpose.clone();
    c.book(BookingKind::Repayment, -amount, purpose, false);
    if c.loans[i].remaining <= 0 {
        c.loans.remove(i);
    }
    Ok(())
}

/// The month's rates of every loan: interest on what is left, the rest repays it. Returns
/// the purposes of the loans paid off.
pub fn pay_rates(c: &mut Company) -> Vec<String> {
    let mut done = Vec::new();
    let loans = std::mem::take(&mut c.loans);
    let mut kept = Vec::new();
    for mut l in loans {
        let interest = (l.remaining as f64 * l.rate / 12.0).round() as Cents;
        let principal = (l.monthly - interest).clamp(0, l.remaining);
        // (the last rate pays what is left, whatever the rounding left over)
        let principal = if l.months_left <= 1 { l.remaining } else { principal };
        c.book(BookingKind::Interest, -interest, l.purpose.clone(), false);
        c.book(BookingKind::Repayment, -principal, l.purpose.clone(), false);
        l.remaining -= principal;
        l.months_left = l.months_left.saturating_sub(1);
        if l.remaining <= 0 {
            done.push(l.purpose.clone());
        } else {
            kept.push(l);
        }
    }
    c.loans = kept;
    done
}

#[cfg(test)]
mod tests {
    use super::super::{found, Founding};
    use super::*;
    use crate::company::model::Difficulty;

    #[test]
    fn a_loan_is_paid_off_in_its_months() {
        let mut c = found(&Founding { name: "Bank".into(), difficulty: Difficulty::Realistic, ..Default::default() }, "Luc");
        let cash = c.cash;
        assert!(take_loan(&mut c, 10_000_000_00, "too much", 0).is_err());
        let id = take_loan(&mut c, 100_000_00, "bus", 0).unwrap();
        assert_eq!(c.cash, cash + 100_000_00);
        assert_eq!(c.debt(), 100_000_00);
        let mut paid = 0;
        let mut interest = 0;
        for _ in 0..72 {
            let before = c.cash;
            let m = c.month(&super::super::dates::month_of(&c.date)).get(BookingKind::Interest);
            pay_rates(&mut c);
            interest = c.month(&super::super::dates::month_of(&c.date)).get(BookingKind::Interest) - m + interest;
            paid += before - c.cash;
        }
        assert!(c.loans.is_empty(), "{:?}", c.loans);
        // the rates came to the principal and the interest on it
        assert!(paid > 100_000_00 && paid < 115_000_00, "{paid}");
        assert_eq!(paid, 100_000_00 - interest);
        // an early repayment shortens it
        let id2 = take_loan(&mut c, 50_000_00, "more", 0).unwrap();
        repay(&mut c, id2, 20_000_00).unwrap();
        assert_eq!(c.debt(), 30_000_00);
        repay(&mut c, id2, 99_000_00).unwrap();
        assert!(c.loans.is_empty());
        let _ = id;
        // Easy lends without interest
        let mut e = found(&Founding { name: "Easy".into(), difficulty: Difficulty::Easy, ..Default::default() }, "Luc");
        take_loan(&mut e, 96_000_00, "x", 0).unwrap();
        assert_eq!(e.loans[0].monthly, 1_000_00);
    }
}
