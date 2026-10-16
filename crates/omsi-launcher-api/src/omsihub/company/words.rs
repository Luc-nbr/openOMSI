//! The bus company's words as the game-drawn launcher says them (`crates/omsi-launcher-ui/src/
//! company/mod.rs`): every text translated in the page's language from the game's tables (the
//! English text is the key), money, numbers and dates written as that language writes them.
//! The page hands its language with every command (`lang`); the views say their texts whole,
//! so that the page draws them as they come.

use omsi_launcher_lib::company::currency::{self as cur, Currency, Look, SymbolAt};
use omsi_launcher_lib::company::{self as co, Cents};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

type Table = Arc<HashMap<String, String>>;

thread_local! {
    /// The language of the command being answered on this thread, and its table.
    static LANG: RefCell<(String, Table)> = RefCell::new((String::new(), Arc::new(HashMap::new())));
}

/// The tables read so far, by language (`crate::i18n::table` hands a copy each time).
fn tables() -> &'static Mutex<HashMap<String, Table>> {
    static T: OnceLock<Mutex<HashMap<String, Table>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Answer in `lang` on this thread (an ISO code, "" or "en" for English).
pub fn set_language(lang: &str) {
    let lang = if lang == "en" { "" } else { lang };
    if LANG.with(|l| l.borrow().0 == lang) {
        return;
    }
    let table = if lang.is_empty() {
        Arc::new(HashMap::new())
    } else {
        let mut all = tables().lock().unwrap_or_else(|e| e.into_inner());
        all.entry(lang.to_string()).or_insert_with(|| Arc::new(crate::i18n::table(lang))).clone()
    };
    LANG.with(|l| *l.borrow_mut() = (lang.to_string(), table));
}

/// The language answered in ("" for English).
pub fn language() -> String {
    LANG.with(|l| l.borrow().0.clone())
}

/// A text in the language answered in (as it is where the tables lack it).
pub fn tr(text: &str) -> String {
    LANG.with(|l| l.borrow().1.get(text).cloned()).unwrap_or_else(|| text.to_string())
}

/// A text with its `%{name}`s filled after the lookup.
pub fn trf(text: &str, args: &[(&str, &str)]) -> String {
    let mut t = tr(text);
    for (k, v) in args {
        t = t.replace(&format!("%{{{k}}}"), v);
    }
    t
}

/// A text with its `%{n}` filled.
pub fn n_of(key: &str, n: usize) -> String {
    tr(key).replace("%{n}", &n.to_string())
}

/// A count in words: the text for one, else the text for `n` with its `%{n}` filled.
pub fn count_text(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        tr(one)
    } else {
        n_of(many, n)
    }
}

/// A message's words, its `%{name}`s filled (the inbox's and the feed's own arguments).
pub fn filled(text: &str, args: &[(String, String)]) -> String {
    let mut t = tr(text);
    for (k, v) in args {
        t = t.replace(&format!("%{{{k}}}"), v);
    }
    t
}

// --- money and numbers ----------------------------------------------------------------------

thread_local! {
    /// The currency of the company the command is answered on (`set_currency`).
    static CURRENCY: std::cell::Cell<Currency> = const { std::cell::Cell::new(Currency::Eur) };
}

/// Write and read money in `c` on this thread (the open company's: `company::with`).
pub fn set_currency(c: Currency) {
    CURRENCY.with(|x| x.set(c));
}

/// The currency money is written in.
pub fn currency() -> Currency {
    CURRENCY.with(|x| x.get())
}

/// How a language writes a number: its thousands separator, its decimal mark, and where the
/// euro sign goes (`company::NumberStyle` of the game-drawn launcher).
#[derive(Clone, Copy, Debug, PartialEq)]
struct NumberStyle {
    thousands: char,
    decimal: char,
    euro: SymbolAt,
}

/// The narrow no-break space French, Russian, Ukrainian and Polish group thousands with.
const NARROW: char = cur::NARROW;

