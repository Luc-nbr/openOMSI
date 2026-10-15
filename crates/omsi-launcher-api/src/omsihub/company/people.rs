//! The staff and the labour market (`crates/omsi-launcher-ui/src/company/people.rs`, `people/`):
//! My employees with a card for everyone and a person opened; Hire employees, the week's
//! applicants of every role; Licences & training, who of the drivers may drive which bus and the
//! courses; the Work schedule of the roles besides the drivers; the Holidays asked for and planned
//! (`holidays`); the agency drivers booked ahead for a day (`agencies`); and the staff's Settings.

pub mod agencies;
pub mod holidays;

use super::kit::{self, Go, Popup};
use super::words::{day_label, eur, eur_cents, grade, num, tr, trf};
use super::{act as apply, Session, RESEARCH_TAB};
use omsi_launcher_lib::company::day::Plan;
use omsi_launcher_lib::company::licences::{self, Endorsement, Lack};
use omsi_launcher_lib::company::research::keys;
use omsi_launcher_lib::company::staff::{self, Applicant, Grade, Hours};
use omsi_launcher_lib::company::training::{self, CourseKind};
use omsi_launcher_lib::company::{self as co, Company, Employee, Licence, Role};
use serde_json::{json, Value};

pub const TABS: [&str; 7] = ["My employees", "Hire employees", "Licences & training", "Work schedule", "Holidays", "Agencies", "Settings"];
const SORTS: [&str; 6] = ["Name", "Role", "Wage", "Level", "Satisfaction", "Stress"];
/// Said under a lock Casual does not open by itself.
const NOT_RESEARCHED_BY_ITSELF: &str = "Casual does not research this by itself: switch to Normal and start it on its Research page.";

/// What a driver lacks for a bus, in words.
pub fn lack_text(c: &Company, x: &Lack) -> String {
    match x {
        Lack::Licence => tr("No licence for this bus"),
        Lack::Endorsement(e) => tr(e.lacking()),
        Lack::Type(k) => trf("No type training for the %{bus}", &[("bus", &licences::type_name(c, k))]),
    }
}

/// A bus joined the fleet: how many drivers may drive it, when not all.
pub fn drivers_note(c: &Company, id: u32) -> Option<String> {
    let v = c.vehicle(id)?;
    let (q, m) = licences::qualified_drivers(c, v.kind, &v.bus);
    (m > 0 && q < m).then(|| trf("%{q} of %{m} drivers may drive it: train more on the Staff page.", &[("q", &q.to_string()), ("m", &m.to_string())]))
}

fn licence_line(l: Licence, e: &[Endorsement]) -> String {
    let mut s = match l {
        Licence::D => "D",
        Licence::D1 => "D1",
    }
    .to_string();
    if !e.is_empty() {
        s.push_str(" · ");
        s.push_str(&e.iter().map(|x| x.short()).collect::<Vec<_>>().join(" "));
    }
    s
}

fn endorsements_tip(l: Licence, e: &[Endorsement]) -> String {
    let mut v = vec![tr(if l == Licence::D1 { "D1: only midibuses" } else { "D: every bus" })];
    v.extend(e.iter().map(|x| format!("{}: {}", x.short(), tr(x.label()))));
    v.join("\n")
}

fn grade_tone(g: Grade) -> &'static str {
    match g {
        Grade::Trainee => "danger",
        Grade::Novice => "warn",
        Grade::Experienced => "early",
        Grade::Expert => "ok",
    }
}

fn span(from: i32, to: i32) -> String {
    format!("{} - {}", co::clock::hhmm(from as i64), co::clock::hhmm(to as i64))
}

fn shift_text(s: &staff::Shift) -> String {
    match s.extra {
        Some((a, b)) if a == s.to => span(s.from, b),
        Some((a, b)) => format!("{}  ·  {}", span(s.from, s.to), span(a, b)),
        None => span(s.from, s.to),
    }
}

fn per_hour(wage: co::Cents) -> String {
    trf("%{amount} / hour", &[("amount", &eur_cents(staff::hourly(wage)))])
}

