//! The company's currency chosen (Beta 62): the founding's select and the settings' one. The
//! money stays in euros underneath (`omsi_launcher_lib::company::currency`): a currency changed
//! changes only how the pages show and read it, never what the company has.

use super::words::{self, eur, tr, trf};
use super::{act as apply, Session};
use crate::Answer;
use omsi_launcher_lib::company::currency::Currency;
use omsi_launcher_lib::company::Company;
use serde_json::{json, Value};

/// The currencies as the select lists them: "Euro (EUR)".
fn options() -> Vec<String> {
    Currency::ALL.iter().map(|c| format!("{} ({})", tr(c.name()), c.code())).collect()
}

fn codes() -> Vec<&'static str> {
    Currency::ALL.iter().map(|c| c.code()).collect()
}

/// The currency a founding chose (`currency`: its code; the euro else).
pub fn chosen(args: &Value) -> Currency {
    args.get("currency").and_then(Value::as_str).and_then(Currency::of_code).unwrap_or_default()
}

fn index(c: Currency) -> usize {
    Currency::ALL.iter().position(|x| *x == c).unwrap_or(0)
}

/// The founding wizard's select: its label, the currencies and their codes, the one chosen.
pub fn wizard_part(args: &Value) -> Value {
    let c = chosen(args);
    json!({ "label": tr("Currency"), "options": options(), "codes": codes(), "value": index(c), "code": c.code() })
}

/// The settings' select, with what changing it does.
pub fn settings_part(c: &Company) -> Value {
    json!({
        "caps": tr("Currency").to_uppercase(), "options": options(), "codes": codes(), "value": index(c.currency), "code": c.currency.code(),
        "say": tr("Money is shown and typed in this currency. Changing it changes no balance: the amounts keep their value."),
    })
}

/// The company's money shown in `code` from now on (the settings' select).
pub fn set(s: &mut Session, code: &str) -> Answer {
    let Some(to) = Currency::of_code(code) else { return Ok(json!({})) };
    let r = apply(s, |c| {
        c.currency = to;
        Ok(c.cash)
    });
    match r {
        Ok(cash) => {
            words::set_currency(to);
            let t = trf("The company's money is shown in %{currency} now: %{amount} in the bank.", &[("currency", &tr(to.name())), ("amount", &eur(cash))]);
            s.say(t, false);
            Ok(json!({}))
        }
        Err(p) => Ok(json!({ "popup": p })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{self as co, Founding};

    #[test]
    fn a_company_of_before_is_in_euros_and_a_founding_keeps_its_choice() {
        // (a file of before the currencies has none)
        let c = co::found(&Founding { name: "Alt".into(), ..Default::default() }, "Luc");
        let mut file = serde_json::to_value(&c).unwrap();
        file.as_object_mut().unwrap().remove("currency");
        let old: Company = serde_json::from_value(file).unwrap();
        assert_eq!(old.currency, Currency::Eur);
        // the founding's choice, saved by its code
        let gbp = co::found(&Founding { name: "Neu".into(), currency: chosen(&json!({ "currency": "GBP" })), ..Default::default() }, "Luc");
        assert_eq!(gbp.currency, Currency::Gbp);
        assert_eq!(serde_json::to_value(&gbp).unwrap()["currency"], "GBP");
        assert_eq!(gbp.cash, c.cash);
        assert_eq!(chosen(&json!({})), Currency::Eur);
        assert_eq!(wizard_part(&json!({ "currency": "HUF" }))["value"], 6);
    }
}
