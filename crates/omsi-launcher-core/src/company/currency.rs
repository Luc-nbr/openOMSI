//! The company's currency (Beta 62): how its money is shown and typed. Everything stays in euro
//! cents - the economy, the balancing, the saves; a currency only converts what the pages show
//! at a fixed rate, and writes it the way its country writes money. A company of before has
//! none: it is in euros.

use super::Cents;
use serde::{Deserialize, Serialize};

/// A currency the company can keep its books in (its ISO 4217 code in the file; an unknown
/// code reads as the euro).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(from = "String", into = "&'static str")]
pub enum Currency {
    #[default]
    Eur,
    Gbp,
    Usd,
    Chf,
    Pln,
    Czk,
    Huf,
    Sek,
    Nok,
    Dkk,
    Ron,
    Try,
    Uah,
    Cad,
    Aud,
}

impl From<String> for Currency {
    fn from(code: String) -> Self {
        Currency::of_code(&code).unwrap_or_default()
    }
}

impl From<Currency> for &'static str {
    fn from(c: Currency) -> Self {
        c.code()
    }
}

/// Where the symbol goes: "£1,234", "CHF 1’234", "1 234 zł".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolAt {
    Before,
    BeforeSpaced,
    After,
}

/// How a currency's money is written: its symbol and where it goes, the thousands separator,
/// the decimal mark, and the decimals of a fine amount (a fare; none where they are not used).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub symbol: &'static str,
    pub at: SymbolAt,
    pub thousands: char,
    pub decimal: char,
    pub places: usize,
}

/// The narrow no-break space Poland, Czechia, Hungary, Scandinavia and Ukraine group thousands with.
pub const NARROW: char = '\u{202F}';

impl Currency {
    pub const ALL: [Currency; 15] = [
        Currency::Eur,
        Currency::Gbp,
        Currency::Usd,
        Currency::Chf,
        Currency::Pln,
        Currency::Czk,
        Currency::Huf,
        Currency::Sek,
        Currency::Nok,
        Currency::Dkk,
        Currency::Ron,
        Currency::Try,
        Currency::Uah,
        Currency::Cad,
        Currency::Aud,
    ];