/// What someone is doing now: two lines and their tone (`people::list::doing`).
fn doing(c: &Company, plan: Option<&Plan>, e: &Employee) -> (String, String, &'static str) {
    let today = c.date.as_str();
    let now = c.clock.minute as i32;
    let until = |u: &Option<String>| u.clone().filter(|u| co::dates::between(today, u) >= 0);
    let until_text = |u: &str| trf("until %{date}", &[("date", &day_label(u))]);
    if let Some(u) = until(&e.sick_until) {
        return (tr("Ill"), until_text(&u), "warn");
    }
    if let Some(u) = e.on_holiday(today) {
        return (tr("On holiday"), until_text(u), "soft");
    }
    if let Some(u) = until(&e.training_until) {
        return (tr("On a course"), until_text(&u), "accent2");
    }
    if staff::drives(e) {
        let mut duties: Vec<(i32, i32, String)> = plan.map(|p| p.tours.iter().filter(|t| !t.tour.unplanned).flat_map(|t| t.duties.iter().filter(|d| d.driver == Some(e.id)).map(move |d| (d.from, d.to, t.tour.number.clone()))).collect()).unwrap_or_default();
        duties.sort_by_key(|d| d.0);
        let (Some(first), Some(last)) = (duties.first(), duties.last()) else {
            let off = e.week_days >= staff::WEEK_DAYS;
            return (tr(if off { "Days off" } else { "Free today" }), String::new(), "soft");
        };
        let day = span(first.0, last.1);
        return match duties.iter().find(|d| d.0 <= now && now < d.1) {
            Some(d) => (tr("Driving a bus"), format!("{}  ·  {}", trf("Line %{n}", &[("n", &d.2)]), span(d.0, d.1)), "ok"),
            None if now < first.0 => (tr("Drives today"), day, "text"),
            None if now >= last.1 => (tr("Resting"), day, "soft"),
            None => (tr("On a break"), day, "soft"),
        };
    }
    let Some(s) = staff::shift_of(c, today, e) else {
        let free = !staff::on_rota(e, today);
        return (tr(if free { "Day off" } else { "Resting" }), String::new(), "soft");
    };
    let hours = shift_text(&s);
    if s.at(now) {
        let detail = match e.role {
            Role::Inspector => staff::inspected(c, today).iter().find(|x| x.2.contains(&e.id)).and_then(|x| c.lines.iter().find(|l| l.name == x.0)).map(|l| trf("Line %{n}", &[("n", &l.number)])).unwrap_or(hours),
            _ => hours,
        };
        (tr(staff::activity(e.role)), detail, "ok")
    } else if now < s.from {
        (trf("Starts at %{time}", &[("time", &co::clock::hhmm(s.from as i64))]), hours, "text")
    } else {
        (tr("Resting"), hours, "soft")
    }
}

/// The four bars of someone: their level, satisfaction, stress and friendliness.
fn needs(e: &Employee) -> Value {
    let g = Grade::of(e.experience);
    json!([
        { "label": tr(g.label()), "frac": e.experience / 100.0, "tone": grade_tone(g), "bold": true, "tip": trf("Experience %{n} of 100: Trainee, Novice, Experienced, Expert", &[("n", &num(e.experience, 0))]) },
        { "label": tr("Satisfaction"), "frac": e.satisfaction / 100.0, "tone": grade(e.satisfaction) },
        { "label": tr("Stress"), "frac": e.stress / 100.0, "tone": grade(100.0 - e.stress) },
        { "label": tr("Friendliness"), "frac": e.friendliness / 100.0, "tone": grade(e.friendliness) },
    ])
}

fn sort(list: &mut [Employee], by: usize, descending: bool) {
    list.sort_by(|a, b| {
        let o = match by {
            1 => Role::ALL.iter().position(|r| *r == a.role).cmp(&Role::ALL.iter().position(|r| *r == b.role)).then(a.name.cmp(&b.name)),
            2 => a.wage.cmp(&b.wage),
            3 => a.experience.total_cmp(&b.experience),
            4 => a.satisfaction.total_cmp(&b.satisfaction),
            5 => a.stress.total_cmp(&b.stress),
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        };
        if descending {
            o.reverse()
        } else {
            o
        }
    });
}

fn list_part(c: &Company, plan: Option<&Plan>, ui: &Value) -> Value {
    if let Some(id) = ui.get("open").and_then(Value::as_u64).map(|x| x as u32) {
        if let Some(e) = c.employee(id) {
            return person(c, plan, e);
        }
    }
    if c.staff.is_empty() {
        return json!({ "kind": "list", "empty": tr("Nobody works here yet. Every duty of a tour needs a driver; mechanics, cleaners, service employees and ticket inspectors help the company in their own ways. Hire some among the week's applicants."), "to_applicants": tr("To the applicants") });
    }
    let filter = ui.get("filter").and_then(Value::as_u64).unwrap_or(0).min(Role::ALL.len() as u64) as usize;
    let by = ui.get("sort").and_then(Value::as_u64).unwrap_or(0).min(SORTS.len() as u64 - 1) as usize;
    let desc = ui.get("descending").and_then(Value::as_bool).unwrap_or(false);
    let mut chips = vec![format!("{}  {}", tr("All"), c.staff.len())];
    chips.extend(Role::ALL.iter().map(|r| format!("{}  {}", tr(staff::plural(*r)), c.staff.iter().filter(|e| e.role == *r).count())));
    let mut list: Vec<Employee> = c.staff.iter().filter(|e| filter == 0 || Role::ALL.get(filter - 1) == Some(&e.role)).cloned().collect();
    sort(&mut list, by, desc);
    let asks: Vec<u32> = c.staff_rules.asks.iter().map(|a| a.id).collect();
    let cards: Vec<Value> = list
        .iter()
        .map(|e| {
            let (a, b, tone) = doing(c, plan, e);
            let tag = if e.notice_until.is_some() {
                Some(json!([tr(if e.resigned { "Resigned" } else { "Leaving" }), "danger"]))
            } else if asks.contains(&e.id) {
                Some(json!([tr("Asks for a raise"), "warn"]))
            } else if e.leave.pending().is_some() {
                Some(json!([tr("Asks for a holiday"), "warn"]))
            } else {
                None
            };
            json!({ "id": e.id, "portrait": e.portrait(), "name": e.name, "role": tr(e.role.label()), "icon": staff::icon(e.role), "wage": per_hour(e.wage), "tag": tag, "doing": [a, b, tone], "needs": needs(e) })
        })
        .collect();
    json!({
        "kind": "list", "chips": chips, "filter": filter, "sort_label": tr("Sort by"), "sorts": SORTS.iter().map(|x| tr(x)).collect::<Vec<_>>(), "sort": by, "descending": desc,
        "dir_tip": tr(if desc { "Descending" } else { "Ascending" }), "cards": cards,
    })
}