fn style(lang: &str) -> NumberStyle {
    let (thousands, decimal, euro) = match lang {
        "" | "en" => (',', '.', SymbolAt::Before),
        "nl" => ('.', ',', SymbolAt::BeforeSpaced),
        "de" | "it" | "es" | "pt" | "pt-pt" | "tr" | "da" | "id" => ('.', ',', SymbolAt::After),
        _ => (NARROW, ',', SymbolAt::After),
    };
    NumberStyle { thousands, decimal, euro }
}

fn num_in(x: f64, places: usize, lang: &str) -> String {
    let st = style(lang);
    cur::number(x, places, &Look { thousands: st.thousands, decimal: st.decimal, ..Currency::Eur.look() })
}

/// How money is written in `c` on a page in `lang`: the euro as the language writes it (it is
/// at home in many), every other currency as its country does.
fn look_in(c: Currency, lang: &str) -> Look {
    let look = c.look();
    if c != Currency::Eur {
        return look;
    }
    let st = style(lang);
    Look { at: st.euro, thousands: st.thousands, decimal: st.decimal, ..look }
}

fn money_in(cents: f64, fine: bool, c: Currency, lang: &str) -> String {
    cur::write(c.units(cents), fine, &look_in(c, lang))
}

fn eur_in(c: Cents, lang: &str) -> String {
    money_in(c as f64, false, Currency::Eur, lang)
}

/// Money in whole units of the company's currency, grouped as it is written ("€1,234",
/// "£1,061", "453 125 Ft"; the name is of before the currencies).
pub fn eur(c: Cents) -> String {
    money_in(c as f64, false, currency(), &language())
}

/// Money with its decimals (a fare, a price per kilometre; none in a currency without them).
pub fn eur_cents(c: f64) -> String {
    money_in(c, true, currency(), &language())
}

/// A fare with its decimals at the step the company's currency prices fares in (£2.40, never
/// £2.39: `currency::Currency::fare_units`).
pub fn fare_text(c: f64) -> String {
    let cur = currency();
    cur::write(cur.fare_units(c), true, &look_in(cur, &language()))
}

/// Euro cents in units of the company's currency (a chart's scale, "+12k").
pub fn units(c: Cents) -> f64 {
    currency().units(c as f64)
}

/// An amount typed in the company's currency ("245000", "£ 245,000", "1 234,50 zł") as euro
/// cents; None without a digit.
pub fn typed_money(s: &str) -> Option<Cents> {
    cur::parse(s).map(|x| currency().cents(x))
}

/// An amount in units of the company's currency (a number field's) as euro cents.
pub fn money_cents(units: f64) -> Cents {
    currency().cents(units)
}

/// How the pages write the company's money themselves (`omsihub/money.js`): its code, its rate
/// to the euro, its symbol and where it goes, its marks, a fine amount's decimals.
pub fn money_json() -> serde_json::Value {
    let c = currency();
    let l = look_in(c, &language());
    let at = match l.at {
        SymbolAt::Before => "before",
        SymbolAt::BeforeSpaced => "spaced",
        SymbolAt::After => "after",
    };
    serde_json::json!({ "code": c.code(), "rate": c.rate(), "symbol": l.symbol, "at": at, "thousands": l.thousands.to_string(), "decimal": l.decimal.to_string(), "places": l.places, "fare_step": c.fare_step() })
}

/// A whole number grouped like money (kilometres, passengers).
pub fn grouped(n: f64) -> String {
    num_in(n.round(), 0, &language())
}

/// A number with `places` decimals as the language writes it.
pub fn num(x: f64, places: usize) -> String {
    num_in(x, places, &language())
}

/// An amount with its sign and its tone: what comes in green "+€894", what goes out red
/// "−€11,175" (`kit::signed`).
pub fn signed(c: Cents) -> (String, &'static str) {
    if c > 0 {
        (format!("+{}", eur(c)), "ok")
    } else if c < 0 {
        (format!("−{}", eur(-c)), "danger")
    } else {
        (eur(0), "dim")
    }
}