    /// The ISO 4217 code ("EUR").
    pub fn code(self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
            Currency::Gbp => "GBP",
            Currency::Usd => "USD",
            Currency::Chf => "CHF",
            Currency::Pln => "PLN",
            Currency::Czk => "CZK",
            Currency::Huf => "HUF",
            Currency::Sek => "SEK",
            Currency::Nok => "NOK",
            Currency::Dkk => "DKK",
            Currency::Ron => "RON",
            Currency::Try => "TRY",
            Currency::Uah => "UAH",
            Currency::Cad => "CAD",
            Currency::Aud => "AUD",
        }
    }

    /// The currency of a code ("gbp" too).
    pub fn of_code(code: &str) -> Option<Currency> {
        Currency::ALL.into_iter().find(|c| c.code().eq_ignore_ascii_case(code.trim()))
    }

    /// Its name in English (the pages translate it).
    pub fn name(self) -> &'static str {
        match self {
            Currency::Eur => "Euro",
            Currency::Gbp => "British pound",
            Currency::Usd => "US dollar",
            Currency::Chf => "Swiss franc",
            Currency::Pln => "Polish złoty",
            Currency::Czk => "Czech koruna",
            Currency::Huf => "Hungarian forint",
            Currency::Sek => "Swedish krona",
            Currency::Nok => "Norwegian krone",
            Currency::Dkk => "Danish krone",
            Currency::Ron => "Romanian leu",
            Currency::Try => "Turkish lira",
            Currency::Uah => "Ukrainian hryvnia",
            Currency::Cad => "Canadian dollar",
            Currency::Aud => "Australian dollar",
        }
    }

    /// What one euro is worth in it: a fixed rate, so that prices stay what the balancing made
    /// them. The euro reference rates of the ECB (as the Bundesbank republishes them) of
    /// 1 October 2026 for CZK, DKK, HUF, PLN, RON and SEK; of 1 July 2026 for GBP, USD and CHF;
    /// of 14 July 2026 (the Official Journal) for NOK, TRY, CAD and AUD. UAH is not among the
    /// ECB's rates: an approximate rate of mid-2026.
    pub fn rate(self) -> f64 {
        match self {
            Currency::Eur => 1.0,
            Currency::Gbp => 0.85973,
            Currency::Usd => 1.1383,
            Currency::Chf => 0.9234,
            Currency::Pln => 4.3735,
            Currency::Czk => 24.459,
            Currency::Huf => 367.18,
            Currency::Sek => 11.331,
            Currency::Nok => 11.08,
            Currency::Dkk => 7.4758,
            Currency::Ron => 5.283,
            Currency::Try => 53.65,
            Currency::Uah => 48.0,
            Currency::Cad => 1.61,
            Currency::Aud => 1.64,
        }
    }

    /// How its country writes it. The euro's is the English one: the pages write the euro as
    /// their language does (`words::style` of the API).
    pub fn look(self) -> Look {
        let l = |symbol, at, thousands, decimal, places| Look { symbol, at, thousands, decimal, places };
        use SymbolAt::*;
        match self {
            Currency::Eur => l("€", Before, ',', '.', 2),
            Currency::Gbp => l("£", Before, ',', '.', 2),
            Currency::Usd => l("$", Before, ',', '.', 2),
            Currency::Chf => l("CHF", BeforeSpaced, '’', '.', 2),
            Currency::Pln => l("zł", After, NARROW, ',', 2),
            Currency::Czk => l("Kč", After, NARROW, ',', 0),
            Currency::Huf => l("Ft", After, NARROW, ',', 0),
            Currency::Sek => l("kr", After, NARROW, ',', 2),
            Currency::Nok => l("kr", After, NARROW, ',', 2),
            Currency::Dkk => l("kr.", After, '.', ',', 2),
            Currency::Ron => l("lei", After, '.', ',', 2),
            Currency::Try => l("₺", Before, '.', ',', 2),
            Currency::Uah => l("₴", After, NARROW, ',', 2),
            Currency::Cad => l("C$", Before, ',', '.', 2),
            Currency::Aud => l("A$", Before, ',', '.', 2),
        }
    }

    /// Euro cents as an amount of this currency (whole units, with their fraction).
    pub fn units(self, cents: f64) -> f64 {
        cents / 100.0 * self.rate()
    }

    /// An amount of this currency as euro cents.
    pub fn cents(self, units: f64) -> Cents {
        if !units.is_finite() {
            return 0;
        }
        (units * 100.0 / self.rate()).round() as Cents
    }

    /// The step its fares are priced in, in its units: 5p, 5 cents, 10 rappen or groszy, a whole
    /// krona, ten forints (what a ticket machine of the country charges).
    pub fn fare_step(self) -> f64 {
        match self {
            Currency::Eur | Currency::Gbp | Currency::Usd | Currency::Cad | Currency::Aud => 0.05,
            Currency::Chf | Currency::Pln | Currency::Ron => 0.10,
            Currency::Czk | Currency::Sek | Currency::Nok | Currency::Dkk | Currency::Try | Currency::Uah => 1.0,
            Currency::Huf => 10.0,
        }
    }

    /// A fare (euro cents) in its units, at its step: what the ticket says (£2.40, never £2.39).
    /// A fare above nothing is at least one step.
    pub fn fare_units(self, cents: f64) -> f64 {
        let step = self.fare_step();
        if !cents.is_finite() || cents <= 0.0 {
            return 0.0;
        }
        ((self.units(cents) / step).round() * step).max(step)
    }

    /// A fare (euro cents) made the round price this currency asks, back in euro cents -
    /// fractional, exactly that price (the economy books the passengers' fares from it; a fare
    /// kept in a file is it rounded to whole cents, which shows back as the same price).
    pub fn round_fare(self, cents: f64) -> f64 {
        self.fare_units(cents) * 100.0 / self.rate()
    }
}

/// A number with `places` decimals, grouped as `look` groups it ("-1,234.50").
pub fn number(x: f64, places: usize, look: &Look) -> String {
    let x = if x.is_finite() { x } else { 0.0 };
    let text = format!("{:.*}", places, x.abs());
    let (whole, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut out = String::new();
    if x < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('-');
    }
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(look.thousands);
        }
        out.push(ch);
    }
    if !frac.is_empty() {
        out.push(look.decimal);
        out.push_str(frac);
    }
    out
}

/// An amount written with its symbol ("-£1,234", "CHF 12.50", "1 234 Ft"): whole units, or with
/// the currency's decimals when `fine`.
pub fn write(units: f64, fine: bool, look: &Look) -> String {
    let places = if fine { look.places } else { 0 };
    let units = if fine { units } else { units.round() };
    let n = number(units, places, look);
    let (sign, n) = match n.strip_prefix('-') {
        Some(rest) => ("-", rest.to_string()),
        None => ("", n),
    };
    let symbol = look.symbol;
    match look.at {
        SymbolAt::Before => format!("{sign}{symbol}{n}"),
        SymbolAt::BeforeSpaced => format!("{sign}{symbol} {n}"),
        SymbolAt::After => format!("{sign}{n} {symbol}"),
    }
}