fn person(c: &Company, plan: Option<&Plan>, e: &Employee) -> Value {
    let (a, b, tone) = doing(c, plan, e);
    let g = Grade::of(e.experience);
    let rows = vec![
        json!({ "label": tr(g.label()), "value": num(e.experience, 0), "frac": e.experience / 100.0, "tone": grade_tone(g), "note": tr("Experience grows with every day worked.") }),
        json!({ "label": tr("Satisfaction"), "value": num(e.satisfaction, 0), "frac": e.satisfaction / 100.0, "tone": grade(e.satisfaction), "note": tr("Their pay against the market, their stress and their holidays. Under 25 they may leave.") }),
        json!({ "label": tr("Stress"), "value": num(e.stress, 0), "frac": e.stress / 100.0, "tone": grade(100.0 - e.stress), "note": tr("Overtime, nights, short rests and too few colleagues raise it; days off bring it down.") }),
        json!({ "label": tr("Friendliness"), "value": num(e.friendliness, 0), "frac": e.friendliness / 100.0, "tone": grade(e.friendliness), "note": tr("Grows with experience and the customer service course, falls with stress. The passengers feel it.") }),
    ];
    let market = staff::market_wage(e.role, e.experience, c.price_index);
    let pay = vec![json!([tr("A month"), eur(e.wage)]), json!([tr("By the hour"), per_hour(e.wage)]), json!([tr("The market pays"), eur(market)]), json!([tr("Costs a month"), eur(staff::monthly_cost(e))])];
    let ask = c.staff_rules.ask_of(e.id).cloned().map(|ask| trf("Asks for %{n} % more, until %{date}", &[("n", &format!("{:.0}", ask.share * 100.0)), ("date", &day_label(&ask.until))]));
    let mut facts: Vec<Value> = vec![json!([tr("Reliable"), format!("{:.0} %", e.reliability * 100.0)]), json!([tr("Days worked"), e.days_worked.to_string()]), json!([tr("Holiday days left"), e.holiday_left.to_string()])];
    // (their holidays asked for: the days not planned yet, the rejections of the year)
    let free = co::holidays::free_days(e, None);
    if free != e.holiday_left {
        facts.push(json!([tr("Not planned yet"), free.to_string()]));
    }
    let refused = e.leave.refused(&c.date);
    if refused > 0 {
        facts.push(json!([tr("Holidays rejected this year"), refused.to_string()]));
    }
    if staff::drives(e) {
        facts.insert(0, json!([tr("Licence"), licence_line(e.licence, &e.endorsements)]));
        facts.insert(1, json!([tr("Driving · punctuality · service"), format!("{:.0} · {:.0} · {:.0}", e.skills.driving, e.skills.punctuality, e.skills.service)]));
        facts.insert(2, json!([tr("Bus types"), e.types.len().to_string()]));
    } else {
        let hours = staff::shift_of(c, &c.date, e).map(|s| shift_text(&s)).unwrap_or_else(|| tr("Not at work"));
        facts.insert(0, json!([tr("Today"), hours]));
        facts.insert(1, json!([tr("Service skill"), format!("{:.0}", e.skills.service)]));
    }
    let courses: Vec<Value> = staff::courses_for(e.role)
        .iter()
        .copied()
        .map(|kind| {
            let state = if training::trained(c, Some(e.id), kind) {
                json!({ "done": true })
            } else if let Some(x) = training::course_of(c, Some(e.id), kind).filter(|x| !x.done) {
                json!({ "until": trf("Until %{date}", &[("date", &day_label(&x.until))]) })
            } else if !co::levels::unlocked(c, kind.spec().feature) {
                json!({ "lock": trf("From level %{n}", &[("n", &kind.spec().feature.level().to_string())]), "kind": format!("{kind:?}") })
            } else {
                json!({ "book": format!("{}  ·  {}", tr("Book"), eur(training::cost_of(c, kind))), "kind": format!("{kind:?}") })
            };
            json!({ "icon": kind.icon(), "label": tr(kind.label()), "tip": tr(kind.effect()), "state": state })
        })
        .collect();
    json!({
        "kind": "person", "id": e.id, "back": tr("All employees"), "portrait": e.portrait(), "name": e.name, "role": tr(e.role.label()),
        "since": trf("%{age}, since %{date}", &[("age", &e.age.to_string()), ("date", &day_label(&e.hired))]),
        "doing": [a, b, tone],
        "leaves": e.notice_until.as_deref().map(|u| trf(if e.resigned { "Resigned: leaves after %{date}" } else { "Leaves after %{date}" }, &[("date", &day_label(u))])),
        "needs_title": tr("Their needs").to_uppercase(), "needs": rows,
        "pay_title": tr("Pay").to_uppercase(), "pay": pay, "hourly_note": (!staff::drives(e)).then(|| tr("Paid by the hour: the night 50 % more, at least four hours a day worked.")),
        "ask": ask, "grant": tr("Grant the raise"), "refuse": tr("Refuse"), "raise": tr("A raise of 5 %"),
        "keep": (e.notice_until.is_some() && !e.resigned).then(|| tr("Take the dismissal back")), "dismiss": e.notice_until.is_none().then(|| tr("Dismiss")),
        "work_title": tr("Work").to_uppercase(), "facts": facts,
        "courses_title": tr("Courses").to_uppercase(), "courses": courses, "licences_note": staff::drives(e).then(|| tr("Licences and bus types: the Licences & training tab.")),
    })
}

