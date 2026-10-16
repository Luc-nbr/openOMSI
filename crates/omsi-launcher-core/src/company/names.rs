//! People's names by the country the company plays in (Luc: "many more diverse names for all
//! employees; at the start the player chooses the country, and then it has local names"). Each
//! country is a data file of its own under `names/` (UTF-8 JSON, the names as comma-separated
//! lists so they are easy to extend), embedded in the build: its groups of people as the
//! country really has them (a share each: the Dutch pool has names of Moroccan, Turkish,
//! Surinamese and Indonesian heritage too), their men's and women's first names and their
//! surnames. A name is drawn from one group, so first name and surname belong together; a
//! small group of several origins separates them with ";" in each of its lists, in the same
//! order, and a name is drawn from one of them.
//!
//! The local rules that are cheap to keep are kept: the feminine surname forms (Polish -ska,
//! Czech and Slovak -ová, Russian and Ukrainian -ova/-ська, the Greek genitive -ου), Spanish
//! and Portuguese double surnames, the Hungarian family name first, and the Dutch tussenvoegsels
//! ("van der") that a list sorts by the name after them (`sort_key`).

use super::rng::Rng;
use super::Company;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// The mixed pool: a company of before the countries on a map whose country is not known.
pub const INTERNATIONAL: &str = "intl";

/// Every country's file, in the order the countries are offered (International first).
const FILES: [&str; 28] = [
    include_str!("names/intl.json"),
    include_str!("names/nl.json"),
    include_str!("names/be.json"),
    include_str!("names/de.json"),
    include_str!("names/at.json"),
    include_str!("names/ch.json"),
    include_str!("names/gb.json"),
    include_str!("names/ie.json"),
    include_str!("names/fr.json"),
    include_str!("names/pl.json"),
    include_str!("names/cz.json"),
    include_str!("names/sk.json"),
    include_str!("names/hu.json"),
    include_str!("names/ro.json"),
    include_str!("names/ua.json"),
    include_str!("names/ru.json"),
    include_str!("names/it.json"),
    include_str!("names/es.json"),
    include_str!("names/pt.json"),
    include_str!("names/dk.json"),
    include_str!("names/se.json"),
    include_str!("names/no.json"),
    include_str!("names/fi.json"),
    include_str!("names/us.json"),
    include_str!("names/ca.json"),
    include_str!("names/au.json"),
    include_str!("names/tr.json"),
    include_str!("names/gr.json"),
];

/// A country's file as it is written.
#[derive(Deserialize)]
struct File {
    key: String,
    flag: String,
    name: String,
    #[serde(default)]
    family_first: bool,
    #[serde(default)]
    double_surname: f64,
    #[serde(default)]
    feminine: String,
    #[serde(default)]
    groups: Vec<GroupFile>,
    /// The countries a mixed pool draws from (International).
    #[serde(default)]
    mix: Vec<String>,
}

#[derive(Deserialize)]
struct GroupFile {
    share: f64,
    #[serde(default)]
    origin: String,
    /// The group's own feminine forms (Polish names in Germany keep -ska); empty: the country's.
    #[serde(default)]
    feminine: Option<String>,
    male: String,
    female: String,
    surnames: String,
}

/// A surname and its feminine form where it has one of its own (written as "Němec|Němcová").
#[derive(Clone, Debug, PartialEq)]
pub struct Surname {
    pub male: String,
    pub female: Option<String>,
}

/// A group of the people of a country: its names all together, and by origin (`parts`).
#[derive(Clone, Debug)]
pub struct Group {
    pub share: f64,
    pub origin: String,
    pub feminine: String,
    pub male: Vec<String>,
    pub female: Vec<String>,
    pub surnames: Vec<Surname>,
    pub parts: Vec<Part>,
}

/// One origin of a group: the names that go together.
#[derive(Clone, Debug)]
pub struct Part {
    pub male: Vec<String>,
    pub female: Vec<String>,
    pub surnames: Vec<Surname>,
}

/// A country's names.
#[derive(Clone, Debug)]
pub struct Pool {
    pub key: String,
    pub flag: String,
    /// Its name in English (translated when shown).
    pub name: String,
    /// The family name first (Hungary: "Nagy Péter").
    pub family_first: bool,
    /// How many carry two surnames (Spain: all of them).
    pub double_surname: f64,
    pub groups: Vec<Group>,
    pub mix: Vec<String>,
}

/// A comma-separated list (any line ends, blanks around the names), without empty entries.
fn list(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n").split([',', '\n']).map(|x| x.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|x| !x.is_empty()).collect()
}

