//! The boss's side of a company session: who is online and what each drives, the notices
//! everyone sees, and the answer to every message a friend's launcher posts (`HostState::answer`),
//! worked out on the company itself - the boss's launcher hands its company, the day's plan and
//! timetable and its game's session (`Here`), and saves the company when the answer changed it.

use super::super::clock;
use super::super::duty_launch::{self as dl, DutyRef, Hitch};
use super::super::model::Company;
use super::super::plan::{BusOf, DayPlan, Who};
use super::book::{self, ClaimState};
use super::code;
use super::wire::{Card, DriveStart, DutyState, Notice, Request, SnapDuty, SnapLine, SnapPlayer, SnapResult, Snapshot};
use serde_json::{json, Value};

/// A player who has not been heard from for this long is offline (s).
pub const ONLINE_FOR: u64 = 20;
/// How many notices are kept.
const NOTICES_KEPT: usize = 80;
/// The duties that ended longer ago than this are left out of the friend's day (minutes).
const ENDED_SHOWN: i32 = 60;

/// Why a message is not answered (English: the keys of the translations).
pub const BAD_SECRET: &str = "The code is no longer valid: ask the boss for the code of his session.";
pub const WORLD_CLOSED: &str = "The boss's world isn't open yet.";
pub const WORLD_MAP: &str = "The boss's game runs another map: the company's world has to be open.";
pub const READING: &str = "The boss's launcher is still reading the day's timetable: try again in a moment.";
pub const UNKNOWN: &str = "The boss's launcher did not understand that: both players need the same version of Omsi-Hub.";
pub const NOT_OPEN_COMPANY: &str = "The boss does not have the company open right now.";

/// What is told to everyone (English: the keys of the translations).
pub const JOINED: &str = "%{name} joined the company.";
pub const LEFT: &str = "%{name} left.";
pub const TOOK: &str = "%{name} took duty %{k} of line %{n} (tour %{tour}).";
pub const ASKS: &str = "%{name} asks for duty %{k} of line %{n} (tour %{tour}).";
pub const GAVE: &str = "%{name} has duty %{k} of line %{n} (tour %{tour}) now.";
pub const REFUSED: &str = "No: duty %{k} of line %{n} (tour %{tour}) is not %{name}'s.";
pub const BACK: &str = "%{name} gave back duty %{k} of line %{n} (tour %{tour}).";
pub const OFF: &str = "%{name} is off duty %{k} of line %{n} (tour %{tour}).";
pub const WAITS: &str = "%{name} wants to drive duty %{k} of line %{n}: the boss's world isn't open yet.";
pub const DRIVES: &str = "%{name} drives duty %{k} of line %{n} (tour %{tour}).";
pub const FINISHED: &str = "%{name} finished duty %{k} of line %{n}: %{trips} trips, score %{score}.";
pub const SAYS: &str = "%{name}: %{text}";
pub const WORLD_OPEN: &str = "The boss's world is open: duties can be driven.";

/// Every English text of this file (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    vec![BAD_SECRET, WORLD_CLOSED, WORLD_MAP, READING, UNKNOWN, NOT_OPEN_COMPANY, JOINED, LEFT, TOOK, ASKS, GAVE, REFUSED, BACK, OFF, WAITS, DRIVES, FINISHED, SAYS, WORLD_OPEN]
}

/// A player as the session knows him now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Online {
    pub guest: u32,
    pub name: String,
    /// When he was last heard from (Unix seconds).
    pub seen: u64,
    pub driving: Option<DutyRef>,
    pub in_game: bool,
    /// The duty he wants to drive, waiting for the boss's world.
    pub waiting: Option<DutyRef>,
}

/// The session's own: its secret, the company, the players heard from and the notices.
#[derive(Clone, Debug, Default)]
pub struct HostState {
    pub secret: [u8; 8],
    pub company: String,
    pub boss: String,
    pub online: Vec<Online>,
    pub notices: Vec<Notice>,
    next: u64,
}