fn hire_part(c: &Company, ui: &Value) -> Value {
    let market = staff::market(c);
    let pick = ui.get("hire_role").and_then(Value::as_u64).unwrap_or(0).min(Role::ALL.len() as u64 - 1) as usize;
    let chips: Vec<String> = Role::ALL.iter().map(|r| format!("{}  {}", tr(staff::plural(*r)), market.iter().filter(|a| a.role == *r).count())).collect();
    let role = Role::ALL[pick];
    let foot = trf("New applicants come on %{date}. Every role has its own market: wages around what the market pays for their experience.", &[("date", &day_label(&co::network::next_monday(c)))]);
    let mut out = json!({ "kind": "hire", "chips": chips, "pick": pick, "foot": foot });
    if let Err(why) = staff::hireable(c, role) {
        let easy = co::autopilot::on(c);
        let go = if easy { Go::ToNormal(RESEARCH_TAB) } else { Go::Research };
        out["locked"] = json!({ "why": tr(why), "more": easy.then(|| tr(NOT_RESEARCHED_BY_ITSELF)), "go": go.json() });
        return out;
    }
    let mine: Vec<Applicant> = market.into_iter().filter(|a| a.role == role).collect();
    out["advert"] = advert_part(c, role, mine.is_empty());
    let cards: Vec<Value> = mine
        .iter()
        .map(|a| {
            let g = Grade::of(a.experience);
            let market = staff::market_wage(a.role, a.experience, c.price_index);
            let vs = a.wage as f64 / market.max(1) as f64 - 1.0;
            let against = if vs.abs() < 0.015 { tr("as the market pays") } else if vs > 0.0 { trf("%{n} % over the market", &[("n", &format!("{:.0}", vs * 100.0))]) } else { trf("%{n} % under the market", &[("n", &format!("{:.0}", -vs * 100.0))]) };
            let bring = if a.role == Role::Driver {
                json!({ "licence": licence_line(a.licence, &a.endorsements), "d1": a.licence == Licence::D1, "licence_tip": endorsements_tip(a.licence, &a.endorsements), "skills": format!("{}  {:.0} · {:.0} · {:.0}", tr("Skills"), a.skills.driving, a.skills.punctuality, a.skills.service), "skills_tip": tr("Driving · punctuality · service") })
            } else {
                let f = a.friendliness();
                json!({ "friendliness": tr("Friendliness"), "value": num(f, 0), "tone": grade(f) })
            };
            json!({
                "no": a.no, "portrait": a.portrait(), "name": a.name, "who": format!("{}  ·  {}", tr(a.role.label()), trf("%{n} years old", &[("n", &a.age.to_string())])),
                "grade": tr(g.label()), "grade_tone": grade_tone(g), "frac": a.experience / 100.0, "grade_tip": trf("Experience %{n} of 100: Trainee, Novice, Experienced, Expert", &[("n", &num(a.experience, 0))]),
                "asks": format!("{}  ·  {}", trf("%{amount} a month", &[("amount", &eur(a.wage))]), per_hour(a.wage)), "against": against,
                "bring": bring, "reliable": format!("{}  {:.0} %", tr("Reliable"), a.reliability * 100.0), "reliable_tip": tr("How seldom they fall ill or come late"),
            })
        })
        .collect();
    out["cards"] = json!(cards);
    out["none"] = json!(tr("Nobody else applies for this work this week."));
    out["hire"] = json!(tr("Hire"));
    out
}

/// The job advert of a role (Beta 54: `staff::advertise`): its button, what it costs, why it
/// cannot be placed now; prominent when nobody is left to hire.
fn advert_part(c: &Company, role: Role, none_left: bool) -> Value {
    let fee = staff::advert_fee(c);
    let costs = if fee > 0 { trf("A job advert costs %{amount}: the applicants who are left make way for new ones at once.", &[("amount", &eur(fee))]) } else { tr("A job advert is free: the applicants who are left make way for new ones at once.") };
    let round = staff::round_of(c, role);
    json!({
        "label": tr("Advertise for new applicants"),
        "tip": costs,
        "prominent": none_left,
        "why": staff::may_advertise(c, role).err().map(tr),
        "round": (round > 0).then(|| trf("Job adverts this week: %{n}", &[("n", &round.to_string())])),
        "none_left": none_left.then(|| tr("Nobody is left to hire for this work this week. A job advert brings new applicants at once.")),
    })
}

/// A column of the training matrix: a licence or endorsement, a model's type training, or one of
/// the staff's courses.
#[derive(Clone, PartialEq)]
enum Col {
    Licence(CourseKind),
    Type(String),
    Course(CourseKind),
}