fn surnames(text: &str) -> Vec<Surname> {
    list(text)
        .into_iter()
        .map(|x| match x.split_once('|') {
            Some((m, f)) => Surname { male: m.trim().to_string(), female: Some(f.trim().to_string()) },
            None => Surname { male: x, female: None },
        })
        .collect()
}

/// A group's origins: its lists cut at ";" (as many parts in each, else one of them all).
fn parts(g: &GroupFile) -> Vec<Part> {
    let (m, f, s): (Vec<&str>, Vec<&str>, Vec<&str>) = (g.male.split(';').collect(), g.female.split(';').collect(), g.surnames.split(';').collect());
    if m.len() > 1 && m.len() == f.len() && m.len() == s.len() {
        m.iter().zip(&f).zip(&s).map(|((m, f), s)| Part { male: list(m), female: list(f), surnames: surnames(s) }).collect()
    } else {
        vec![Part { male: list(&g.male.replace(';', ",")), female: list(&g.female.replace(';', ",")), surnames: surnames(&g.surnames.replace(';', ",")) }]
    }
}

/// A country's file read (`\r\n` as `\n`: a file saved on Windows reads the same).
pub fn parse(text: &str) -> Result<Pool, String> {
    let text = text.replace("\r\n", "\n");
    let f: File = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    let groups = f
        .groups
        .into_iter()
        .map(|g| {
            let parts = parts(&g);
            let all = |pick: fn(&Part) -> Vec<String>| parts.iter().flat_map(pick).collect::<Vec<_>>();
            Group {
                share: g.share,
                origin: g.origin,
                feminine: g.feminine.unwrap_or_else(|| f.feminine.clone()),
                male: all(|p| p.male.clone()),
                female: all(|p| p.female.clone()),
                surnames: parts.iter().flat_map(|p| p.surnames.clone()).collect(),
                parts,
            }
        })
        .collect();
    Ok(Pool { key: f.key.to_lowercase(), flag: f.flag, name: f.name, family_first: f.family_first, double_surname: f.double_surname, groups, mix: f.mix.iter().map(|k| k.to_lowercase()).collect() })
}

/// Every country, in the order they are offered (International first).
pub fn pools() -> &'static [Pool] {
    static POOLS: OnceLock<Vec<Pool>> = OnceLock::new();
    POOLS.get_or_init(|| FILES.iter().filter_map(|t| parse(t).ok()).collect())
}

/// A country by its key (any case); an unknown one: International.
pub fn pool(key: &str) -> &'static Pool {
    let all = pools();
    all.iter().find(|p| p.key.eq_ignore_ascii_case(key.trim())).or_else(|| all.iter().find(|p| p.key == INTERNATIONAL)).unwrap_or(&all[0])
}

/// Whether `key` is a country of ours.
pub fn known(key: &str) -> bool {
    pools().iter().any(|p| p.key.eq_ignore_ascii_case(key.trim()))
}

/// The country a company's people come from (a company of before the countries: its map's).
pub fn country_of(c: &Company) -> &'static str {
    if known(&c.country) {
        &pool(&c.country).key
    } else {
        guess(&c.map, &c.map_name)
    }
}

/// The groups of a country a name is drawn from, each with its share and the pool it belongs to
/// (a mixed pool's: those of the countries it mixes, each country as much as another).
fn groups_of(p: &'static Pool) -> Vec<(f64, &'static Pool, &'static Group)> {
    if p.mix.is_empty() {
        return p.groups.iter().map(|g| (g.share, p, g)).collect();
    }
    let mut out = Vec::new();
    for k in &p.mix {
        let q = pool(k);
        if q.mix.is_empty() {
            let total: f64 = q.groups.iter().map(|g| g.share).sum();
            out.extend(q.groups.iter().map(|g| (g.share / total.max(1e-9), q, g)));
        }
    }
    out
}