/// What the boss's launcher knows besides the company.
pub struct Here<'a> {
    /// The company's day (None: its timetable is still being read).
    pub plan: Option<&'a DayPlan>,
    pub lines: &'a [crate::LineInfo],
    /// The boss's game hosting a session: its code and its map.
    pub world: Option<(String, String)>,
    /// Unix seconds.
    pub now: u64,
    /// The bus's lead before a duty's first departure (minutes, the boss's setting).
    pub lead: i64,
}

/// An answer, and whether the company changed (to be saved).
pub struct Answered {
    pub body: Value,
    pub changed: bool,
}

/// An answer that refuses, with what is wrong.
pub fn refusal(text: &str) -> Value {
    json!({ "ok": false, "error": text })
}

/// Equal secrets, compared in constant time.
fn same_secret(given: &str, secret: &[u8; 8]) -> bool {
    let want: Vec<u8> = secret.iter().flat_map(|b| format!("{b:02x}").into_bytes()).collect();
    let got = given.trim().to_ascii_lowercase().into_bytes();
    got.len() == want.len() && got.iter().zip(&want).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0
}

/// The arguments a duty is told with: its line's number, its tour and its number in the tour.
pub fn duty_args(p: Option<&DayPlan>, r: &DutyRef) -> Vec<(String, String)> {
    let number = p.and_then(|p| r.find(p).map(|(ti, _)| p.tours[ti].tour.number.clone())).unwrap_or_else(|| r.line.clone());
    vec![("n".into(), number), ("tour".into(), r.tour.clone()), ("k".into(), (r.duty + 1).to_string())]
}

impl HostState {
    pub fn new(company: &str, boss: &str, secret: [u8; 8]) -> HostState {
        HostState { secret, company: company.to_string(), boss: boss.to_string(), ..Default::default() }
    }

    /// Tell everyone something.
    pub fn tell(&mut self, now: u64, text: &str, args: Vec<(String, String)>) {
        self.next += 1;
        self.notices.push(Notice { n: self.next, at: now, text: text.to_string(), args });
        if self.notices.len() > NOTICES_KEPT {
            let extra = self.notices.len() - NOTICES_KEPT;
            self.notices.drain(..extra);
        }
    }

    /// Tell everyone something about player `name` and duty `r`.
    pub fn tell_duty(&mut self, now: u64, text: &str, name: &str, p: Option<&DayPlan>, r: &DutyRef) {
        let mut args = vec![("name".to_string(), name.to_string())];
        args.extend(duty_args(p, r));
        self.tell(now, text, args);
    }

    /// The players heard from lately.
    pub fn online_now(&self, now: u64) -> impl Iterator<Item = &Online> {
        self.online.iter().filter(move |o| now.saturating_sub(o.seen) <= ONLINE_FOR)
    }

    pub fn is_online(&self, guest: u32, now: u64) -> bool {
        self.online_now(now).any(|o| o.guest == guest)
    }

    /// The last notice's number.
    pub fn last_notice(&self) -> u64 {
        self.next
    }

    /// Answer a message of a friend's launcher (its body: a `Request` as JSON).
    pub fn answer(&mut self, c: &mut Company, here: &Here, body: &str) -> Answered {
        let before = c.together.clone();
        let body = self.answer_inner(c, here, body);
        Answered { changed: c.together != before, body }
    }