impl Col {
    fn kind(&self) -> CourseKind {
        match self {
            Col::Licence(k) | Col::Course(k) => *k,
            Col::Type(_) => CourseKind::TypeTraining,
        }
    }
    fn subject(&self) -> &str {
        match self {
            Col::Type(t) => t,
            _ => "",
        }
    }
    fn title(&self, c: &Company) -> String {
        match self {
            Col::Type(t) => licences::type_name(c, t),
            _ => tr(self.kind().label()),
        }
    }
    fn cost(&self, c: &Company) -> co::Cents {
        match self {
            Col::Course(k) => training::cost_of(c, *k),
            _ => licences::course_cost(c, self.kind()),
        }
    }
    fn has(&self, c: &Company, e: &Employee) -> bool {
        match self {
            Col::Course(k) => training::trained(c, Some(e.id), *k),
            _ => licences::has(e, self.kind(), self.subject()),
        }
    }
    fn booked<'a>(&self, c: &'a Company, e: &Employee) -> Option<&'a training::Course> {
        match self {
            Col::Course(k) => training::course_of(c, Some(e.id), *k).filter(|x| !x.done),
            _ => licences::booked(c, e.id, self.kind(), self.subject()),
        }
    }
}

fn columns(c: &Company, group: usize) -> Vec<Col> {
    match group {
        0 => CourseKind::LICENCES.iter().map(|k| Col::Licence(*k)).collect(),
        1 => licences::fleet_types(c).into_iter().map(Col::Type).collect(),
        _ => CourseKind::STAFF.iter().map(|k| Col::Course(*k)).collect(),
    }
}

fn course_terms(cost: co::Cents, k: CourseKind) -> String {
    let days = k.spec().days;
    let d = if days == 1 { tr("1 day") } else { trf("%{n} days", &[("n", &days.to_string())]) };
    format!("{}  ·  {}", eur(cost), d)
}

/// The matrix of the licences, the type trainings or the courses (`people::training`): its group,
/// its columns (`first`, `per` of them), the drivers' rows.
fn training_part(c: &Company, ui: &Value) -> Value {
    let foot = tr("A course begins today and the driver is away for its days. New drivers learn the fleet's models when they start; a model new to the company needs its type training first.");
    if !c.staff.iter().any(staff::drives) {
        return json!({ "kind": "training", "nobody": tr("Nobody works here yet. Every duty of a tour needs a driver: hire some among the week's applicants."), "foot": foot });
    }
    let group = ui.get("group").and_then(Value::as_u64).unwrap_or(0).min(2) as usize;
    let all = columns(c, group);
    let groups = vec![tr("Licences"), tr("Bus types"), tr("Courses")];
    if all.is_empty() {
        return json!({ "kind": "training", "groups": groups, "group": group, "none": tr("The fleet has no buses yet."), "foot": foot });
    }
    let per = (ui.get("per").and_then(Value::as_u64).unwrap_or(6) as usize).clamp(1, all.len());
    let first = (ui.get("first").and_then(Value::as_u64).unwrap_or(0) as usize).min(all.len() - per);
    let shown: Vec<Col> = all[first..first + per].to_vec();
    let people: Vec<&Employee> = c.staff.iter().filter(|e| e.notice_until.is_none() && staff::drives(e)).collect();
    let heads: Vec<Value> = shown
        .iter()
        .enumerate()
        .map(|(k, col)| {
            let kind = col.kind();
            let open = co::levels::unlocked(c, kind.spec().feature);
            let terms = course_terms(col.cost(c), kind);
            let have = people.iter().filter(|e| col.has(c, e)).count();
            json!({
                "k": k, "kind": format!("{kind:?}"), "icon": if matches!(col, Col::Type(_)) { "directions_bus" } else { kind.icon() }, "open": open, "title": col.title(c),
                "under": if matches!(col, Col::Type(_)) { format!("{}  ·  {terms}", tr("Type training")) } else { terms },
                "count": trf("%{n} of %{m} have it", &[("n", &have.to_string()), ("m", &people.len().to_string())]), "all": have == people.len(),
                "tip": tr(kind.effect()),
                "lock": (!open).then(|| trf("From level %{n}", &[("n", &kind.spec().feature.level().to_string())])),
                "train_all": (open && have < people.len()).then(|| tr("Train all")),
            })
        })
        .collect();
    let rows: Vec<Value> = people
        .iter()
        .map(|e| {
            let away = e.absent(&c.date);
            let cells: Vec<Value> = shown
                .iter()
                .map(|col| {
                    let kind = col.kind();
                    if col.has(c, e) {
                        return json!({ "has": true });
                    }
                    match col.booked(c, e) {
                        Some(x) => json!({ "until": trf("Until %{date}", &[("date", &day_label(&x.until))]) }),
                        None if !co::levels::unlocked(c, kind.spec().feature) => json!({ "lock": true }),
                        None if e.licence == Licence::D1 && matches!(kind, CourseKind::ArticulatedLicence | CourseKind::DoubleDeckerLicence) => json!({ "text": tr("D licence first") }),
                        None if away => json!({ "text": tr("Away") }),
                        None => json!({ "book": tr("Book") }),
                    }
                })
                .collect();
            json!({ "id": e.id, "name": e.name, "licence": licence_line(e.licence, &e.endorsements), "cells": cells })
        })
        .collect();
    json!({
        "kind": "training", "groups": groups, "group": group, "first": first, "per": per, "total": all.len(),
        "paging": (all.len() > per).then(|| trf("%{a}-%{b} of %{n}", &[("a", &(first + 1).to_string()), ("b", &(first + per).to_string()), ("n", &all.len().to_string())])),
        "driver": tr("Driver").to_uppercase(), "heads": heads, "rows": rows, "foot": foot,
        "prev_tip": tr("Earlier columns"), "next_tip": tr("More columns"),
    })
}