/// A surname in its feminine form by the rules of `rule` (pl, cs, sk, ru, uk, el; anything
/// else: the same for men and women).
pub fn feminine(rule: &str, s: &Surname) -> String {
    if let Some(f) = &s.female {
        return f.clone();
    }
    let m = s.male.as_str();
    let cut = |n: usize| m.chars().take(m.chars().count().saturating_sub(n)).collect::<String>();
    match rule {
        // (-ski, -cki, -dzki: -ska, -cka, -dzka; Nowak, Wójcik stay as they are)
        "pl" => {
            if m.ends_with("ki") {
                format!("{}a", cut(1))
            } else {
                m.to_string()
            }
        }
        // (Novotný Novotná, Krejčí stays, Svoboda Svobodová, Hájek Hájková, Novák Nováková;
        // the irregular ones, Němec Němcová, are written out in the file)
        "cs" | "sk" => {
            let last = m.chars().last().unwrap_or(' ');
            if last == 'ý' {
                format!("{}á", cut(1))
            } else if last == 'í' {
                m.to_string()
            } else if last == 'a' || (rule == "sk" && last == 'o') {
                format!("{}ová", cut(1))
            } else if rule == "cs" && m.ends_with("ek") && m.chars().rev().nth(2).is_some_and(|c| !"aeiouyáéíóúůýě".contains(c)) {
                format!("{}ková", cut(2))
            } else {
                format!("{m}ová")
            }
        }
        "ru" => {
            if m.ends_with("ский") || m.ends_with("цкий") || m.ends_with("ой") || m.ends_with("ый") {
                format!("{}ая", cut(2))
            } else if ["ов", "ев", "ёв", "ин", "ын"].iter().any(|x| m.ends_with(x)) {
                format!("{m}а")
            } else {
                m.to_string()
            }
        }
        "uk" => {
            if m.ends_with("ський") || m.ends_with("цький") || m.ends_with("ий") {
                format!("{}а", cut(2))
            } else if m.ends_with("ій") {
                format!("{}я", cut(2))
            } else if ["ов", "ев", "єв", "ін", "ин"].iter().any(|x| m.ends_with(x)) {
                format!("{m}а")
            } else {
                m.to_string()
            }
        }
        // (Greek: the genitive, Παπαδόπουλος Παπαδοπούλου, Μιχαηλίδης Μιχαηλίδου, Παππάς Παππά)
        "el" => {
            if m.ends_with("όπουλος") {
                format!("{}οπούλου", cut(7))
            } else if m.ends_with("δης") {
                format!("{}δου", cut(3))
            } else if m.ends_with("ος") {
                format!("{}ου", cut(2))
            } else if m.ends_with("ός") {
                format!("{}ού", cut(2))
            } else if m.ends_with('ς') {
                cut(1)
            } else {
                m.to_string()
            }
        }
        _ => m.to_string(),
    }
}

/// A person drawn: their name as it is written and shown, and whether she is a woman.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub name: String,
    pub female: bool,
}

/// One person of `country` by `rng`, whatever their name.
fn one(country: &str, rng: &mut Rng) -> Person {
    let groups = groups_of(pool(country));
    let total: f64 = groups.iter().map(|g| g.0).sum();
    let mut x = rng.f64() * total;
    let (_, p, g) = groups.iter().find(|g| {
        x -= g.0;
        x < 0.0
    }).or(groups.last()).copied().expect("every pool has a group");
    let part = rng.pick(&g.parts).expect("every group has its names");
    let female = rng.chance(0.5);
    let firsts = if female { &part.female } else { &part.male };
    let first = rng.pick(firsts).cloned().unwrap_or_else(|| "Alex".into());
    let surname = |rng: &mut Rng| rng.pick(&part.surnames).map(|s| if female { feminine(&g.feminine, s) } else { s.male.clone() }).unwrap_or_else(|| "Weber".into());
    let mut family = surname(rng);
    if p.double_surname > 0.0 && rng.chance(p.double_surname) {
        let second = surname(rng);
        if second != family {
            family = format!("{family} {second}");
        }
    }
    let name = if p.family_first { format!("{family} {first}") } else { format!("{first} {family}") };
    Person { name, female }
}

/// A person of `country` whose name nobody in `used` has (it is added): drawn again until it is
/// a new one, a middle initial at the very last.
pub fn draw(country: &str, rng: &mut Rng, used: &mut HashSet<String>) -> Person {
    for _ in 0..200 {
        let p = one(country, rng);
        if used.insert(p.name.clone()) {
            return p;
        }
    }
    let p = one(country, rng);
    for letter in 'A'..='Z' {
        let initial = format!("{letter}.");
        let mut words: Vec<&str> = p.name.splitn(2, ' ').collect();
        words.insert(1, &initial);
        let name = words.join(" ");
        if used.insert(name.clone()) {
            return Person { name, female: p.female };
        }
    }
    p
}

/// The names in use in a company: its people's, the agency drivers' booked and the players who
/// joined (no new person gets one of them).
pub fn used_in(c: &Company) -> HashSet<String> {
    let mut used: HashSet<String> = c.staff.iter().map(|e| e.name.clone()).collect();
    used.extend(c.staff_rules.names_given.iter().cloned());
    used.extend(c.planning.agency.iter().flat_map(|b| b.drivers.iter().map(|d| d.employee.name.clone())));
    used
}