/// An amount typed ("245000", "245.000", "€ 245,000", "1 234,50 zł", "CHF 1’234.50") in units of
/// the currency: its digits, and the last mark a decimal one when one or two digits follow it
/// (else every mark groups thousands). None without a digit.
pub fn parse(typed: &str) -> Option<f64> {
    let t: String = typed.chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
    let t = t.trim_matches([',', '.']);
    if !t.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let (whole, frac) = match t.rfind([',', '.']) {
        Some(p) if t.len() - p - 1 <= 2 => (&t[..p], &t[p + 1..]),
        _ => (t, ""),
    };
    let digits: String = whole.chars().filter(|c| c.is_ascii_digit()).collect();
    let x: f64 = format!("{}.{}", if digits.is_empty() { "0" } else { &digits }, if frac.is_empty() { "0" } else { frac }).parse().ok()?;
    Some(if typed.trim_start().starts_with(['-', '−']) { -x } else { x })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(c: Currency, cents: Cents, fine: bool) -> String {
        write(c.units(cents as f64), fine, &c.look())
    }

    #[test]
    fn each_currency_is_written_its_countrys_way() {
        // (1,000,000 euros at each rate)
        assert_eq!(shown(Currency::Gbp, 1_000_000_00, false), "£859,730");
        assert_eq!(shown(Currency::Usd, 1_000_000_00, false), "$1,138,300");
        assert_eq!(shown(Currency::Chf, 1_000_000_00, false), "CHF 923’400");
        assert_eq!(shown(Currency::Pln, 1_000_000_00, false), "4\u{202F}373\u{202F}500 zł");
        assert_eq!(shown(Currency::Huf, 1_000_00, false), "367\u{202F}180 Ft");
        assert_eq!(shown(Currency::Dkk, 1_000_00, false), "7.476 kr.");
        assert_eq!(shown(Currency::Try, 1_000_00, false), "₺53.650");
        assert_eq!(shown(Currency::Cad, -500_00, false), "-C$805");
        // a fare with its decimals, none where they are not used
        assert_eq!(shown(Currency::Gbp, 2_50, true), "£2.15");
        assert_eq!(shown(Currency::Ron, 2_50, true), "13,21 lei");
        assert_eq!(shown(Currency::Huf, 2_50, true), "918 Ft");
        assert_eq!(shown(Currency::Czk, 2_50, true), "61 Kč");
        assert_eq!(shown(Currency::Eur, 1_234_50, true), "€1,234.50");
    }

    #[test]
    fn a_typed_amount_comes_back_as_it_was_shown() {
        for c in Currency::ALL {
            for cents in [100_00, 12_345_00, 245_000_00, 3_000_000_00] {
                let text = shown(c, cents, false);
                let back = c.cents(parse(&text).unwrap());
                assert_eq!(shown(c, back, false), text, "{text} in {}", c.code());
            }
        }
        assert_eq!(parse("245000"), Some(245_000.0));
        assert_eq!(parse("€ 245.000"), Some(245_000.0));
        assert_eq!(parse("1 234,50 zł"), Some(1_234.5));
        assert_eq!(parse("CHF 1’234.50"), Some(1_234.5));
        assert_eq!(parse("-12,5"), Some(-12.5));
        assert_eq!(parse("abc"), None);
        assert_eq!(Currency::Huf.cents(367.18), 1_00);
    }

    #[test]
    fn fares_are_round_prices_of_the_currency() {
        // £2.40 converted in and kept as whole euro cents shows as £2.40 again, never £2.39
        for pence in (150..=750).step_by(5) {
            let kept = Currency::Gbp.round_fare(Currency::Gbp.cents(pence as f64 / 100.0) as f64).round() as Cents;
            let back = Currency::Gbp.fare_units(kept as f64);
            assert!((back * 100.0 - pence as f64).abs() < 1e-6, "{pence} {back}");
            assert_eq!(write(back, true, &Currency::Gbp.look()), format!("£{}.{:02}", pence / 100, pence % 100));
        }
        // any amount comes out at a 5p step, a euro amount at 5 cents, a forint one at ten
        let p = Currency::Gbp.fare_units(279.0 * 1.013);
        assert!(((p * 20.0).round() - p * 20.0).abs() < 1e-9, "{p}");
        assert!((Currency::Eur.fare_units(243.0) - 2.45).abs() < 1e-9);
        assert!((Currency::Huf.fare_units(243.0) - 890.0).abs() < 1e-9);
        assert_eq!(Currency::Gbp.fare_units(0.0), 0.0);
        assert!((Currency::Gbp.fare_units(1.0) - 0.05).abs() < 1e-9);
    }

    #[test]
    fn codes_are_read_and_unknown_ones_are_euros() {
        assert_eq!(serde_json::to_string(&Currency::Gbp).unwrap(), "\"GBP\"");
        assert_eq!(serde_json::from_str::<Currency>("\"PLN\"").unwrap(), Currency::Pln);
        assert_eq!(serde_json::from_str::<Currency>("\"XYZ\"").unwrap(), Currency::Eur);
        assert_eq!(Currency::of_code("huf"), Some(Currency::Huf));
        assert_eq!(Currency::of_code("XYZ"), None);
    }
}