    fn answer_inner(&mut self, c: &mut Company, here: &Here, body: &str) -> Value {
        let Ok(req) = serde_json::from_str::<Request>(body) else { return refusal(UNKNOWN) };
        if req.v != code::VERSION {
            return refusal(code::NEWER);
        }
        if !same_secret(&req.secret, &self.secret) || req.company != code::tag_of(&self.company) {
            return refusal(BAD_SECRET);
        }
        if c.id != self.company {
            return refusal(NOT_OPEN_COMPANY);
        }
        let Some(guest) = c.together.guest_of(&req.player, &req.name) else { return refusal(book::TOO_MANY) };
        let name = c.together.name_of(guest);
        let now = here.now;
        // (who is here, and what he does)
        let leaving = req.what == "leave";
        let came = !self.is_online(guest, now);
        match self.online.iter_mut().find(|o| o.guest == guest) {
            Some(o) => {
                o.seen = now;
                o.name = name.clone();
                o.driving = req.driving.clone();
                o.in_game = req.in_game;
            }
            None => self.online.push(Online { guest, name: name.clone(), seen: now, driving: req.driving.clone(), in_game: req.in_game, waiting: None }),
        }
        if came && !leaving {
            self.tell(now, JOINED, vec![("name".into(), name.clone())]);
        }
        let p = here.plan;
        let clock_now = clock::now(c);
        let mut extra = json!({});
        let result: Result<(), &'static str> = match req.what.as_str() {
            "hello" | "snapshot" => Ok(()),
            "take" | "release" | "start" | "report" if p.is_none() => Err(READING),
            "take" => match req.duty.as_ref() {
                None => Err(book::NO_DUTY),
                Some(r) => book::take(c, p.unwrap(), r, guest, clock_now).map(|state| {
                    let text = if state == ClaimState::Asked { ASKS } else { TOOK };
                    self.tell_duty(now, text, &name, p, r);
                    extra = json!({ "state": state });
                }),
            },
            "release" => match req.duty.as_ref() {
                None => Err(book::NO_DUTY),
                Some(r) => book::release(c, r, Some(guest)).map(|_| self.tell_duty(now, BACK, &name, p, r)),
            },
            "start" => match req.duty.as_ref() {
                None => Err(book::NO_DUTY),
                Some(r) => self.start(c, here, r, guest, &name).map(|s| extra = json!({ "start": s })),
            },
            "report" => match req.duty.as_ref() {
                None => Err(book::NO_DUTY),
                Some(r) => book::report(c, p.unwrap(), r, guest, &req.trips, now).map(|(taken, refused)| {
                    extra = json!({ "taken": taken, "refused": refused });
                    if req.done && book::set_state(c, r, guest, ClaimState::Done).is_ok() {
                        let res = result_of(c, p, r);
                        let mut args = vec![("name".to_string(), name.clone())];
                        args.extend(duty_args(p, r));
                        args.push(("trips".into(), res.trips.to_string()));
                        args.push(("score".into(), res.score.to_string()));
                        self.tell(now, FINISHED, args);
                        if let Some(o) = self.online.iter_mut().find(|o| o.guest == guest) {
                            o.driving = None;
                        }
                    }
                }),
            },
            "say" => {
                let text = book::clean(&req.text, 160);
                if !text.is_empty() {
                    self.tell(now, SAYS, vec![("name".into(), name.clone()), ("text".into(), text)]);
                }
                Ok(())
            }
            "leave" => {
                // (what he only asked for or was planned on goes back; what he drives stays)
                let mine: Vec<DutyRef> = c.together.claims.iter().filter(|x| x.guest == guest && matches!(x.state, ClaimState::Asked | ClaimState::Planned)).map(|x| x.duty.clone()).collect();
                for r in &mine {
                    let _ = book::release(c, r, Some(guest));
                }
                self.online.retain(|o| o.guest != guest);
                self.tell(now, LEFT, vec![("name".into(), name.clone())]);
                return json!({ "ok": true });
            }
            _ => Err(UNKNOWN),
        };
        let snap = snapshot(c, here, self, Some(guest), req.since);
        let mut out = match result {
            Ok(()) => json!({ "ok": true }),
            Err(e) => refusal(e),
        };
        if let (Some(o), Some(x)) = (out.as_object_mut(), extra.as_object()) {
            o.extend(x.clone());
        }
        out["you"] = json!(guest);
        out["snapshot"] = serde_json::to_value(snap).unwrap_or(Value::Null);
        out
    }

    /// A player starts his duty `r`: its start in the boss's world, or why not.
    fn start(&mut self, c: &mut Company, here: &Here, r: &DutyRef, guest: u32, name: &str) -> Result<DriveStart, &'static str> {
        let p = here.plan.ok_or(READING)?;
        let x = c.together.claim_of(r).filter(|x| x.guest == guest).ok_or(book::NOT_YOURS)?;
        match x.state {
            ClaimState::Asked => return Err(book::NOT_YOURS),
            ClaimState::Done => return Err(book::DONE),
            _ => {}
        }
        let mut sheet = dl::sheet(c, p, r, clock::now(c), &[]);
        // (planned for him: not another's)
        sheet.hitches.retain(|h| !matches!(h, Hitch::NotYours(_) | Hitch::NoDriver));
        let st = dl::start(&sheet, here.lines, here.lead).map_err(|h| h.words().0)?;
        let Some((game, map)) = here.world.clone() else {
            if let Some(o) = self.online.iter_mut().find(|o| o.guest == guest) {
                o.waiting = Some(r.clone());
            }
            self.tell_duty(here.now, WAITS, name, Some(p), r);
            return Err(WORLD_CLOSED);
        };
        if !same_map(&map, &c.map) {
            return Err(WORLD_MAP);
        }
        book::set_state(c, r, guest, ClaimState::Driving)?;
        if let Some(o) = self.online.iter_mut().find(|o| o.guest == guest) {
            o.waiting = None;
            o.driving = Some(r.clone());
        }
        self.tell_duty(here.now, DRIVES, name, Some(p), r);
        Ok(DriveStart::of(&st, &game))
    }
}