/// Every first name of every country, each with whether it is a woman's (a name both are of:
/// the first country's word).
fn genders() -> &'static HashMap<String, bool> {
    static MAP: OnceLock<HashMap<String, bool>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for p in pools() {
            for g in &p.groups {
                for (names, f) in [(&g.male, false), (&g.female, true)] {
                    for n in names {
                        m.entry(n.to_lowercase()).or_insert(f);
                    }
                }
            }
        }
        m
    })
}

/// Whether `first` is a woman's first name (Some(true)), a man's (Some(false)), or no first
/// name of ours (None).
pub fn female_first(first: &str) -> Option<bool> {
    genders().get(&first.to_lowercase()).copied()
}

/// Whether the person called `name` is a woman by their first name: the first word, or the last
/// when the first is none (the family name first: "Nagy Anna").
pub fn female(name: &str) -> Option<bool> {
    let mut words = name.split_whitespace();
    let first = words.next()?;
    female_first(first).or_else(|| words.last().and_then(female_first))
}

/// A letter without its accents, lower case (for sorting: "Ö" by the O, "ß" as "ss"; `fold` a
/// whole text).
fn fold_into(out: &mut String, c: char) {
    const FROM: &str = "àáâãäåāăąçćčďđèéêëēėęěğìíîïīıįłľĺñńňòóôõöőøōŕřśšşșťţțùúûüűůūųýÿźżž";
    const TO: &str = "aaaaaaaaacccddeeeeeeeegiiiiiiilllnnnoooooooorrsssstttuuuuuuuuyyzzz";
    for l in c.to_lowercase() {
        match l {
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            'þ' => out.push_str("th"),
            _ => match FROM.chars().position(|x| x == l) {
                Some(k) => out.extend(TO.chars().nth(k)),
                None => out.push(l),
            },
        }
    }
}

/// A text without its accents, lower case (`sort_key`; a list's search).
pub fn fold(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        fold_into(&mut out, c);
    }
    out
}

/// The key a list of people sorts by: the family name first, its leading particles left out
/// (the Dutch "Jan van der Berg" under B, as in the Netherlands; a capital particle counts, as
/// in Flanders: "Lies Van Damme" under V), then the first name; accents do not count.
pub fn sort_key(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() < 2 {
        return fold(name);
    }
    // (the family name first: the first word no first name, the last one is)
    let family_first = female_first(words[0]).is_none() && female_first(words[words.len() - 1]).is_some();
    let (given, family) = if family_first { (&words[words.len() - 1..], &words[..words.len() - 1]) } else { (&words[..1], &words[1..]) };
    let particles = family.iter().take_while(|w| w.chars().next().is_some_and(char::is_lowercase)).count().min(family.len() - 1);
    format!("{} {} {}", fold(&family[particles..].join(" ")), fold(&given.join(" ")), fold(&family[..particles].join(" "))).trim_end().to_string()
}

