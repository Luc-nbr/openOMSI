//! How the line editor writes money, dates and bus names, as the bus company's pages and the
//! bus step write them: whole euros grouped as the interface's language groups them ("€1,234",
//! "€ 1.234", "1.234 €", "1 234 €"), a day as "Tue 5 Mar 1989", bus names in the order a
//! person sorts them (2 before 10).

use omsi_launcher_lib::company::{self as co, Cents};
use std::cmp::Ordering;
use std::path::Path;

/// How the interface's language writes a number: its thousands separator, its decimal mark,
/// and where the euro sign goes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NumberStyle {
    thousands: char,
    decimal: char,
    /// "€17,968" (en), "€ 17.968" (nl) or "17.968 €".
    euro: EuroAt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EuroAt {
    Before,
    BeforeSpaced,
    After,
}

/// The narrow no-break space French, Russian, Ukrainian and Polish group thousands with.
const NARROW: char = '\u{202F}';

fn style(lang: &str) -> NumberStyle {
    let (thousands, decimal, euro) = match lang {
        "" | "en" => (',', '.', EuroAt::Before),
        "nl" => ('.', ',', EuroAt::BeforeSpaced),
        "de" | "it" | "es" | "pt" | "pt-pt" | "tr" | "da" | "id" => ('.', ',', EuroAt::After),
        _ => (NARROW, ',', EuroAt::After),
    };
    NumberStyle { thousands, decimal, euro }
}

/// `x` rounded to `places` decimals, its thousands grouped, as `lang` writes it.
fn num_in(x: f64, places: usize, lang: &str) -> String {
    let st = style(lang);
    let x = if x.is_finite() { x } else { 0.0 };
    let text = format!("{:.*}", places, x.abs());
    let (whole, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut out = String::new();
    // (-0 is 0)
    if x < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('-');
    }
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(st.thousands);
        }
        out.push(ch);
    }
    if !frac.is_empty() {
        out.push(st.decimal);
        out.push_str(frac);
    }
    out
}

/// An amount with its euro sign where `lang` puts it (the minus before all).
fn with_euro(n: String, lang: &str) -> String {
    let (sign, n) = match n.strip_prefix('-') {
        Some(rest) => ("-", rest.to_string()),
        None => ("", n),
    };
    match style(lang).euro {
        EuroAt::Before => format!("{sign}€{n}"),
        EuroAt::BeforeSpaced => format!("{sign}€ {n}"),
        EuroAt::After => format!("{sign}{n} €"),
    }
}

fn eur_in(c: Cents, lang: &str) -> String {
    with_euro(num_in((c as f64 / 100.0).round(), 0, lang), lang)
}

/// Whole euros, grouped as the language groups them ("€1,234,567", "€ 1.234.567",
/// "1.234.567 €", "1 234 567 €").
pub(crate) fn eur(c: Cents) -> String {
    eur_in(c, &omsi_ui::i18n::language())
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Tue 5 Mar 1989".
pub(crate) fn day_label(date: &str) -> String {
    let Some(d) = co::dates::parse(date) else { return date.to_string() };
    let (y, m, day) = co::dates::civil_from_days(d);
    format!("{} {} {} {}", omsi_ui::tr(WEEKDAYS[co::dates::weekday(d) as usize]), day, omsi_ui::tr(MONTHS[(m as usize).clamp(1, 12) - 1]), y)
}

/// Names in the order a person sorts them: letters without regard to case, numbers by their
/// value (2 before 10).
pub(crate) fn name_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let x: String = std::iter::from_fn(|| a.next_if(|c| c.is_ascii_digit())).collect();
                let y: String = std::iter::from_fn(|| b.next_if(|c| c.is_ascii_digit())).collect();
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let o = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// The model and the version of a bus: `[friendlyname]`'s second line split at its first
/// " - "; without one, the version is the bus's own paint, or its file's name.
pub(crate) fn split_type(type_name: &str, file: &str, default_paint: &str) -> (String, String) {
    use omsi_launcher_lib::{display_bus_name, vehicle_type_label};
    let label = vehicle_type_label(type_name, Path::new(file));
    let parts: Vec<&str> = label.split(" - ").map(str::trim).filter(|s| !s.is_empty()).collect();
    let model = parts.first().map(|s| s.to_string()).unwrap_or_else(|| label.clone());
    let version = if parts.len() > 1 {
        parts[1..].join(" · ")
    } else if !default_paint.trim().is_empty() {
        display_bus_name(default_paint)
    } else {
        display_bus_name(&Path::new(file).file_stem().unwrap_or_default().to_string_lossy())
    };
    (model, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_are_written_as_the_language_writes_them() {
        assert_eq!(eur_in(123_456_789, "en"), "€1,234,568");
        assert_eq!(eur_in(123_456_789, "nl"), "€ 1.234.568");
        assert_eq!(eur_in(-50_000, "de"), "-500 €");
        assert_eq!(eur_in(123_456_700, "fr"), format!("1{NARROW}234{NARROW}567 €"));
    }

    #[test]
    fn names_are_sorted_as_a_person_sorts_them() {
        let mut v = vec!["Line 10", "line 2", "Line 1"];
        v.sort_by(|a, b| name_cmp(a, b));
        assert_eq!(v, ["Line 1", "line 2", "Line 10"]);
    }
}