fn schedule_part(c: &Company, ui: &Value) -> Value {
    let pick = ui.get("hours_role").and_then(Value::as_u64).unwrap_or(0).min(staff::OTHERS.len() as u64 - 1) as usize;
    let chips: Vec<String> = staff::OTHERS.iter().map(|r| format!("{}  {}", tr(staff::plural(*r)), c.staff.iter().filter(|e| e.role == *r).count())).collect();
    let role = staff::OTHERS[pick];
    let open = co::research::has(c, keys::SHIFT_SCHEDULE);
    let h = c.staff_rules.hours_of(role);
    let have = c.staff.iter().filter(|e| e.role == role && e.notice_until.is_none()).count() as u32;
    let (shifts, uncovered) = staff::shifts(c, &c.date, role);
    let all = c.staff.iter().filter(|e| e.role == role && e.employed_on(&c.date)).count();
    let paid: f64 = shifts.iter().map(|s| s.paid() as f64 / 60.0).sum();
    let night: f64 = shifts.iter().map(|s| s.night() as f64 / 60.0).sum();
    let wages: f64 = shifts.iter().filter_map(|s| c.employee(s.id).map(|e| staff::hourly(e.wage) * (s.paid() as f64 + staff::NIGHT_SUPPLEMENT * s.night() as f64) / 60.0)).sum::<f64>() * (1.0 + co::economy::EMPLOYER_SHARE);
    let asked = h.outside as i32 * (1440 - h.length());
    let mut rows: Vec<Value> = vec![json!([tr("At work"), format!("{} / {}", shifts.len(), all)]), json!([tr("Hours paid"), format!("{} h", num(paid, 1))]), json!([tr("Of them at night"), format!("{} h", num(night, 1))]), json!([tr("Wages today"), eur(wages.round() as i64)])];
    if asked > 0 {
        rows.push(json!([tr("Outside the core hours covered"), format!("{:.0} %", 100.0 * (asked - uncovered) as f64 / asked as f64)]));
    }
    let last = c.staff_rules.last.clone().unwrap_or_default();
    let what = match role {
        Role::Mechanic => Some(json!([tr("In-house maintenance and repairs"), format!("{:.0} % (≤ {:.0} %)", 100.0 * staff::workshop_share(c, &c.date), 100.0 * staff::WORKSHOP_MAX)])),
        Role::Cleaner if !last.date.is_empty() => Some(json!([tr("Buses cleaned last night"), format!("{}  ·  {:.0} %", last.cleaned, last.clean)])),
        Role::Service if !last.date.is_empty() => Some(json!([tr("Complaints answered yesterday"), format!("{} / {}", last.handled, last.complaints)])),
        Role::Inspector => {
            let lines: Vec<String> = staff::inspected(c, &c.date).iter().filter_map(|x| c.lines.iter().find(|l| l.name == x.0).map(|l| l.number.clone())).collect();
            Some(json!([tr("Lines checked today"), if lines.is_empty() { "–".into() } else { lines.join(", ") }]))
        }
        _ => None,
    };
    rows.extend(what);
    let each: Vec<Value> = shifts.iter().map(|s| json!([c.employee(s.id).map(|e| e.name.clone()).unwrap_or_default(), shift_text(s), s.extra.is_some()])).collect();
    let easy = co::autopilot::on(c);
    json!({
        "kind": "schedule", "chips": chips, "pick": pick, "open": open,
        "core": tr("Core hours"), "from": h.from, "to": h.to, "from_text": co::clock::hhmm(h.from as i64), "to_text": co::clock::hhmm(h.to as i64),
        "all_there": tr("During the core hours all of them who work that day are there."),
        "outside_title": tr("Present outside the core hours"), "outside": h.outside, "most": have.max(h.outside).max(1),
        "notes": [
            tr("The least stressed stay on after the core hours, in turns: nobody works more than 12 hours a day."),
            tr("Between 20:00 and 06:00 they are paid 50 % more (night supplement)."),
            tr("Everyone is paid for at least 4 hours on a day they work: core hours shorter than that are not worth it."),
            tr("Each works five days a week; their days off follow one another round the week."),
        ],
        "locked": (!open).then(|| json!({ "text": tr("The working hours can be set once the company has researched the shift schedule."), "more": easy.then(|| tr(NOT_RESEARCHED_BY_ITSELF)), "go": if easy { Go::ToNormal(RESEARCH_TAB).json() } else { Go::Research.json() } })),
        "today_title": tr("Today").to_uppercase(), "rows": rows, "each_title": tr("Hours of each").to_uppercase(), "each": each,
    })
}