/// Words of a map's file or name that tell its country (the towns and regions OMSI's maps are
/// set in, and the countries' own names).
const MAP_WORDS: [(&str, &[&str]); 27] = [
    ("nl", &["nl", "nederland", "netherlands", "holland", "dutch", "amsterdam", "rotterdam", "utrecht", "haag", "eindhoven", "groningen", "tilburg", "almere", "breda", "nijmegen", "arnhem", "zwolle", "leiden", "haarlem", "maastricht", "enschede", "apeldoorn", "amersfoort", "dordrecht", "friesland", "limburg", "zeeland", "brabant"]),
    ("be", &["belgie", "belgium", "belgique", "brussel", "bruxelles", "brussels", "antwerpen", "anvers", "gent", "ghent", "brugge", "leuven", "liege", "luik", "charleroi", "namur", "mechelen", "hasselt", "vlaanderen", "wallonie"]),
    ("de", &["de", "deutschland", "germany", "german", "grundorf", "spandau", "berlin", "hamburg", "munchen", "muenchen", "munich", "koln", "koeln", "cologne", "frankfurt", "dresden", "leipzig", "chemnitz", "hannover", "bremen", "essen", "dortmund", "ruhr", "ruhrau", "bochum", "wuppertal", "gladbeck", "duisburg", "nurnberg", "nuernberg", "stuttgart", "rugen", "ruegen", "huegelsdorf", "hugelsdorf", "neuendorf", "hellenstein", "kiel", "lubeck", "luebeck", "aachen", "bayern", "sachsen", "westfalen", "niedersachsen", "harz", "mosel"]),
    ("at", &["at", "osterreich", "oesterreich", "austria", "wien", "vienna", "graz", "linz", "salzburg", "innsbruck", "klagenfurt", "tirol", "kärnten", "steiermark", "vorarlberg"]),
    ("ch", &["ch", "schweiz", "suisse", "svizzera", "switzerland", "zurich", "zuerich", "bern", "basel", "geneve", "genf", "geneva", "lausanne", "luzern", "lugano", "winterthur", "stgallen", "lemmental"]),
    ("gb", &["uk", "gb", "british", "britain", "england", "english", "london", "routemaster", "rhd", "tfl", "scotland", "wales", "yorkshire", "lincolnshire", "manchester", "birmingham", "liverpool", "leeds", "glasgow", "edinburgh", "cardiff", "bristol", "newcastle", "sheffield", "nottingham", "brighton", "kent", "cornwall"]),
    ("ie", &["ireland", "eire", "irish", "dublin", "cork", "galway", "limerick", "belfast"]),
    ("fr", &["france", "french", "paris", "lyon", "marseille", "toulouse", "lille", "bordeaux", "nantes", "strasbourg", "nice", "rennes", "montpellier", "grenoble", "rouen", "bretagne", "normandie", "alsace", "provence"]),
    ("pl", &["pl", "polska", "poland", "polish", "warszawa", "warsaw", "krakow", "kraków", "wroclaw", "wrocław", "gdansk", "gdańsk", "poznan", "poznań", "lodz", "łódź", "szczecin", "lublin", "katowice", "bydgoszcz", "gdynia", "bialystok", "białystok", "rzeszow", "rzeszów", "slask", "śląsk", "mazury"]),
    ("cz", &["cz", "cesko", "česko", "czech", "czechia", "praha", "prague", "brno", "ostrava", "plzen", "plzeň", "liberec", "olomouc", "budejovice", "budějovice", "pardubice", "morava"]),
    ("sk", &["slovensko", "slovakia", "slovak", "bratislava", "kosice", "košice", "presov", "prešov", "zilina", "žilina", "nitra", "trnava", "trencin", "trenčín"]),
    ("hu", &["hu", "magyarorszag", "magyarország", "hungary", "hungarian", "budapest", "debrecen", "szeged", "miskolc", "pecs", "pécs", "gyor", "győr", "balaton"]),
    ("ro", &["ro", "romania", "românia", "bucuresti", "bucurești", "bucharest", "cluj", "timisoara", "timișoara", "iasi", "iași", "constanta", "constanța", "brasov", "brașov", "sibiu", "oradea", "transilvania"]),
    ("ua", &["ua", "ukraine", "ukraina", "україна", "kyiv", "kiev", "київ", "lviv", "львів", "kharkiv", "харків", "odesa", "odessa", "одеса", "dnipro", "zaporizhzhia", "vinnytsia"]),
    ("ru", &["ru", "russia", "rossiya", "россия", "moskva", "moscow", "москва", "petersburg", "peterburg", "петербург", "novosibirsk", "yekaterinburg", "ekaterinburg", "kazan", "казань", "samara", "nizhny", "omsk", "rostov", "volgograd"]),
    ("it", &["it", "italia", "italy", "italian", "roma", "rome", "milano", "milan", "napoli", "naples", "torino", "turin", "palermo", "genova", "bologna", "firenze", "florence", "venezia", "venice", "verona", "bari", "trieste", "sicilia", "toscana"]),
    ("es", &["es", "espana", "españa", "spain", "spanish", "madrid", "barcelona", "valencia", "sevilla", "seville", "zaragoza", "malaga", "málaga", "bilbao", "mallorca", "palma", "murcia", "alicante", "granada", "cataluna", "catalunya", "andalucia", "galicia"]),
    ("pt", &["portugal", "portuguese", "lisboa", "lisbon", "porto", "braga", "coimbra", "faro", "funchal", "madeira", "algarve", "aveiro"]),
    ("dk", &["dk", "danmark", "denmark", "danish", "kobenhavn", "københavn", "copenhagen", "aarhus", "odense", "aalborg", "esbjerg", "jylland", "sjaelland", "sjælland"]),
    ("se", &["sverige", "sweden", "swedish", "stockholm", "goteborg", "göteborg", "gothenburg", "malmo", "malmö", "uppsala", "vasteras", "västerås", "orebro", "örebro", "linkoping", "linköping", "norrland", "skane", "skåne"]),
    ("no", &["norge", "norway", "norwegian", "oslo", "bergen", "trondheim", "stavanger", "drammen", "tromso", "tromsø", "kristiansand"]),
    ("fi", &["fi", "suomi", "finland", "finnish", "helsinki", "espoo", "tampere", "vantaa", "turku", "oulu", "jyvaskyla", "jyväskylä", "lahti", "kuopio"]),
    ("us", &["us", "usa", "america", "american", "newyork", "manhattan", "brooklyn", "chicago", "losangeles", "houston", "phoenix", "philadelphia", "boston", "seattle", "detroit", "miami", "atlanta", "denver", "texas", "california", "mta"]),
    ("ca", &["canada", "canadian", "toronto", "montreal", "montréal", "vancouver", "ottawa", "calgary", "edmonton", "winnipeg", "quebec", "québec", "ontario", "ttc"]),
    ("au", &["australia", "australian", "sydney", "melbourne", "brisbane", "perth", "adelaide", "canberra", "hobart", "darwin", "queensland", "victoria"]),
    ("tr", &["tr", "turkiye", "türkiye", "turkey", "turkish", "istanbul", "İstanbul", "ankara", "izmir", "i̇zmir", "bursa", "antalya", "adana", "konya", "gaziantep", "kayseri", "iett"]),
    ("gr", &["gr", "greece", "greek", "hellas", "ellada", "athens", "athina", "athen", "thessaloniki", "saloniki", "patra", "patras", "heraklion", "iraklio", "larisa", "crete", "kriti"]),
];

