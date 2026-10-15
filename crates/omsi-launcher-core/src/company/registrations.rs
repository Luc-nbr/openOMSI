//! Registrations and the age of a bus (Luc, 2026-10-15: "make an age range, so the finance
//! section is based on that"). An OMSI bus names its plates in a `[registration_list]` file
//! (`.org`, one plate a line, beside the `[number]` list of fleet numbers). A British plate tells
//! when the bus was first registered:
//!
//! - the current system (since September 2001), `AB 26 CDE`: a local memory tag, the age
//!   identifier, three random letters. The identifier is the year from March to August (26:
//!   March-August 2026) and the year plus 50 from September to February (76: September 2026 -
//!   February 2027);
//! - the prefix system (August 1983 - August 2001), `A123 BCD`: a year letter first;
//! - the suffix system (1963 - July 1983), `ABC 123A`: a year letter last.
//!
//! A European plate (and a list of plain numbers) says nothing of the age: there the model's
//! era (a year in its name) or the player's own word counts (`AgeFrom`).

use super::dates;
use super::market::MarketBus;
use super::model::Company;
use super::rng::Rng;
use super::vehicles::Region;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Where a bus's date of first registration (`Vehicle::built`) comes from.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgeFrom {
    /// Drawn by the market (and every bus of before).
    #[default]
    Generated,
    /// Registered new by the company.
    New,
    /// Read from its British plate.
    Plate,
    /// After the model's era (a year in its name).
    Model,
    /// Set by the player.
    Manual,
}

/// A British plate's system.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum System {
    Current,
    Prefix,
    Suffix,
}

/// The months a plate's age mark stands for (first and last month, both included).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Period {
    pub system: System,
    /// The age identifier ("65") or year letter ("A").
    pub mark: String,
    pub from: (i32, u32),
    pub to: (i32, u32),
}

impl Period {
    /// Its first day ("2015-09-01").
    pub fn first_day(&self) -> String {
        format!("{:04}-{:02}-01", self.from.0, self.from.1)
    }

    /// Its first month ("2015-09").
    pub fn month(&self) -> String {
        format!("{:04}-{:02}", self.from.0, self.from.1)
    }

    /// A day in it, `frac` of the way (0..1).
    pub fn day_at(&self, frac: f64) -> String {
        let a = dates::days_from_civil(self.from.0, self.from.1, 1);
        let b = dates::days_from_civil(self.to.0, self.to.1, dates::days_in_month(self.to.0, self.to.1));
        dates::fmt(a + ((b - a) as f64 * frac.clamp(0.0, 1.0)).floor() as i64)
    }
}

/// The year letters of the prefix system from A (August 1983) to S (August 1998 - February 1999).
const PREFIX_LETTERS: &str = "ABCDEFGHJKLMNPRS";
/// The suffix system's letters from F (August 1967) to Y (August 1982 - July 1983).
const SUFFIX_LETTERS: &str = "FGHJKLMNPRSTVWXY";

fn letters(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase())
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// When a British plate was first registered (None: not a British plate with an age, as a
/// European plate, a plain number or a Q plate).
pub fn decode_uk(plate: &str) -> Option<Period> {
    let s: String = plate.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    let n = s.len();
    // the current system: AB12 CDE
    if n == 7 && letters(&s[..2]) && digits(&s[2..4]) && letters(&s[4..]) {
        let d: i32 = s[2..4].parse().ok()?;
        let mark = s[2..4].to_string();
        return match d {
            2..=50 => Some(Period { system: System::Current, mark, from: (2000 + d, 3), to: (2000 + d, 8) }),
            51..=99 => Some(Period { system: System::Current, mark, from: (1950 + d, 9), to: (1951 + d, 2) }),
            _ => None,
        };
    }
    // the prefix system: A123 BCD
    if (5..=7).contains(&n) && letters(&s[..1]) && digits(&s[1..n - 3]) && letters(&s[n - 3..]) {
        let c = s.chars().next()?;
        let (from, to) = match c {
            'T' => ((1999, 3), (1999, 8)),
            'V' => ((1999, 9), (2000, 2)),
            'W' => ((2000, 3), (2000, 8)),
            'X' => ((2000, 9), (2001, 2)),
            'Y' => ((2001, 3), (2001, 8)),
            'S' => ((1998, 8), (1999, 2)),
            _ => {
                let k = PREFIX_LETTERS.find(c)? as i32;
                ((1983 + k, 8), (1984 + k, 7))
            }
        };
        return Some(Period { system: System::Prefix, mark: c.to_string(), from, to });
    }
    // the suffix system: ABC 123A
    if (5..=7).contains(&n) && letters(&s[..3]) && digits(&s[3..n - 1]) && letters(&s[n - 1..]) {
        let c = s.chars().last()?;
        let (from, to) = match c {
            'A' => ((1963, 1), (1963, 12)),
            'B' => ((1964, 1), (1964, 12)),
            'C' => ((1965, 1), (1965, 12)),
            'D' => ((1966, 1), (1966, 12)),
            'E' => ((1967, 1), (1967, 7)),
            _ => {
                let k = SUFFIX_LETTERS.find(c)? as i32;
                ((1967 + k, 8), (1968 + k, 7))
            }
        };
        return Some(Period { system: System::Suffix, mark: c.to_string(), from, to });
    }
    None
}