fn settings_part(c: &Company) -> Value {
    let rules = c.staff_rules.clone();
    let roles: Vec<Value> = Role::ALL
        .iter()
        .map(|role| {
            let have = c.staff.iter().filter(|e| e.role == *role && e.notice_until.is_none()).count();
            json!({ "role": format!("{role:?}"), "icon": staff::icon(*role), "name": tr(staff::plural(*role)), "n": rules.minimum_of(*role), "now": trf("%{n} now", &[("n", &have.to_string())]) })
        })
        .collect();
    json!({
        "kind": "settings",
        "auto": rules.auto_hire, "auto_label": tr("Hire automatically to keep a minimum of each role"),
        "auto_note": tr("At night the best applicant of the week is hired for a role that has fewer than its minimum - one a night for each role, while the cash allows."),
        "roles": roles, "fewer": tr("Fewer"), "more": tr("More"),
        "follow": rules.follow_prices, "follow_label": tr("Wages rise with the prices at every month's end"),
        "follow_note": tr("Satisfaction follows a wage against what the market pays, and the market rises with the prices: without this, everyone earns a little less each month."),
        "grant": rules.grant_small, "grant_label": tr("Grant requests for a raise of up to 5 % without asking"),
        "grant_note": tr("Someone who earns less than the market and is not satisfied asks for a raise now and then. Larger requests wait for your answer on their card; no answer within a week is a no."),
    })
}

/// The Staff page: its tabs (the easy mode the first two, unless a way led to another), the wages
/// and today's duties beside them, and the tab shown.
pub fn view(s: &mut Session, ui: &Value) -> Value {
    let Some(c) = s.company.clone() else { return Value::Null };
    let tab = ui.get("tab").and_then(Value::as_u64).unwrap_or(0).min(TABS.len() as u64 - 1) as usize;
    let shown = if co::autopilot::on(&c) && tab < 2 { 2 } else { TABS.len() };
    let tabs: Vec<String> = TABS.iter().enumerate().take(shown).map(|(k, t)| if k == 0 { format!("{} ({})", tr(t), c.staff.len()) } else { tr(t) }).collect();
    let wages: i64 = c.staff.iter().map(staff::monthly_cost).sum();
    let duties: usize = s.plan.as_ref().map(|p| p.tours.iter().filter(|t| !t.by_player && !t.live && !t.tour.unplanned).map(|t| t.duties.len()).sum()).unwrap_or(0);
    let plan = s.plan.clone();
    // (the Agencies tab's day picked: its open duties, when its timetable is read)
    let aplan = if tab == 5 { agencies::plan_for(s, ui) } else { None };
    let body = match tab {
        1 => hire_part(&c, ui),
        2 => training_part(&c, ui),
        3 => schedule_part(&c, ui),
        4 => holidays::part(&c, ui),
        5 => agencies::part(&c, aplan.as_ref(), ui),
        6 => settings_part(&c),
        _ => list_part(&c, plan.as_ref(), ui),
    };
    json!({ "tabs": tabs, "tab": tab, "info": trf("Wages %{amount} a month with the employer's share  ·  %{n} duties today", &[("amount", &eur(wages)), ("n", &duties.to_string())]), "body": body })
}

fn role_of(name: &str) -> Option<Role> {
    Role::ALL.iter().copied().find(|r| format!("{r:?}") == name)
}