/// The country a map is set in, guessed from the words of its file and its name
/// (`maps/Grundorf/global.cfg`: Germany); none of them known: International.
pub fn guess(map: &str, map_name: &str) -> &'static str {
    let text = format!("{map} {map_name}").to_lowercase();
    // (the words, and the words run together: "New York" as "newyork")
    let words: Vec<String> = text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty() && *w != "maps" && *w != "global" && *w != "cfg").map(str::to_string).collect();
    let joined: Vec<String> = words.windows(2).map(|w| format!("{}{}", w[0], w[1])).collect();
    for (key, list) in MAP_WORDS {
        if words.iter().chain(joined.iter()).any(|w| list.contains(&w.as_str())) {
            return key;
        }
    }
    // (a word that holds a town's name: "Spandau1986", "Hamburg-Tag")
    for (key, list) in MAP_WORDS {
        if words.iter().any(|w| list.iter().filter(|x| x.chars().count() >= 5).any(|x| w.contains(x))) {
            return key;
        }
    }
    INTERNATIONAL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pool_parses_and_is_big_enough() {
        assert_eq!(pools().len(), FILES.len(), "a file does not parse: {:?}", FILES.iter().map(|t| parse(t).err()).collect::<Vec<_>>());
        let mut keys = HashSet::new();
        for p in pools() {
            assert!(keys.insert(p.key.clone()), "{} twice", p.key);
            assert!(!p.flag.is_empty() && !p.name.is_empty(), "{}", p.key);
            let groups = groups_of(p);
            assert!(!groups.is_empty(), "{} has no groups", p.key);
            let mut male = HashSet::new();
            let mut female = HashSet::new();
            let mut families = HashSet::new();
            for (share, _, g) in &groups {
                assert!(*share > 0.0, "{}: {} has no share", p.key, g.origin);
                assert!(g.parts.iter().all(|x| !x.male.is_empty() && !x.female.is_empty() && !x.surnames.is_empty()), "{}: {} lacks names", p.key, g.origin);
                male.extend(g.male.iter().cloned());
                female.extend(g.female.iter().cloned());
                families.extend(g.surnames.iter().map(|s| s.male.clone()));
            }
            assert!(male.len() + female.len() >= 150, "{}: only {} first names", p.key, male.len() + female.len());
            assert!(male.len() >= 70 && female.len() >= 70, "{}: {} men's and {} women's first names", p.key, male.len(), female.len());
            assert!(families.len() >= 150, "{}: only {} surnames", p.key, families.len());
        }
        // (the mixed pool mixes the countries of the Latin alphabet: anyone reads its names)
        assert!(pool(INTERNATIONAL).mix.len() >= 20);
        assert!(pool(INTERNATIONAL).mix.iter().all(|k| known(k) && k != INTERNATIONAL));
        assert_eq!(pool("nonsense").key, INTERNATIONAL);
    }

    #[test]
    fn no_first_name_is_both_a_mans_and_a_womans() {
        // (the faces follow the first names: `staff::female_name`)
        let mut seen: HashMap<String, (bool, String)> = HashMap::new();
        let mut both = Vec::new();
        for p in pools() {
            for g in &p.groups {
                for (names, f) in [(&g.male, false), (&g.female, true)] {
                    for n in names {
                        if let Some((was, at)) = seen.get(&n.to_lowercase()) {
                            if *was != f {
                                both.push(format!("{n} ({at} and {})", p.key));
                            }
                        } else {
                            seen.insert(n.to_lowercase(), (f, p.key.clone()));
                        }
                    }
                }
            }
        }
        assert!(both.is_empty(), "first names of both genders: {}", both.join(", "));
    }

    #[test]
    fn a_family_name_first_is_no_first_name() {
        // (Hungary's "Antal Eszter" would read as a man called Antal)
        for p in pools().iter().filter(|p| p.family_first) {
            let bad: Vec<&str> = p.groups.iter().flat_map(|g| g.surnames.iter()).filter(|s| female_first(&s.male).is_some()).map(|s| s.male.as_str()).collect();
            assert!(bad.is_empty(), "{}: surnames that are first names: {bad:?}", p.key);
        }
        assert_eq!(female("Nagy Anna"), Some(true));
        assert_eq!(female("Kovács Péter"), Some(false));
    }

    #[test]
    fn names_are_never_given_twice() {
        for p in pools() {
            let mut used = HashSet::new();
            let mut rng = Rng::of(&["twice", &p.key], 0);
            for _ in 0..600 {
                let before = used.len();
                let person = draw(&p.key, &mut rng, &mut used);
                assert_eq!(used.len(), before + 1, "{}: {} given twice", p.key, person.name);
                assert_eq!(female(&person.name), Some(person.female), "{}: {}", p.key, person.name);
            }
        }
    }

    #[test]
    fn polish_czech_and_slovak_women_have_their_forms() {
        let s = |m: &str| Surname { male: m.into(), female: None };
        assert_eq!(feminine("pl", &s("Kowalski")), "Kowalska");
        assert_eq!(feminine("pl", &s("Wiśniewski")), "Wiśniewska");
        assert_eq!(feminine("pl", &s("Nowicki")), "Nowicka");
        assert_eq!(feminine("pl", &s("Zawadzki")), "Zawadzka");
        assert_eq!(feminine("pl", &s("Nowak")), "Nowak");
        assert_eq!(feminine("cs", &s("Novák")), "Nováková");
        assert_eq!(feminine("cs", &s("Novotný")), "Novotná");
        assert_eq!(feminine("cs", &s("Svoboda")), "Svobodová");
        assert_eq!(feminine("cs", &s("Hájek")), "Hájková");
        assert_eq!(feminine("cs", &s("Krejčí")), "Krejčí");
        assert_eq!(feminine("cs", &s("Dvořák")), "Dvořáková");
        assert_eq!(feminine("sk", &s("Kováč")), "Kováčová");
        assert_eq!(feminine("sk", &s("Šimko")), "Šimková");
        assert_eq!(feminine("ru", &s("Иванов")), "Иванова");
        assert_eq!(feminine("ru", &s("Достоевский")), "Достоевская");
        assert_eq!(feminine("uk", &s("Шевченко")), "Шевченко");
        assert_eq!(feminine("uk", &s("Ковальський")), "Ковальська");
        assert_eq!(feminine("el", &s("Παπαδόπουλος")), "Παπαδοπούλου");
        assert_eq!(feminine("el", &s("Μιχαηλίδης")), "Μιχαηλίδου");
        assert_eq!(feminine("el", &s("Παππάς")), "Παππά");
        assert_eq!(feminine("el", &s("Γεωργίου")), "Γεωργίου");
        // (the irregular ones as the file writes them)
        let cz = pool("cz");
        let nemec = cz.groups.iter().flat_map(|g| g.surnames.iter()).find(|s| s.male == "Němec").expect("Němec");
        assert_eq!(feminine("cs", nemec), "Němcová");
        // (and the pools give them to the women: every Polish -ski woman is a -ska)
        let mut used = HashSet::new();
        let mut rng = Rng::of(&["forms"], 0);
        let mut seen = 0;
        for _ in 0..400 {
            let p = draw("pl", &mut rng, &mut used);
            let family = p.name.split_whitespace().last().unwrap();
            if p.female {
                assert!(!family.ends_with("ski") && !family.ends_with("cki"), "{}", p.name);
            } else {
                assert!(!family.ends_with("ska") && !family.ends_with("cka"), "{}", p.name);
            }
            seen += usize::from(p.female && family.ends_with("ska"));
        }
        assert!(seen > 10);
        // (a Czech woman: -ová or -á, Krejčí as it is; the Ukrainians of Czechia keep their own)
        let czech = &pool("cz").groups[0];
        for _ in 0..400 {
            let p = draw("cz", &mut rng, &mut used);
            let (first, family) = p.name.split_once(' ').unwrap();
            if p.female && czech.female.iter().any(|f| f == first) && !family.ends_with('í') {
                assert!(family.ends_with("ová") || family.ends_with('á'), "{}", p.name);
            }
        }
    }

    #[test]
    fn dutch_tussenvoegsels_sort_by_the_name_after_them() {
        let mut v = vec!["Jan van der Berg", "Piet Bakker", "Anna de Vries", "Kees Visser", "Fatima El Amrani", "Lies Van Damme", "Özlem Yıldız", "Nagy Péter", "Sanne van Dijk"];
        v.sort_by_key(|n| sort_key(n));
        assert_eq!(v, ["Piet Bakker", "Jan van der Berg", "Sanne van Dijk", "Fatima El Amrani", "Nagy Péter", "Lies Van Damme", "Kees Visser", "Anna de Vries", "Özlem Yıldız"]);
        assert_eq!(sort_key("Jan van der Berg"), "berg jan van der");
    }

    #[test]
    fn a_company_never_gives_a_name_twice() {
        use super::super::{dates, found, staff, Founding, GameMode};
        let mut c = found(&Founding { name: "Namen".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), country: "nl".into(), ..Default::default() }, "Luc");
        assert_eq!(c.country, "nl");
        let mut gone = Vec::new();
        for week in 0..12 {
            // (a week's applicants: none of the company's people's names, nor of those who left)
            let market = staff::market(&c);
            let mut names: Vec<&str> = market.iter().map(|a| a.name.as_str()).collect();
            let n = names.len();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), n, "week {week}: an applicant's name twice");
            for a in &market {
                assert!(!c.staff.iter().any(|e| e.name == a.name) && !gone.contains(&a.name), "week {week}: {} again", a.name);
            }
            for a in market.iter().take(3) {
                staff::hire(&mut c, a).expect("hired");
            }
            // (one of them leaves again: their name stays the company's)
            let e = c.staff.remove(0);
            gone.push(e.name);
            c.date = dates::add(&c.date, 7);
        }
        assert!(c.staff_rules.names_given.len() >= 36);
    }

    #[test]
    fn an_older_company_plays_in_its_maps_country() {
        use super::super::{found, store, Company, Founding, GameMode};
        let old = |map: &str| {
            let c = found(&Founding { name: "Alt".into(), game_mode: GameMode::Normal, map: map.into(), ..Default::default() }, "Luc");
            let mut v = serde_json::to_value(&c).unwrap();
            v.as_object_mut().unwrap().remove("country");
            let c: Company = serde_json::from_value(v).unwrap();
            assert_eq!(c.country, "", "a file of before has no country");
            store::migrate(c)
        };
        assert_eq!(old("maps/Grundorf/global.cfg").country, "de");
        assert_eq!(old("maps/Phantasia/global.cfg").country, INTERNATIONAL);
        // (and its people keep their names: only the new ones are of the country)
        let mut c = old("maps/Phantasia/global.cfg");
        c.staff.push(super::super::plan::agency_driver());
        c.staff[0].name = "Anna Becker".into();
        c.country = "pl".into();
        let c = store::migrate(c);
        assert_eq!((c.country.as_str(), c.staff[0].name.as_str()), ("pl", "Anna Becker"));
    }

    #[test]
    fn the_map_tells_the_country() {
        assert_eq!(guess("maps/Grundorf/global.cfg", "Grundorf"), "de");
        assert_eq!(guess("maps\\Spandau1986\\global.cfg", ""), "de");
        assert_eq!(guess("maps/London_Route_4/global.cfg", ""), "gb");
        assert_eq!(guess("maps/Amsterdam/global.cfg", "Amsterdam Noord"), "nl");
        assert_eq!(guess("maps/Krakow/global.cfg", ""), "pl");
        assert_eq!(guess("maps/Somewhere/global.cfg", "Somewhere"), INTERNATIONAL);
        assert_eq!(guess("", ""), INTERNATIONAL);
    }
}