/// The plates of a list that tell their age, with it.
pub fn dated(list: &[String]) -> Vec<(String, Period)> {
    list.iter().filter_map(|p| decode_uk(p).map(|d| (p.trim().to_string(), d))).collect()
}

/// The memory tag most of a list's current plates carry ("CN").
pub fn memory_tag(list: &[String]) -> Option<String> {
    let mut count: HashMap<String, usize> = HashMap::new();
    for (p, d) in dated(list) {
        if d.system == System::Current {
            let s: String = p.chars().filter(|c| c.is_ascii_alphanumeric()).take(2).collect::<String>().to_ascii_uppercase();
            *count.entry(s).or_default() += 1;
        }
    }
    count.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|x| x.0)
}

/// The current system's age identifier of a date ("26" from March 2026, "76" from September).
pub fn age_mark(date: &str) -> Option<String> {
    let (y, m, _) = dates::civil_from_days(dates::parse(date)?);
    let n = match m {
        3..=8 => y - 2000,
        9..=12 => y - 2000 + 50,
        _ => y - 1 - 2000 + 50,
    };
    (2..=99).contains(&n).then(|| format!("{n:02}"))
}

/// The letters a British plate's random part draws from (no I, Q).
const RANDOM: &[u8] = b"ABCDEFGHJKLMNOPRSTUVWXYZ";

fn random_letters(rng: &mut Rng, n: usize) -> String {
    (0..n).map(|_| *rng.pick(RANDOM).unwrap_or(&b'A') as char).collect()
}

/// A British plate first registered on `date`: the current system's with `tag` from September
/// 2001, the prefix system's before, the suffix system's before August 1983.
pub fn uk_plate(tag: &str, date: &str, rng: &mut Rng) -> String {
    let tag: String = tag.chars().filter(|c| c.is_ascii_alphabetic()).take(2).collect::<String>().to_ascii_uppercase();
    let tag = if tag.len() == 2 { tag } else { "LX".to_string() };
    if let Some(mark) = age_mark(date).filter(|_| date >= "2001-09-01") {
        return format!("{tag}{mark} {}", random_letters(rng, 3));
    }
    let number = rng.int(1, 999);
    let (y, m, _) = dates::civil_from_days(dates::parse(date).unwrap_or(0));
    if (y, m) >= (1983, 8) {
        let letter = [('Y', (2001, 3)), ('X', (2000, 9)), ('W', (2000, 3)), ('V', (1999, 9)), ('T', (1999, 3)), ('S', (1998, 8))]
            .iter()
            .find(|(_, from)| (y, m) >= *from)
            .map(|x| x.0)
            .unwrap_or_else(|| PREFIX_LETTERS.as_bytes()[(if m >= 8 { y } else { y - 1 } - 1983).clamp(0, 15) as usize] as char);
        return format!("{letter}{number} {}", random_letters(rng, 3));
    }
    let letter = match (y, m) {
        (..=1963, _) => 'A',
        (1964, _) => 'B',
        (1965, _) => 'C',
        (1966, _) => 'D',
        (1967, ..=7) => 'E',
        _ => SUFFIX_LETTERS.as_bytes()[(if m >= 8 { y } else { y - 1 } - 1967).clamp(0, 15) as usize] as char,
    };
    format!("{} {number}{letter}", random_letters(rng, 3))
}

/// A year in a model's names: its era ("Gelenkbus (1999)", "Citaro 2015").
pub fn era_of(texts: &[&str], this_year: i32) -> Option<i32> {
    texts.iter().flat_map(|t| t.split(|c: char| !c.is_ascii_digit())).filter(|w| w.len() == 4).filter_map(|w| w.parse::<i32>().ok()).find(|y| (1950..=this_year + 1).contains(y))
}

/// The plates a bus file's `[registration_list]` names (empty: none, or not readable). Read once
/// a file.
pub fn list_of(file: &str) -> Arc<Vec<String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Vec<String>>>>> = OnceLock::new();
    let key = super::vehicles::key_of(file);
    if let Some(v) = CACHE.get_or_init(Default::default).lock().ok().and_then(|c| c.get(&key).cloned()) {
        return v;
    }
    let out = Arc::new(read_list(file));
    if let Ok(mut c) = CACHE.get_or_init(Default::default).lock() {
        c.insert(key, out.clone());
    }
    out
}