/// Money short, in the company's currency as it is written: "£1.2M", "€8.4k", "−£950"; `sign`:
/// a "+" before what is not below nought (the chooser's cards, Luc 2026-10-16).
pub fn compact(c: Cents, sign: bool) -> String {
    let lang = language();
    let look = look_in(currency(), &lang);
    let u = currency().units(c as f64);
    let a = u.abs();
    let (x, places, unit) = if a >= 999_500.0 {
        (a / 1_000_000.0, 1, "M")
    } else if a >= 99_950.0 {
        (a / 1_000.0, 0, "k")
    } else if a >= 1_000.0 {
        (a / 1_000.0, 1, "k")
    } else {
        (a, 0, "")
    };
    let mut n = cur::number(x, places, &look);
    if places > 0 {
        let zero = format!("{}0", look.decimal);
        if let Some(whole) = n.strip_suffix(&zero) {
            n = whole.to_string();
        }
    }
    let n = format!("{n}{unit}");
    let s = if u < 0.0 && a >= 0.5 { "−" } else if sign { "+" } else { "" };
    match look.at {
        SymbolAt::Before => format!("{s}{}{n}", look.symbol),
        SymbolAt::BeforeSpaced => format!("{s}{} {n}", look.symbol),
        SymbolAt::After => format!("{s}{n} {}", look.symbol),
    }
}

// --- dates --------------------------------------------------------------------------------------

pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
pub const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Tue 5 Mar 1989".
pub fn day_label(date: &str) -> String {
    let Some(d) = co::dates::parse(date) else { return date.to_string() };
    let (y, m, day) = co::dates::civil_from_days(d);
    format!("{} {} {} {}", tr(WEEKDAYS[co::dates::weekday(d) as usize]), day, tr(MONTHS[(m as usize).clamp(1, 12) - 1]), y)
}

/// "Mar 1989" of a `YYYY-MM`.
pub fn month_label(month: &str) -> String {
    let y = month.get(..4).unwrap_or("");
    let m: usize = month.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    format!("{} {}", tr(MONTHS[m.clamp(1, 12) - 1]), y)
}

/// "12 Oct": a date short.
pub fn short_date(date: &str) -> String {
    let Some(d) = co::dates::parse(date) else { return date.to_string() };
    let (_, m, day) = co::dates::civil_from_days(d);
    format!("{} {}", day, tr(MONTHS[(m as usize).clamp(1, 12) - 1]))
}

/// A span of minutes as the pages write it, its two largest parts: "2 d 4 h", "31 d",
/// "4 h 10 min", "3 h", "12 min".
pub fn span_text(minutes: i64) -> String {
    let (d, h, m) = co::clock::span(minutes);
    let t = match (d, h, m) {
        (0, 0, _) => tr("%{m} min"),
        (0, _, 0) => tr("%{h} h"),
        (0, _, _) => tr("%{h} h %{m} min"),
        (_, 0, _) => tr("%{d} d"),
        _ => tr("%{d} d %{h} h"),
    };
    t.replace("%{d}", &d.to_string()).replace("%{h}", &h.to_string()).replace("%{m}", &m.to_string())
}

/// The colour of a 0-100 condition or satisfaction (`company::grade`).
pub fn grade(v: f64) -> &'static str {
    if v >= 65.0 {
        "ok"
    } else if v >= 40.0 {
        "warn"
    } else {
        "danger"
    }
}

/// What a share's colour says: all of it green, part amber, none red; faint when it does not
/// count (`kit::share_colour`).
pub fn share_tone(frac: f64, idle: bool) -> &'static str {
    if idle {
        "faint"
    } else if frac >= 0.999 {
        "ok"
    } else if frac > 0.0 {
        "warn"
    } else {
        "danger"
    }
}