/// Maps named alike (`maps/X/global.cfg`, from `maps/` on, either slash, any case).
pub fn same_map(a: &str, b: &str) -> bool {
    let n = |s: &str| {
        let s = s.trim().replace('\\', "/").to_ascii_lowercase();
        match s.rfind("maps/") {
            Some(i) => s[i..].to_string(),
            None => s,
        }
    };
    !a.trim().is_empty() && n(a) == n(b)
}

/// What a player's trips of duty `r` came to.
pub fn result_of(c: &Company, p: Option<&DayPlan>, r: &DutyRef) -> SnapResult {
    let mut res = SnapResult { duty: r.clone(), number: duty_args(p, r)[0].1.clone(), ..Default::default() };
    let mut total = 0u32;
    for t in book::trips_of(c, r) {
        let ev = super::super::career::evaluate(&t.trip);
        res.trips += 1;
        res.passengers += t.trip.passengers;
        res.km += super::super::career::believable_km(&t.trip);
        res.xp += ev.xp;
        res.on_time += t.trip.on_time().max(0) as u32;
        res.stops += t.trip.stops.max(0) as u32;
        total += ev.score;
    }
    if res.trips > 0 {
        res.score = total / res.trips as u32;
    }
    res.done = c.together.claim_of(r).is_some_and(|x| x.state == ClaimState::Done);
    res
}

/// Who drives a duty of the company, as a friend's page names him (English for the agency).
fn who_of(c: &Company, w: Option<Who>, boss: &str) -> String {
    match w {
        Some(Who::Staff(id)) => c.employee(id).map(|e| e.name.split_whitespace().next().unwrap_or(&e.name).to_string()).unwrap_or_default(),
        Some(Who::Agency) => "Agency driver".into(),
        Some(Who::Player) => boss.to_string(),
        Some(Who::Guest(g)) => c.together.name_of(g),
        None => String::new(),
    }
}