/// A model's plates when it is a British model (`list_of`; a European one's say nothing).
pub fn list_for(bus: &MarketBus) -> Arc<Vec<String>> {
    if bus.region == Region::Uk {
        list_of(&bus.file)
    } else {
        Arc::new(Vec::new())
    }
}

fn read_list(file: &str) -> Vec<String> {
    let Ok(path) = crate::resolve_content(file) else { return Vec::new() };
    let Ok(v) = omsi_vehicle::Vehicle::load(&path) else { return Vec::new() };
    let Some((list, _, _)) = v.registration_list.as_ref() else { return Vec::new() };
    let Ok(f) = omsi_cfg::CfgFile::read(&omsi_cfg::resolve_path(v.dir(), list)) else { return Vec::new() };
    f.lines.iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

/// What a bus's plate and first registration are, as the market gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct Registered {
    /// Empty: the company's own plate (`Company::plate_for`).
    pub plate: String,
    pub built: String,
    pub from: AgeFrom,
}

fn plates_taken(c: &Company) -> Vec<String> {
    let norm = |p: &str| p.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    c.fleet.iter().map(|v| norm(&v.plate)).collect()
}

fn free(c: &Company, plate: &str) -> bool {
    let p: String = plate.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    !plates_taken(c).contains(&p)
}

/// A used bus of a model for the market on `today` (a day number), its age drawn as `built`:
/// a British model with a dated list takes one of its plates of an age between `ages` (years,
/// a few more allowed) and is as old as it says; another British model gets a plate of its
/// drawn age; a European one keeps its plate the company gives it, no older than its model's
/// era. `list`: the model's plates (`list_of`).
pub fn used(c: &Company, bus: &MarketBus, list: &[String], today: i64, built: String, ages: (f64, f64), rng: &mut Rng) -> Registered {
    let date = dates::fmt(today);
    let this_year = dates::civil_from_days(today).0;
    if bus.region == Region::Uk {
        let fits: Vec<(String, Period)> = dated(list)
            .into_iter()
            .filter(|(p, d)| {
                let age = dates::years_between(&d.first_day(), &date);
                dates::parse(&d.day_at(1.0)).is_some_and(|end| end < today - 120) && age >= ages.0 - 1.0 && age <= ages.1 + 5.0 && free(c, p)
            })
            .collect();
        if let Some((plate, d)) = rng.pick(&fits).cloned() {
            let built = d.day_at(rng.f64());
            return Registered { plate, built, from: AgeFrom::Plate };
        }
        let tag = memory_tag(list).unwrap_or_else(|| c.short.clone());
        let plate = uk_plate(&tag, &built, rng);
        return Registered { plate, built, from: AgeFrom::Generated };
    }
    // (a model of 2015 is not sold used as a bus of 2012)
    if let Some(era) = era_of(&[&bus.name, &bus.file], this_year) {
        let y = dates::civil_from_days(dates::parse(&built).unwrap_or(today)).0;
        if y < era {
            let latest = (today - 200).max(dates::days_from_civil(era, 1, 1));
            let first = dates::days_from_civil(era, 1, 1).min(latest);
            let day = first + ((latest - first) as f64 * rng.f64() * 0.5) as i64;
            return Registered { plate: String::new(), built: dates::fmt(day), from: AgeFrom::Model };
        }
    }
    Registered { plate: String::new(), built, from: AgeFrom::Generated }
}

/// The plate a bus gets that joins the fleet without one of its own: a British model's of its
/// date of first registration (its list's memory tag), else the company's (`plate_for`).
pub fn plate_for(c: &Company, bus: &MarketBus, number: &str, built: &str, vehicle: u32) -> String {
    if bus.region != Region::Uk {
        return c.plate_for(number);
    }
    let list = list_for(bus);
    let tag = memory_tag(&list).unwrap_or_else(|| c.short.clone());
    let mut rng = Rng::of(&[&c.id, "plate"], vehicle as i64);
    for _ in 0..20 {
        let p = uk_plate(&tag, built, &mut rng);
        if free(c, &p) {
            return p;
        }
    }
    c.plate_for(number)
}

/// A year (or "YYYY-MM") the player typed as a date of first registration: the first of that
/// month (the middle of the year for a year alone), never after `today` nor before 1950.
pub fn typed_date(typed: &str, today: &str) -> Option<String> {
    let t = typed.trim();
    let (y, m) = match t.split_once(['-', '/', '.']) {
        Some((a, b)) => (a.trim().parse::<i32>().ok()?, b.trim().parse::<u32>().ok()?),
        None => (t.parse::<i32>().ok()?, 7),
    };
    if !(1950..=2100).contains(&y) || !(1..=12).contains(&m) {
        return None;
    }
    let date = format!("{y:04}-{m:02}-01");
    Some(if date.as_str() > today { today.to_string() } else { date })
}