/// Today's date as the computer has it, in its own time zone.
pub fn today() -> String {
    if let Some((y, m, d, _, _)) = omsi_launcher_lib::local_now() {
        return format!("{y:04}-{m:02}-{d:02}");
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    co::dates::fmt((secs / 86_400) as i64)
}

/// A map's name where the company has none of its own (`state::short_map`).
pub fn short_map(m: &str) -> String {
    m.trim_start_matches("maps/").trim_end_matches("/global.cfg").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_grouped_as_the_language_groups_it() {
        assert_eq!(eur_in(123_456_789, ""), "€1,234,568");
        assert_eq!(eur_in(-5_000_00, ""), "-€5,000");
        assert_eq!(eur_in(123_456_789, "nl"), "€ 1.234.568");
        assert_eq!(eur_in(123_456_789, "de"), "1.234.568 €");
        assert_eq!(eur_in(123_456_789, "fr"), "1\u{202F}234\u{202F}568 €");
        assert_eq!(num_in(4.5, 1, "nl"), "4,5");
        assert_eq!(num_in(-0.04, 1, "nl"), "0,0");
    }

    #[test]
    fn a_text_is_said_in_the_language_asked() {
        set_language("nl");
        assert_eq!(language(), "nl");
        assert_ne!(tr("Simulate to tomorrow"), "Simulate to tomorrow");
        assert_eq!(eur(1_234_00), "€ 1.234");
        assert!(day_label("1989-05-30").ends_with("1989"));
        set_language("en");
        assert_eq!(tr("Simulate to tomorrow"), "Simulate to tomorrow");
        assert_eq!(day_label("1989-05-30"), "Tue 30 May 1989");
        assert_eq!(span_text(2 * 1440 + 4 * 60 + 7), "2 d 4 h");
        assert_eq!(short_map("maps/Bad_Huegelsdorf_2020/global.cfg"), "Bad_Huegelsdorf_2020");
    }

    #[test]
    fn money_is_written_in_the_companys_currency_whatever_the_language() {
        set_language("de");
        set_currency(Currency::Gbp);
        assert_eq!(eur(1_000_000_00), "£859,730");
        assert_eq!(eur_cents(2_50.0), "£2.15");
        assert_eq!(signed(-1_000_00).0, "−£860");
        set_currency(Currency::Huf);
        assert_eq!(eur(1_000_00), "367\u{202F}180 Ft");
        assert_eq!(eur_cents(2_50.0), "918 Ft");
        // (typed in forints, kept in euro cents)
        assert_eq!(typed_money("367 180 Ft"), Some(1_000_00));
        assert_eq!(money_cents(36_718.0), 100_00);
        assert_eq!(money_json()["symbol"], "Ft");
        // the euro as the language writes it
        set_currency(Currency::Eur);
        assert_eq!(eur(1_234_00), "1.234 €");
        assert_eq!(typed_money("1.234 €"), Some(1_234_00));
        set_language("en");
        assert_eq!(eur(1_234_00), "€1,234");
    }

    /// Every amount the pages show goes through `eur`, `eur_cents` and `money.js`: no euro sign
    /// of its own in the API's page code, nor in the pages' scripts (the tests and comments
    /// aside).
    #[test]
    fn no_page_writes_a_euro_sign_of_its_own() {
        fn files(dir: &std::path::Path, ext: &str, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    files(&p, ext, out);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
                    out.push(p);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = Vec::new();
        let mut rs = Vec::new();
        files(&root.join("src"), "rs", &mut rs);
        assert!(rs.len() > 20, "no sources read");
        for p in rs {
            let text = std::fs::read_to_string(&p).unwrap_or_default().replace("\r\n", "\n");
            // (a file's tests are at its end)
            let code = text.split("#[cfg(test)]").next().unwrap_or("");
            for (i, line) in code.lines().enumerate() {
                let t = line.trim_start();
                if !t.starts_with("//") && t.contains('€') {
                    found.push(format!("{}:{}", p.display(), i + 1));
                }
            }
        }
        let mut js = Vec::new();
        files(&root.join("../../launcher/ui/js/omsihub"), "js", &mut js);
        for p in js {
            // (the euro's look before the company's is known: `money.js` itself)
            if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("money.js")) {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap_or_default().replace("\r\n", "\n");
            for (i, line) in text.lines().enumerate() {
                let t = line.trim_start();
                if !(t.starts_with("//") || t.starts_with('*') || t.starts_with("/*")) && t.contains('€') {
                    found.push(format!("{}:{}", p.display(), i + 1));
                }
            }
        }
        assert!(found.is_empty(), "a euro sign of its own (write money with `eur`, `eur_cents` or `money.js`): {found:?}");
    }
}
