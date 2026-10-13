//! What cannot be done, said plainly (`crates/omsi-launcher-ui/src/company/kit.rs`): the popup
//! with what, why and what opens it, and the way there. The page draws it as it comes; its
//! way (`go`) is a page of the company's the page opens.

use super::words::{eur, grouped, tr, trf};
use omsi_launcher_lib::company::{self as co, finance, levels, Company};
use serde_json::{json, Value};

/// Where a popup's second button goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Go {
    Progress,
    Depot,
    Loan,
    Finances,
    Courses,
    Planning,
    Lines,
    Dealer,
    Hire,
    Training,
    Licences,
    Research,
    /// A Casual company switched to Normal (asked first), then this page of it.
    ToNormal(usize),
}

impl Go {
    pub fn label(self) -> &'static str {
        match self {
            Go::Research => "To the research",
            Go::ToNormal(_) => "Switch to Normal",
            Go::Progress => "Show my progress",
            Go::Depot => "Build parking spaces",
            Go::Loan => "Take a loan",
            Go::Finances => "Show the finances",
            Go::Courses => "To the courses",
            Go::Planning => "Open the planning",
            Go::Lines => "To the lines",
            Go::Dealer => "To the dealer",
            Go::Hire => "Hire drivers",
            Go::Training => "Train the drivers",
            Go::Licences => "Train drivers",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Go::Research => "science",
            Go::ToNormal(_) => "tune",
            Go::Progress => "emoji_events",
            Go::Depot => "garage",
            Go::Loan | Go::Finances => "payments",
            Go::Courses => "badge",
            Go::Planning => "event",
            Go::Lines => "route",
            Go::Dealer => "directions_bus",
            Go::Hire => "groups",
            Go::Training => "badge",
            Go::Licences => "key",
        }
    }

    /// Its name for the page (`company/company.js`'s `go`).
    pub fn key(self) -> String {
        match self {
            Go::Progress => "progress".into(),
            Go::Depot => "depot".into(),
            Go::Loan => "loan".into(),
            Go::Finances => "finances".into(),
            Go::Courses => "courses".into(),
            Go::Planning => "planning".into(),
            Go::Lines => "lines".into(),
            Go::Dealer => "dealer".into(),
            Go::Hire => "hire".into(),
            Go::Training => "training".into(),
            Go::Licences => "licences".into(),
            Go::Research => "research".into(),
            Go::ToNormal(tab) => format!("to-normal:{tab}"),
        }
    }

    /// The way as the page has it: its name, its words and its icon.
    pub fn json(self) -> Value {
        json!({ "to": self.key(), "label": tr(self.label()), "icon": self.icon() })
    }
}

/// Something that cannot be done: what, why, and what opens it.
#[derive(Clone, Debug, PartialEq)]
pub struct Popup {
    pub icon: &'static str,
    pub title: String,
    pub text: String,
    pub unlock: String,
    pub go: Option<Go>,
}

impl Popup {
    pub fn new(icon: &'static str, title: &str, text: impl Into<String>, unlock: impl Into<String>, go: Option<Go>) -> Popup {
        Popup { icon, title: tr(title), text: text.into(), unlock: unlock.into(), go }
    }

    /// As the page draws it; in Casual only the ways to its own pages (`casual::popup_go`).
    pub fn json(&self, casual: bool) -> Value {
        let go = if casual { self.go.and_then(super::casual::popup_go) } else { self.go };
        json!({ "icon": self.icon, "title": self.title, "text": self.text, "unlock": self.unlock, "go": go.map(Go::json) })
    }
}

/// The company's level in words: "you are level 1 (430 of 600 points)".
fn level_words(c: &Company) -> String {
    let lv = levels::level(c);
    let (xp, _, next) = levels::progress(c);
    match next {
        Some(n) => trf("Your company is level %{lv} (%{xp} of %{next} points).", &[("lv", &lv.to_string()), ("xp", &grouped(xp as f64)), ("next", &grouped(n as f64))]),
        None => trf("Your company is level %{lv}.", &[("lv", &lv.to_string())]),
    }
}

/// A feature the company's level does not open yet.
pub fn locked(c: &Company, f: levels::Feature) -> Popup {
    let text = trf("%{what} open at company level %{n}.", &[("what", &tr(f.label())), ("n", &f.level().to_string())]);
    Popup::new("lock", "Not open yet", format!("{text} {}", level_words(c)), tr("The company earns points with every day closed: tours run, trips on time, passengers carried and a good reputation."), Some(Go::Progress))
}

/// Not enough cash for `amount` (0: not known).
pub fn no_cash(c: &Company, amount: co::Cents) -> Popup {
    let mut text = trf("The company has %{cash} in cash.", &[("cash", &eur(c.cash))]);
    if amount > 0 {
        text.push(' ');
        text.push_str(&trf("This costs %{amount}: %{short} more than there is.", &[("amount", &eur(amount)), ("short", &eur((amount - c.cash).max(0)))]));
    }
    let room = finance::credit(c, 0).left;
    let (unlock, go) = if room >= finance::SMALLEST_LOAN {
        (trf("The bank lends up to %{amount} more. Or sell a bus, or wait for the month's income.", &[("amount", &eur(room))]), Go::Loan)
    } else {
        (tr("The bank lends nothing more now: sell a bus, or wait for the month's income."), Go::Finances)
    };
    Popup::new("payments", "Not enough cash", text, unlock, Some(go))
}