/// What a bus's age is said with: its plate's period when the plate tells it (and the player did
/// not set it himself), else the month of `built`.
pub fn period_of(plate: &str, region: Option<Region>, from: AgeFrom) -> Option<Period> {
    if from == AgeFrom::Manual || region != Some(Region::Uk) {
        return None;
    }
    decode_uk(plate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(plate: &str) -> Option<((i32, u32), (i32, u32))> {
        decode_uk(plate).map(|d| (d.from, d.to))
    }

    #[test]
    fn current_plates_tell_their_half_year() {
        // 17: March-August 2017; 65: September 2015 - February 2016
        assert_eq!(p("AD17 SUL"), Some(((2017, 3), (2017, 8))));
        assert_eq!(p("CN65 AAE"), Some(((2015, 9), (2016, 2))));
        assert_eq!(p("AB26CDE"), Some(((2026, 3), (2026, 8))));
        assert_eq!(p("ab 76 cde"), Some(((2026, 9), (2027, 2))));
        assert_eq!(p("XY75 ABC"), Some(((2025, 9), (2026, 2))));
        assert_eq!(decode_uk("CN65 AAE").unwrap().first_day(), "2015-09-01");
        // and the other way round
        assert_eq!(age_mark("2026-03-01").as_deref(), Some("26"));
        assert_eq!(age_mark("2026-10-15").as_deref(), Some("76"));
        assert_eq!(age_mark("2027-02-28").as_deref(), Some("76"));
        let mut rng = Rng::new(1);
        let made = uk_plate("CN", "2026-10-15", &mut rng);
        assert!(made.starts_with("CN76 ") && made.len() == 8, "{made}");
        assert_eq!(p(&made), Some(((2026, 9), (2027, 2))));
    }

    #[test]
    fn prefix_and_suffix_plates_tell_their_year() {
        assert_eq!(p("A123 BCD"), Some(((1983, 8), (1984, 7))));
        assert_eq!(p("R45 XYZ"), Some(((1997, 8), (1998, 7))));
        assert_eq!(p("S1 ABC"), Some(((1998, 8), (1999, 2))));
        assert_eq!(p("V999 ABC"), Some(((1999, 9), (2000, 2))));
        assert_eq!(p("ABC 123A"), Some(((1963, 1), (1963, 12))));
        assert_eq!(p("XYZ 12F"), Some(((1967, 8), (1968, 7))));
        assert_eq!(p("JKL 9Y"), Some(((1982, 8), (1983, 7))));
        // a Q plate has no age
        assert_eq!(p("Q123 ABC"), None);
        // made the other way round
        let mut rng = Rng::new(2);
        assert_eq!(p(&uk_plate("LX", "1990-05-01", &mut rng)).map(|x| x.0), Some((1989, 8)));
        assert_eq!(p(&uk_plate("LX", "1975-09-01", &mut rng)).map(|x| x.0), Some((1975, 8)));
    }

    #[test]
    fn european_and_numeric_lists_have_no_age() {
        let list: Vec<String> = ["883", "881", "B-V 1234", "HH-AB 123", "M AB 1234"].iter().map(|s| s.to_string()).collect();
        assert!(dated(&list).is_empty());
        assert_eq!(memory_tag(&list), None);
        let uk: Vec<String> = ["AD17 SUL", "AW17 SUL", "CN65 AAE", "CN65 AAF", "CN65 AAJ"].iter().map(|s| s.to_string()).collect();
        assert_eq!(dated(&uk).len(), 5);
        assert_eq!(memory_tag(&uk).as_deref(), Some("CN"));
        assert_eq!(era_of(&["Hamburger Stadtbus Gelenkbus (1999)"], 2026), Some(1999));
        assert_eq!(era_of(&["MAN NL 202", "Vehicles/MAN_NL/MAN_NL.bus"], 2026), None);
    }

    #[test]
    fn a_typed_year_is_a_date() {
        assert_eq!(typed_date("2015", "2026-10-15").as_deref(), Some("2015-07-01"));
        assert_eq!(typed_date("2015-09", "2026-10-15").as_deref(), Some("2015-09-01"));
        assert_eq!(typed_date("2030", "2026-10-15").as_deref(), Some("2026-10-15"));
        assert_eq!(typed_date("abc", "2026-10-15"), None);
        assert_eq!(typed_date("1900", "2026-10-15"), None);
    }
}