/// The company's day as player `guest` sees it (None: the boss's own view of it), with the
/// notices after `since`.
pub fn snapshot(c: &Company, here: &Here, st: &HostState, guest: Option<u32>, since: u64) -> Snapshot {
    let mut s = Snapshot {
        card: Card { id: c.id.clone(), name: c.name.clone(), short: c.short.clone(), colours: c.colours.clone(), map: c.map.clone(), map_name: c.map_name.clone(), boss: st.boss.clone() },
        date: c.date.clone(),
        minute: c.clock.minute as i32,
        open_claims: c.together.open_claims,
        world: here.world.as_ref().is_some_and(|(_, m)| same_map(m, &c.map)),
        lines: c.lines.iter().map(|l| SnapLine { line: l.name.clone(), number: l.number.clone(), colour: l.colour.clone() }).collect(),
        reading: here.plan.is_none(),
        ..Default::default()
    };
    let now = clock::now(c);
    if let Some(p) = here.plan {
        for (ti, t) in p.tours.iter().enumerate() {
            if t.tour.unplanned {
                continue;
            }
            let colour = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&t.tour.line)).map(|l| l.colour.clone()).unwrap_or_default();
            let bus = match t.bus {
                Some(BusOf::Own(id)) => c.vehicle(id).map(|v| format!("{} {}", v.number, v.name).trim().to_string()).unwrap_or_default(),
                Some(BusOf::Rental) => "Rental bus".into(),
                None => String::new(),
            };
            for (k, d) in t.duties.iter().enumerate() {
                let Some(r) = DutyRef::of(p, ti, k) else { continue };
                let claim = c.together.claim_of(&r);
                let mine = claim.is_some_and(|x| Some(x.guest) == guest);
                if !mine && now >= clock::moment(&p.date, (d.to + ENDED_SHOWN) as i64) {
                    continue;
                }
                let trips = &t.tour.trips[d.start..d.end];
                let mut sd = SnapDuty {
                    duty: r.clone(),
                    number: t.tour.number.clone(),
                    colour: colour.clone(),
                    from: d.from,
                    to: d.to,
                    first: trips.first().map(|x| x.from.clone()).unwrap_or_default(),
                    last: trips.last().map(|x| x.end().to_string()).unwrap_or_default(),
                    trips: trips.iter().filter(|x| x.counts()).count(),
                    km: trips.iter().map(|x| x.km).sum::<f64>().round(),
                    bus: bus.clone(),
                    ..Default::default()
                };
                let begun = now >= clock::moment(&p.date, d.from as i64);
                (sd.state, sd.who, sd.why) = match claim {
                    Some(x) if mine => (
                        match x.state {
                            ClaimState::Planned => DutyState::Mine,
                            ClaimState::Asked => DutyState::Asked,
                            ClaimState::Driving => DutyState::Driving,
                            ClaimState::Done => DutyState::Done,
                        },
                        c.together.name_of(x.guest),
                        String::new(),
                    ),
                    Some(x) => (DutyState::Player, c.together.name_of(x.guest), String::new()),
                    None if t.by_player => (DutyState::Closed, String::new(), book::DRIVEN.into()),
                    None if !matches!(t.bus, Some(BusOf::Own(_))) => (DutyState::Closed, String::new(), book::NO_BUS.into()),
                    None if d.who.is_some() => (DutyState::Company, who_of(c, d.who, &st.boss), String::new()),
                    None if begun => (DutyState::Closed, String::new(), book::BEGUN.into()),
                    None => (DutyState::Open, String::new(), String::new()),
                };
                s.duties.push(sd);
            }
        }
        s.duties.sort_by(|a, b| (a.from, &a.number, &a.duty.tour).cmp(&(b.from, &b.number, &b.duty.tour)));
    }
    // (the boss first, then the players online, then those with a duty today)
    s.players.push(SnapPlayer { name: st.boss.clone(), boss: true, online: true, ..Default::default() });
    let label = |r: &DutyRef| (duty_args(here.plan, r)[0].1.clone(), r.tour.clone(), r.duty + 1);
    for o in st.online_now(here.now) {
        s.players.push(SnapPlayer { name: o.name.clone(), me: Some(o.guest) == guest, online: true, driving: o.driving.as_ref().map(label), boss: false });
    }
    for g in &c.together.guests {
        let busy = c.together.claims.iter().any(|x| x.guest == g.no && x.duty.date == c.date);
        if busy && !st.is_online(g.no, here.now) {
            s.players.push(SnapPlayer { name: g.name.clone(), me: Some(g.no) == guest, online: false, ..Default::default() });
        }
    }
    let newer: Vec<&Notice> = st.notices.iter().filter(|n| n.n > since).collect();
    s.notices = newer[newer.len().saturating_sub(30)..].iter().map(|n| (*n).clone()).collect();
    if let Some(g) = guest {
        let mut duties: Vec<DutyRef> = Vec::new();
        for t in c.together.trips.iter().filter(|t| t.guest == g) {
            if !duties.iter().any(|d| book::same_duty(d, &t.duty)) {
                duties.push(t.duty.clone());
            }
        }
        s.results = duties.iter().map(|r| result_of(c, here.plan, r)).collect();
    }
    s
}