/// What the Staff page's buttons do.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    let done = |r: Result<(), Value>| Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})));
    Some(match what {
        "hire" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            let Some(a) = staff::market(&c).into_iter().find(|a| a.no == no) else { return Some(Ok(json!({}))) };
            match apply(s, |c| staff::hire(c, &a)) {
                Ok(_) => {
                    let t = trf("%{name} works for %{company} from today.", &[("name", &a.name), ("company", &c.name)]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "raise" => done(apply(s, |c| {
            staff::raise(c, id, 0.05);
            Ok(())
        })),
        // (a job advert for the role shown: new applicants at once - Beta 54)
        "advertise" => {
            let k = args.get("role").and_then(Value::as_u64).unwrap_or(0).min(Role::ALL.len() as u64 - 1) as usize;
            let role = Role::ALL[k];
            match apply(s, |c| staff::advertise(c, role)) {
                Ok(n) => {
                    let t = trf("%{n} new applicants answered the job advert.", &[("n", &n.to_string())]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "keep" => done(apply(s, |c| staff::withdraw_notice(c, id)).map(|_| ())),
        "grant_raise" => done(apply(s, |c| staff::answer_raise(c, id, true)).map(|_| ())),
        "refuse_raise" => done(apply(s, |c| staff::answer_raise(c, id, false)).map(|_| ())),
        "course_lock" => {
            let Some(kind) = super::career::course_of(args.get("kind").and_then(Value::as_str).unwrap_or("")) else { return Some(Ok(json!({}))) };
            Ok(json!({ "popup": super::popup(s, kit::locked(&c, kind.spec().feature)) }))
        }
        "book_course" => {
            let Some(kind) = super::career::course_of(args.get("kind").and_then(Value::as_str).unwrap_or("")) else { return Some(Ok(json!({}))) };
            match apply(s, |c| training::enrol(c, kind, Some(id))) {
                Ok(until) => {
                    let t = trf("Booked: %{course}, until %{date}.", &[("course", &tr(kind.label())), ("date", &day_label(&until))]);
                    s.say(t, false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "train" => {
            // a column of the matrix booked for one driver (`id`) or for all who lack it
            let group = args.get("group").and_then(Value::as_u64).unwrap_or(0) as usize;
            let k = args.get("col").and_then(Value::as_u64).unwrap_or(0) as usize;
            let Some(col) = columns(&c, group).get(k).cloned() else { return Some(Ok(json!({}))) };
            let kind = col.kind();
            if !co::levels::unlocked(&c, kind.spec().feature) {
                return Some(Ok(json!({ "popup": super::popup(s, kit::locked(&c, kind.spec().feature)) })));
            }
            let course = if matches!(col, Col::Type(_)) { format!("{} {}", tr("Type training"), col.title(&c)) } else { col.title(&c) };
            let subject = col.subject().to_string();
            let staff_course = matches!(col, Col::Course(_));
            match args.get("id").and_then(Value::as_u64).map(|x| x as u32) {
                Some(id) => {
                    let r = if staff_course { apply(s, |c| training::enrol(c, kind, Some(id))) } else { apply(s, |c| licences::enrol(c, id, kind, &subject)) };
                    match r {
                        Ok(until) => {
                            let t = trf("Booked: %{course}, until %{date}.", &[("course", &course), ("date", &day_label(&until))]);
                            s.say(t, false);
                            Ok(json!({}))
                        }
                        Err(p) => Ok(json!({ "popup": p })),
                    }
                }
                None => {
                    let cost = col.cost(&c);
                    if !c.can_pay(cost) {
                        return Some(Ok(json!({ "popup": super::popup(s, kit::no_cash(&c, cost)) })));
                    }
                    let r = if staff_course { apply(s, |c| Ok(training::enrol_all(c, kind))) } else { apply(s, |c| Ok(licences::enrol_all(c, kind, &subject))) };
                    match r {
                        Ok((0, _)) => Ok(json!({ "popup": super::popup(s, Popup::new("groups", "Nobody could be booked", tr("Who lacks it is away today, on another course or leaving the company - or needs the D licence first."), "", None)) })),
                        Ok((n, spent)) => {
                            let t = trf("Booked for %{n} drivers: %{course}, %{amount} together.", &[("n", &n.to_string()), ("course", &course), ("amount", &eur(spent))]);
                            s.say(t, false);
                            Ok(json!({}))
                        }
                        Err(p) => Ok(json!({ "popup": p })),
                    }
                }
            }
        }
        "set_hours" => {
            let pick = args.get("role").and_then(Value::as_u64).unwrap_or(0) as usize;
            let Some(role) = staff::OTHERS.get(pick).copied() else { return Some(Ok(json!({}))) };
            let h = c.staff_rules.hours_of(role);
            let from = args.get("from").and_then(Value::as_i64).map(|x| x as i32).unwrap_or(h.from);
            let to = args.get("to").and_then(Value::as_i64).map(|x| x as i32).unwrap_or(h.to);
            let outside = args.get("outside").and_then(Value::as_u64).map(|x| x as u32).unwrap_or(h.outside);
            if from == to {
                return Some(Ok(json!({})));
            }
            done(apply(s, |c| staff::set_hours(c, role, Hours { from, to, outside })).map(|_| ()))
        }
        "staff_rule" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
            let which = args.get("which").and_then(Value::as_str).unwrap_or("").to_string();
            done(apply(s, |c| {
                match which.as_str() {
                    "auto" => c.staff_rules.auto_hire = on,
                    "follow" => c.staff_rules.follow_prices = on,
                    _ => c.staff_rules.grant_small = on,
                }
                Ok(())
            }))
        }
        "holiday_answer" | "holiday_limit" => return holidays::act(s, what, args),
        "agency_book" | "agency_cancel" => return agencies::act(s, what, args),
        "staff_minimum" => {
            let Some(role) = role_of(args.get("role").and_then(Value::as_str).unwrap_or("")) else { return Some(Ok(json!({}))) };
            let n = args.get("n").and_then(Value::as_u64).unwrap_or(0).min(99) as u32;
            done(apply(s, |c| {
                c.staff_rules.set_minimum(role, n);
                Ok(())
            }))
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{found, Founding, GameMode};

    #[test]
    fn the_list_is_sorted_by_role_and_wage() {
        let mut c = found(&Founding { name: "Liste".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        for role in [Role::Driver, Role::Cleaner, Role::Mechanic] {
            let a = staff::applicants_of(&c, role)[0].clone();
            staff::hire(&mut c, &a).unwrap();
        }
        let mut list = c.staff.clone();
        sort(&mut list, 1, false);
        assert_eq!(list.iter().map(|e| e.role).collect::<Vec<_>>(), vec![Role::Driver, Role::Mechanic, Role::Cleaner]);
        super::super::words::set_language("en");
        assert_eq!(doing(&c, None, &c.staff[0]).0, "Free today");
        assert_eq!(span(4 * 60 + 35, 12 * 60 + 40), "04:35 - 12:40");
    }
}