/// The bank does not lend `amount` (0: not known).
pub fn no_credit(c: &Company, amount: co::Cents) -> Popup {
    let cr = finance::credit(c, 0);
    let mut text = trf("The bank lends at most %{left} more now (%{debt} of %{limit} used).", &[("left", &eur(cr.left)), ("debt", &eur(cr.debt)), ("limit", &eur(cr.limit))]);
    if amount > 0 {
        text = format!("{}  {}", trf("You asked for %{amount}.", &[("amount", &eur(amount))]), text);
    }
    Popup::new("payments", "The bank says no", text, tr("Pay loans back, or let the fleet and the results grow: the credit room grows with them."), Some(Go::Finances))
}

/// What a refusal of the company's rules means, and what opens it (`kit::refusal`).
pub fn refusal(c: &Company, reason: &str) -> Popup {
    use levels::Feature;
    let said = tr(reason);
    match reason {
        "Articulated buses open at a higher company level." => locked(c, Feature::ArticulatedBuses),
        "Double-deckers open at a higher company level." => locked(c, Feature::DoubleDeckers),
        "Rear adverts open at a higher company level." => locked(c, Feature::RearAdverts),
        "Side adverts open at a higher company level." => locked(c, Feature::SideAdverts),
        "Full wraps open at a higher company level." => locked(c, Feature::FullWraps),
        "Electric buses open at a higher company level." => locked(c, Feature::ElectricBuses),
        "Your company cannot build this yet." | "Your company's level does not offer this course yet." => Popup::new("lock", "Not open yet", format!("{said} {}", level_words(c)), tr("The company earns points with every day closed: tours run, trips on time, passengers carried and a good reputation."), Some(Go::Progress)),
        "Not enough cash." | "Not enough money for the course." => no_cash(c, 0),
        "The bank does not lend that much." => no_credit(c, 0),
        "The company cannot pay for the line: take a loan on the Finances page, or make it smaller." | "The company cannot pay for the change: take a loan on the Finances page." => {
            let p = no_cash(c, 0);
            Popup { text: format!("{said} {}", p.text), ..p }
        }
        "The depot has no room for another bus: build more parking spaces." | "The depot has no room for so many buses: build more parking spaces." => {
            let held = c.fleet.iter().filter(|v| v.held_on(&c.date)).count();
            let text = trf("The depot has %{spaces} parking spaces and holds %{buses} buses; a few more may stand in the street.", &[("spaces", &co::depot::places(c).to_string()), ("buses", &held.to_string())]);
            if co::autopilot::on(c) {
                Popup::new("garage", "No room in the depot", text, tr("The company builds more parking spaces by itself when its cash allows. Or buy a bigger depot now on your company's page."), Some(Go::Depot))
            } else {
                Popup::new("garage", "No room in the depot", text, tr("Build more parking spaces at the depot: each level gives twelve."), Some(Go::Depot))
            }
        }
        "Articulated buses need research first: start it on the Research page." | "Double-deckers need research first: start it on the Research page." => {
            if co::autopilot::on(c) {
                let what = if reason.starts_with("Articulated") { "Articulated buses need research first." } else { "Double-deckers need research first." };
                Popup::new("science", "Research first", tr(what), tr("The company researches them by itself once a researcher works here and the trophies and the money are there. To start it now, switch to Normal."), Some(Go::ToNormal(super::RESEARCH_TAB)))
            } else {
                Popup::new("science", "Research first", said, "", Some(Go::Research))
            }
        }
        "You need the repairs course first." => Popup::new("badge", "A course first", said, tr("Book the repairs course under Training."), Some(Go::Courses)),
        "Your driver level is too low for this test." => Popup::new("badge", "Not yet", said, tr("Your driver level grows with every trip you drive to a timetable."), Some(Go::Progress)),
        "Sign the contract first." => Popup::new("livery_pen", "Not signed yet", tr("Sign in the field with the mouse, or type your name beside it."), "", None),
        "This tender is closed." | "This tender has not opened yet." | "Another bid leads with more: bid at least the least shown." | "That is the buy-out price: buy the line instead." | "There is no such tender." => Popup::new("receipt_long", "The bid cannot be placed", said, "", None),
        "The dealer does not want to talk to you for now." => Popup::new("forum", "The dealer will not talk", said, tr("Come back another day - or buy at the list price."), None),
        "This timetable carries no passengers of its own: depot runs, empty runs or other traffic." => Popup::new(
            "route",
            "Not a line with passengers",
            said,
            tr("Depot and empty runs are part of the tours of the lines that start or end there: take those lines on and these runs are driven with them, as empty kilometres. Choose a line with passengers in the list."),
            None,
        ),
        _ => Popup::new("error", "This cannot be done", said, "", None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Founding, GameMode};

    #[test]
    fn a_refusal_says_why_and_what_opens_it() {
        super::super::words::set_language("en");
        let c = found(&Founding { name: "Kit".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let p = refusal(&c, "Articulated buses open at a higher company level.");
        assert_eq!(p.go, Some(Go::Progress));
        assert!(p.text.contains("level 2") && p.text.contains("of 600 points"), "{}", p.text);
        let p = refusal(&c, "The depot has no room for another bus: build more parking spaces.");
        assert_eq!(p.go, Some(Go::Depot));
        let mut e = c.clone();
        e.game_mode = GameMode::Casual;
        let p = refusal(&e, "Double-deckers need research first: start it on the Research page.");
        assert_eq!(p.go, Some(Go::ToNormal(super::super::RESEARCH_TAB)));
        // (Casual leads only to its own pages: the way to Normal is one)
        assert_eq!(p.json(true)["go"]["to"], "to-normal:10");
        assert_eq!(refusal(&c, "Something new.").go, None);
        let p = no_cash(&c, c.cash + 100_00);
        assert!(p.text.contains(&eur(100_00)) && p.go == Some(Go::Loan));
    }
}
