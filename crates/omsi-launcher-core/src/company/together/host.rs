//! The boss's side of a company session: who is online and what each drives, the notices
//! everyone sees, and the answer to every message a friend's launcher posts (`HostState::answer`),
//! worked out on the company itself - the boss's launcher hands its company, the day's plan and
//! timetable (`Here`), and saves the company when the answer changed it. Every message is
//! checked against what its player may do (`roles`): a player whose page shows a button he may
//! not use is still refused here.
//!
//! The boss opens no world: a player drives his duty in his own game, alone, at his own pace
//! (`wire::DriveStart`), and his launcher reports the trips as they end.

use super::super::clock;
use super::super::duty_launch::{self as dl, DutyRef, Hitch};
use super::super::model::Company;
use super::super::plan::{BusOf, DayPlan, Who};
use super::book::{self, ClaimState, Handed, LeftWhy};
use super::code;
use super::roles::{Perm, Role};
use super::wire::{Card, DriveStart, DutyState, Notice, Request, SnapAsk, SnapDuty, SnapLine, SnapMember, SnapPlayer, SnapResult, SnapYou, Snapshot};
use serde_json::{json, Value};

/// A player who has not been heard from for this long is offline (s).
pub const ONLINE_FOR: u64 = 20;
/// A player not heard from for this long has left: the AI drives what runs of his duties (s).
pub const LEFT_AFTER: u64 = 60;
/// How many notices are kept.
const NOTICES_KEPT: usize = 120;
/// The duties that ended longer ago than this are left out of the friend's day (minutes).
const ENDED_SHOWN: i32 = 60;

/// Why a message is not answered (English: the keys of the translations). `READING`, `BUSY` and
/// `NOT_OPEN_COMPANY` pass: a friend's launcher asks again (`passing`).
pub const BAD_SECRET: &str = "The code is no longer valid: ask the boss for the code of his session.";
pub const READING: &str = "The boss's launcher is still reading the day's timetable: try again in a moment.";
pub const UNKNOWN: &str = "The boss's launcher did not understand that: both players need the same version of Omsi-Hub.";
pub const NOT_OPEN_COMPANY: &str = "The boss does not have the company open right now.";
pub const BUSY: &str = "The boss's company is busy with its day just now: try again in a moment.";
/// A player who may not do what he asked (the answer names the permission: `perm`).
pub const NOT_ALLOWED: &str = "Your role does not allow that.";
pub const BANNED: &str = "The boss has removed you from this company.";
pub const KICKED: &str = "The boss sent you away from this session.";
pub const NO_PLAYER: &str = "That player is no longer in the company.";
/// Players may not set their own role, nor one above theirs.
pub const NOT_ABOVE: &str = "You can only manage players whose permissions you have yourself.";
/// The boss ended the session (stopped sharing, closed the company, quit his launcher): every
/// message is answered with this, and the company's name (`closed_answer`).
pub const CLOSED: &str = "The host closed this session.";

/// Whether a refusal passes by itself (the friend's launcher sends the same again later).
pub fn passing(error: &str) -> bool {
    [READING, BUSY, NOT_OPEN_COMPANY].contains(&error)
}

/// Whether a refusal ends the player's session (his launcher stops asking).
pub fn ending(error: &str) -> bool {
    [BAD_SECRET, BANNED, KICKED, CLOSED].contains(&error)
}

/// The answer to every message once the session is closed: the company's name with it (the
/// player's launcher says "<company> - the host closed this session").
pub fn closed_answer(company: &str) -> Value {
    json!({ "ok": false, "error": CLOSED, "company": company })
}

/// What is told to everyone (English: the keys of the translations).
pub const JOINED: &str = "%{name} joined the company.";
pub const LEFT: &str = "%{name} left.";
pub const TOOK: &str = "%{name} took duty %{k} of line %{n} (tour %{tour}).";
pub const ASKS: &str = "%{name} asks for duty %{k} of line %{n} (tour %{tour}).";
pub const GAVE: &str = "%{name} has duty %{k} of line %{n} (tour %{tour}) now.";
pub const REFUSED: &str = "No: duty %{k} of line %{n} (tour %{tour}) is not %{name}'s.";
pub const BACK: &str = "%{name} gave back duty %{k} of line %{n} (tour %{tour}).";
pub const OFF: &str = "%{name} is off duty %{k} of line %{n} (tour %{tour}).";
pub const DRIVES: &str = "%{name} drives duty %{k} of line %{n} (tour %{tour}).";
pub const FINISHED: &str = "%{name} finished duty %{k} of line %{n}: %{trips} trips, score %{score}.";
pub const SAYS: &str = "%{name}: %{text}";
pub const ROLE_SET: &str = "%{name} is %{role} now.";
pub const SENT_AWAY: &str = "%{name} was sent away.";
pub const REMOVED: &str = "%{name} was removed from the company.";
pub const LET_BACK: &str = "%{name} may join again.";
/// The AI on a duty its player left (Beta 63).
pub const AI_REST: &str = "%{name} left duty %{k} of line %{n} (tour %{tour}): the AI drives the rest of it.";
pub const AI_ALL: &str = "%{name} left duty %{k} of line %{n} (tour %{tour}) before its first trip: the AI drives all of it.";
pub const LEFT_OPEN: &str = "%{name} left duty %{k} of line %{n} (tour %{tour}) before its first trip: it is open for another player.";
pub const TOOK_BACK: &str = "%{name} takes duty %{k} of line %{n} (tour %{tour}) back from the AI at its next trip.";
pub const BACK_AGAIN: &str = "%{name} is back: duty %{k} of line %{n} (tour %{tour}) is his again.";
pub const CLOSING: &str = "The host closed this session.";

/// Every English text of this file (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    vec![BAD_SECRET, READING, UNKNOWN, NOT_OPEN_COMPANY, BUSY, NOT_ALLOWED, BANNED, KICKED, NO_PLAYER, NOT_ABOVE, CLOSED, JOINED, LEFT, TOOK, ASKS, GAVE, REFUSED, BACK, OFF, DRIVES, FINISHED, SAYS, ROLE_SET, SENT_AWAY, REMOVED, LET_BACK, AI_REST, AI_ALL, LEFT_OPEN, TOOK_BACK, BACK_AGAIN]
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
}

/// The session's own: its secret, the company, the players heard from, those sent away from it
/// and the notices.
#[derive(Clone, Debug, Default)]
pub struct HostState {
    pub secret: [u8; 8],
    pub company: String,
    pub boss: String,
    pub online: Vec<Online>,
    /// Sent away from this session (a ban keeps them out of the company: `Guest::banned`).
    pub kicked: Vec<u32>,
    /// The players who left this session (said so, or were sent away), and why: what runs of
    /// their duties the AI drives (`watch`).
    pub gone: Vec<(u32, LeftWhy)>,
    pub notices: Vec<Notice>,
    /// The boss ended the session: every message is answered with `closed_answer`.
    pub closed: bool,
    next: u64,
}

/// What the boss's launcher knows besides the company.
pub struct Here<'a> {
    /// The company's day (None: its timetable is still being read).
    pub plan: Option<&'a DayPlan>,
    pub lines: &'a [crate::LineInfo],
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

/// A refusal for want of permission `p`.
pub fn not_allowed(p: Perm) -> Value {
    json!({ "ok": false, "error": NOT_ALLOWED, "perm": p })
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

/// The arguments of a duty a player has (its claim knows its number when the plan no longer
/// has it).
fn claim_args(c: &Company, p: Option<&DayPlan>, r: &DutyRef) -> Vec<(String, String)> {
    let mut args = duty_args(p, r);
    if p.and_then(|p| r.find(p)).is_none() {
        if let Some(x) = c.together.claim_of(r).filter(|x| !x.number.is_empty()) {
            args[0].1 = x.number.clone();
        }
    }
    args
}

/// Whether `actor` may manage player `target`: not himself, and nobody who may do more than he.
fn may_manage(c: &Company, actor: u32, target: u32) -> Result<(), &'static str> {
    if actor == target {
        return Err(NOT_ABOVE);
    }
    let mine = c.together.perms_of(actor);
    let g = c.together.guest(target).ok_or(NO_PLAYER)?;
    if g.perms().iter().all(|p| mine.contains(p)) {
        Ok(())
    } else {
        Err(NOT_ABOVE)
    }
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

    /// Whether a message comes with the session's code (else the refusal, saying nothing else).
    pub fn admits(&self, body: &str) -> Result<Request, Value> {
        let Ok(req) = serde_json::from_str::<Request>(body) else { return Err(refusal(UNKNOWN)) };
        if req.v != code::VERSION {
            return Err(refusal(code::NEWER));
        }
        if !same_secret(&req.secret, &self.secret) || req.company != code::tag_of(&self.company) {
            return Err(refusal(BAD_SECRET));
        }
        Ok(req)
    }

    /// The player a message admitted (`admits`) comes from, known to the company (added when
    /// new) and seen now: his number and what he may do. Else the refusal (sent away, too many).
    pub fn member(&mut self, c: &mut Company, req: &Request, now: u64) -> Result<(u32, Vec<Perm>), Value> {
        if c.id != self.company {
            return Err(refusal(NOT_OPEN_COMPANY));
        }
        let Some(guest) = c.together.guest_of(&req.player, &req.name) else { return Err(refusal(book::TOO_MANY)) };
        if c.together.guest(guest).is_some_and(|g| g.banned) {
            self.online.retain(|o| o.guest != guest);
            return Err(refusal(BANNED));
        }
        if self.kicked.contains(&guest) {
            return Err(refusal(KICKED));
        }
        let name = c.together.name_of(guest);
        let leaving = req.what == "leave";
        let came = !self.is_online(guest, now);
        match self.online.iter_mut().find(|o| o.guest == guest) {
            Some(o) => {
                o.seen = now;
                o.name = name.clone();
                o.driving = req.driving.clone();
                o.in_game = req.in_game;
            }
            None => self.online.push(Online { guest, name: name.clone(), seen: now, driving: req.driving.clone(), in_game: req.in_game }),
        }
        if came && !leaving {
            self.gone.retain(|g| g.0 != guest);
            self.tell(now, JOINED, vec![("name".into(), name.clone())]);
        }
        // (not heard from for a while, but his game drove on: the duty is his again)
        if !leaving {
            if let Some(r) = req.driving.as_ref().filter(|_| req.in_game) {
                let silent = c.together.claim_of(r).filter(|x| x.guest == guest).and_then(|x| x.ai_now()).is_some_and(|a| a.why == LeftWhy::Silent);
                if silent {
                    if let Some(x) = c.together.claims.iter_mut().find(|x| book::same_duty(&x.duty, r)) {
                        x.ai.pop();
                    }
                    let mut a = vec![("name".to_string(), name)];
                    a.extend(duty_args(None, r));
                    self.tell(now, BACK_AGAIN, a);
                }
            }
        }
        Ok((guest, c.together.perms_of(guest)))
    }

    fn answer_inner(&mut self, c: &mut Company, here: &Here, body: &str) -> Value {
        let req = match self.admits(body) {
            Ok(r) => r,
            Err(v) => return v,
        };
        if self.closed {
            return closed_answer(&c.name);
        }
        let now = here.now;
        let (guest, perms) = match self.member(c, &req, now) {
            Ok(x) => x,
            Err(v) => return v,
        };
        let name = c.together.name_of(guest);
        let can = |p: Perm| perms.contains(&p);
        let p = here.plan;
        let clock_now = clock::now(c);
        let mut extra = json!({});
        // (a duty whose day closed: no plan needed)
        let late = req.duty.as_ref().and_then(|r| c.together.claim_of(r)).is_some_and(|x| x.late);
        let result: Result<(), Value> = match req.what.as_str() {
            "hello" | "snapshot" => Ok(()),
            "take" | "release" | "start" | "take_back" | "report" | "assign" | "unassign" | "answer" if p.is_none() && !late => Err(refusal(READING)),
            "take" if !can(Perm::Drive) => Err(not_allowed(Perm::Drive)),
            "take" => match req.duty.as_ref() {
                None => Err(refusal(book::NO_DUTY)),
                Some(r) => book::take(c, p.unwrap(), r, guest, clock_now, can(Perm::Approve)).map_err(refusal).map(|state| {
                    let text = if state == ClaimState::Asked { ASKS } else { TOOK };
                    self.tell_duty(now, text, &name, p, r);
                    extra = json!({ "state": state });
                }),
            },
            "release" => match req.duty.as_ref() {
                None => Err(refusal(book::NO_DUTY)),
                Some(r) => {
                    let args = claim_args(c, p, r);
                    book::release(c, r, Some(guest)).map_err(refusal).map(|_| {
                        let mut a = vec![("name".to_string(), name.clone())];
                        a.extend(args);
                        self.tell(now, BACK, a);
                    })
                }
            },
            "start" if !can(Perm::Drive) => Err(not_allowed(Perm::Drive)),
            "start" => match req.duty.as_ref() {
                None => Err(refusal(book::NO_DUTY)),
                Some(r) => self.start(c, here, r, guest, &name).map_err(refusal).map(|s| extra = json!({ "start": s })),
            },
            // (Beta 63: the duty he left, back from the AI at its next trip)
            "take_back" if !can(Perm::Drive) => Err(not_allowed(Perm::Drive)),
            "take_back" => match req.duty.as_ref() {
                None => Err(refusal(book::NO_DUTY)),
                Some(r) => self.take_back(c, here, r, guest, &name).map_err(refusal).map(|(s, back)| extra = json!({ "start": s, "back": back })),
            },
            "report" => match req.duty.as_ref() {
                None => Err(refusal(book::NO_DUTY)),
                Some(r) => book::report(c, p, r, guest, &req.trips, now).map_err(refusal).map(|(taken, refused)| {
                    extra = json!({ "taken": taken, "refused": refused });
                    // (his game ended before the duty did: the AI drives the rest - Beta 63)
                    let ai = req.done && self.left_duty(c, here, r, guest, LeftWhy::Quit);
                    if req.done && !ai && c.together.claim_of(r).is_some_and(|x| x.ai_now().is_none()) && book::set_state(c, r, guest, ClaimState::Done).is_ok() {
                        let res = result_of(c, p, r);
                        let mut args = vec![("name".to_string(), name.clone())];
                        args.extend(claim_args(c, p, r));
                        args.push(("trips".into(), res.trips.to_string()));
                        args.push(("score".into(), res.score.to_string()));
                        self.tell(now, FINISHED, args);
                        if let Some(o) = self.online.iter_mut().find(|o| o.guest == guest) {
                            o.driving = None;
                        }
                    }
                    if ai {
                        if let Some(o) = self.online.iter_mut().find(|o| o.guest == guest) {
                            o.driving = None;
                        }
                    }
                }),
            },
            "assign" if !can(Perm::Assign) => Err(not_allowed(Perm::Assign)),
            "assign" => match (req.duty.as_ref(), c.together.guest(req.target).filter(|g| !g.banned).map(|g| g.name.clone())) {
                (None, _) => Err(refusal(book::NO_DUTY)),
                (_, None) => Err(refusal(NO_PLAYER)),
                (Some(r), Some(who)) => book::give(c, p.unwrap(), r, req.target, clock_now).map_err(refusal).map(|_| self.tell_duty(now, GAVE, &who, p, r)),
            },
            "unassign" if !can(Perm::Assign) => Err(not_allowed(Perm::Assign)),
            "unassign" => match req.duty.as_ref() {
                None => Err(refusal(book::NO_DUTY)),
                Some(r) => {
                    let args = claim_args(c, p, r);
                    book::release(c, r, None).map_err(refusal).map(|g| {
                        let mut a = vec![("name".to_string(), c.together.name_of(g))];
                        a.extend(args);
                        self.tell(now, OFF, a);
                    })
                }
            },
            "answer" if !can(Perm::Approve) => Err(not_allowed(Perm::Approve)),
            "answer" => match req.duty.as_ref() {
                None => Err(refusal(book::NO_DUTY)),
                Some(r) => match book::answer(c, r, req.yes) {
                    Some(g) => {
                        let who = c.together.name_of(g);
                        self.tell_duty(now, if req.yes { GAVE } else { REFUSED }, &who, p, r);
                        Ok(())
                    }
                    None => Err(refusal(book::NO_DUTY)),
                },
            },
            "set_role" if !can(Perm::Players) => Err(not_allowed(Perm::Players)),
            "set_role" => match may_manage(c, guest, req.target) {
                Err(e) => Err(refusal(e)),
                // (nobody gives what he may not do himself)
                Ok(()) if !req.perms.iter().all(|x| perms.contains(x)) => Err(refusal(NOT_ABOVE)),
                Ok(()) => {
                    self.set_role(c, now, req.target, req.role, &req.perms);
                    Ok(())
                }
            },
            "kick" if !can(Perm::Players) => Err(not_allowed(Perm::Players)),
            "kick" => match may_manage(c, guest, req.target) {
                Err(e) => Err(refusal(e)),
                Ok(()) => {
                    self.send_away(c, now, req.target, req.ban);
                    Ok(())
                }
            },
            "say" => {
                let text = book::clean(&req.text, 160);
                if !text.is_empty() {
                    self.tell(now, SAYS, vec![("name".into(), name.clone()), ("text".into(), text)]);
                }
                Ok(())
            }
            "leave" => {
                // (what he only asked for or was planned on and has not begun goes back; what
                // runs of his duties the AI drives - Beta 63)
                let runs = self.running(c, guest);
                let mine: Vec<DutyRef> = c.together.claims.iter().filter(|x| x.guest == guest && matches!(x.state, ClaimState::Asked | ClaimState::Planned) && !x.late && !runs.iter().any(|r| book::same_duty(r, &x.duty))).map(|x| x.duty.clone()).collect();
                for r in &mine {
                    let _ = book::release(c, r, Some(guest));
                }
                self.online.retain(|o| o.guest != guest);
                self.tell(now, LEFT, vec![("name".into(), name.clone())]);
                for r in &runs {
                    self.left_duty(c, here, r, guest, LeftWhy::Left);
                }
                self.went(guest, LeftWhy::Left);
                return json!({ "ok": true });
            }
            _ => Err(refusal(UNKNOWN)),
        };
        // (a duty that became a player's: how it starts, kept with it)
        self.prepare_starts(c, here);
        let snap = snapshot(c, here, self, Some(guest), req.since);
        let mut out = match result {
            Ok(()) => json!({ "ok": true }),
            Err(v) => v,
        };
        if let (Some(o), Some(x)) = (out.as_object_mut(), extra.as_object()) {
            o.extend(x.clone());
        }
        out["you"] = json!(guest);
        out["snapshot"] = serde_json::to_value(snap).unwrap_or(Value::Null);
        out
    }

    /// Player `target` gets `role` with the permissions `want` (the boss's panel, or a player
    /// who manages players): told to everyone.
    pub fn set_role(&mut self, c: &mut Company, now: u64, target: u32, role: Role, want: &[Perm]) -> bool {
        if !c.together.set_role(target, role, want) {
            return false;
        }
        let name = c.together.name_of(target);
        self.tell(now, ROLE_SET, vec![("name".into(), name), ("role".into(), role.label().into())]);
        true
    }

    /// Send player `target` away from the session (`ban`: from the company, until the boss lets
    /// him back): what he only asked for or was planned on goes back; what he drives stays.
    pub fn send_away(&mut self, c: &mut Company, now: u64, target: u32, ban: bool) {
        let name = c.together.name_of(target);
        // (what runs of his duties the AI drives: `watch`)
        let runs = self.running(c, target);
        let his: Vec<DutyRef> = c.together.claims.iter().filter(|x| x.guest == target && matches!(x.state, ClaimState::Asked | ClaimState::Planned) && !runs.iter().any(|r| book::same_duty(r, &x.duty))).map(|x| x.duty.clone()).collect();
        for r in &his {
            let _ = book::release(c, r, None);
        }
        self.online.retain(|o| o.guest != target);
        if !self.kicked.contains(&target) {
            self.kicked.push(target);
        }
        if ban {
            if let Some(g) = c.together.guest_mut(target) {
                g.banned = true;
            }
        }
        self.tell(now, if ban { REMOVED } else { SENT_AWAY }, vec![("name".into(), name)]);
        self.went(target, LeftWhy::Removed);
    }

    /// The boss lets player `target` back (sent away, or removed).
    pub fn let_back(&mut self, c: &mut Company, now: u64, target: u32) {
        self.kicked.retain(|g| *g != target);
        self.gone.retain(|g| g.0 != target);
        if let Some(g) = c.together.guest_mut(target) {
            g.banned = false;
        }
        let name = c.together.name_of(target);
        self.tell(now, LET_BACK, vec![("name".into(), name)]);
    }

    /// How the players' duties of today start, worked out once each and kept with the claim
    /// (so that a duty whose day closes before it is driven can still be started).
    pub fn prepare_starts(&mut self, c: &mut Company, here: &Here) {
        let Some(p) = here.plan else { return };
        if here.lines.is_empty() {
            return;
        }
        let missing: Vec<DutyRef> = c.together.claims.iter().filter(|x| x.start.is_none() && !x.late && x.duty.date == p.date && matches!(x.state, ClaimState::Planned | ClaimState::Driving)).map(|x| x.duty.clone()).collect();
        for r in missing {
            if let Ok(st) = start_of(c, here, p, &r) {
                book::keep_start(c, &r, &st);
            }
        }
    }

    /// A player starts his duty `r`: its start in his own game, or why not.
    fn start(&mut self, c: &mut Company, here: &Here, r: &DutyRef, guest: u32, name: &str) -> Result<DriveStart, &'static str> {
        let x = c.together.claim_of(r).filter(|x| x.guest == guest).ok_or(book::NOT_YOURS)?.clone();
        match x.state {
            ClaimState::Asked => return Err(book::NOT_YOURS),
            ClaimState::Done => return Err(book::DONE),
            _ => {}
        }
        if x.ai_now().is_some() {
            return Err(book::AI_DRIVES);
        }
        // (today's: from the plan as it is now; a closed day's: as it was kept)
        let st = match here.plan.filter(|p| !x.late && p.date == r.date) {
            Some(p) => start_of(c, here, p, r)?,
            None => x.start.clone().ok_or(book::CLOSED_DAY)?,
        };
        book::keep_start(c, r, &st);
        book::set_state(c, r, guest, ClaimState::Driving)?;
        if let Some(o) = self.online.iter_mut().find(|o| o.guest == guest) {
            o.driving = Some(r.clone());
        }
        let args = claim_args(c, here.plan, r);
        let mut a = vec![("name".to_string(), name.to_string())];
        a.extend(args);
        self.tell(here.now, DRIVES, a);
        Ok(st)
    }

    /// A player takes his duty `r` back from the AI (`book::take_back`): its start in his own
    /// game from the trip the AI hands back at, and that trip (an index into its trips).
    fn take_back(&mut self, c: &mut Company, here: &Here, r: &DutyRef, guest: u32, name: &str) -> Result<(DriveStart, usize), &'static str> {
        let p = here.plan.filter(|p| p.date == r.date).ok_or(book::CLOSED_DAY)?;
        let now = clock::now(c);
        // (worked out before the AI hands back: it keeps the duty when he cannot start)
        let back = book::back_at(c, Some(p), r, now).ok_or(book::AI_DONE)?;
        let st = start_from(c, here, p, r, back)?;
        let back = book::take_back(c, Some(p), r, guest, now)?;
        book::set_state(c, r, guest, ClaimState::Driving)?;
        if let Some(o) = self.online.iter_mut().find(|o| o.guest == guest) {
            o.driving = Some(r.clone());
        }
        let mut a = vec![("name".to_string(), name.to_string())];
        a.extend(claim_args(c, Some(p), r));
        self.tell(here.now, TOOK_BACK, a);
        Ok((st, back))
    }

    /// Player `guest` is gone from the session (`why`): kept, so that what runs of his duties
    /// later is the AI's too (`watch`).
    fn went(&mut self, guest: u32, why: LeftWhy) {
        self.gone.retain(|g| g.0 != guest);
        self.gone.push((guest, why));
    }

    /// The duties of player `guest` that run now: the one his game drives, and those planned for
    /// him that have begun on the company's clock (the company's day only: the AI does not drive
    /// a closed day's).
    fn running(&self, c: &Company, guest: u32) -> Vec<DutyRef> {
        let now = clock::now(c);
        c.together
            .claims
            .iter()
            .filter(|x| x.guest == guest && !x.late && x.duty.date == c.date && x.ai_now().is_none())
            .filter(|x| x.state == ClaimState::Driving || (x.state == ClaimState::Planned && now >= clock::moment(&x.duty.date, x.from as i64)))
            .map(|x| x.duty.clone())
            .collect()
    }

    /// Player `guest` left his duty `r` (`why`): the AI drives the rest of it, or all of it, or it
    /// goes back open (`book::hand_to_ai`), told to everyone. Whether the AI took it over now.
    pub fn left_duty(&mut self, c: &mut Company, here: &Here, r: &DutyRef, guest: u32, why: LeftWhy) -> bool {
        if c.together.claim_of(r).is_some_and(|x| x.ai_now().is_some()) {
            return false;
        }
        let args = claim_args(c, here.plan, r);
        let minute = c.clock.minute as i32;
        let Ok(handed) = book::hand_to_ai(c, here.plan, r, guest, why, here.now, minute) else { return false };
        let text = match handed {
            Handed::Over => return false,
            Handed::Open => LEFT_OPEN,
            Handed::Ai(0) => AI_ALL,
            Handed::Ai(_) => AI_REST,
        };
        let mut a = vec![("name".to_string(), c.together.name_of(guest))];
        a.extend(args);
        self.tell(here.now, text, a);
        if let Some(o) = self.online.iter_mut().find(|o| o.guest == guest && o.driving.as_ref().is_some_and(|d| book::same_duty(d, r))) {
            o.driving = None;
        }
        matches!(handed, Handed::Ai(_))
    }

    /// Keep watch over the players' duties (the boss's launcher every few seconds, and with every
    /// message): what runs of the duties of a player who left the session, was sent away or has
    /// not been heard from for `LEFT_AFTER` the AI drives from where he got to. Whether the
    /// company changed.
    pub fn watch(&mut self, c: &mut Company, here: &Here) -> bool {
        if self.closed {
            return false;
        }
        let before = c.together.clone();
        let silent: Vec<(u32, LeftWhy)> = self.online.iter().filter(|o| here.now.saturating_sub(o.seen) > LEFT_AFTER).map(|o| (o.guest, LeftWhy::Silent)).collect();
        let away: Vec<(u32, LeftWhy)> = self.gone.iter().copied().chain(silent).collect();
        for (guest, why) in away {
            for r in self.running(c, guest) {
                self.left_duty(c, here, &r, guest, why);
            }
        }
        c.together != before
    }

    /// The boss ends the session (he stops sharing, closes the company, quits his launcher):
    /// every message is answered with `closed_answer` from now on. A duty a player's game drives
    /// is the AI's to finish (his results can no longer come), what was only planned for the
    /// players or asked for goes back to the company. Whether the company changed.
    pub fn close(&mut self, c: &mut Company, here: &Here) -> bool {
        if self.closed {
            return false;
        }
        let before = c.together.clone();
        let driving: Vec<(DutyRef, u32)> = c.together.claims.iter().filter(|x| x.state == ClaimState::Driving && !x.late && x.duty.date == c.date && x.ai_now().is_none()).map(|x| (x.duty.clone(), x.guest)).collect();
        for (r, g) in &driving {
            self.left_duty(c, here, r, *g, LeftWhy::Closed);
        }
        c.together.claims.retain(|x| matches!(x.state, ClaimState::Driving | ClaimState::Done) || x.late || !x.ai.is_empty());
        self.tell(here.now, CLOSING, Vec::new());
        self.closed = true;
        c.together != before
    }
}

/// How a player's game starts on duty `r` of the day `p` (planned for a player: not another's).
fn start_of(c: &Company, here: &Here, p: &DayPlan, r: &DutyRef) -> Result<DriveStart, &'static str> {
    if here.lines.is_empty() {
        return Err(READING);
    }
    let mut sheet = dl::sheet(c, p, r, clock::now(c), &[]);
    // (and driven in his own game at his own pace: the company's clock may be past it, and his
    // launcher sees whether its bus is installed there)
    sheet.hitches.retain(|h| !matches!(h, Hitch::NotYours(_) | Hitch::NoDriver | Hitch::Over | Hitch::NotInstalled(_)));
    let st = dl::start(&sheet, here.lines, here.lead).map_err(|h| h.words().0)?;
    Ok(DriveStart::of(&st))
}

/// How a player's game starts on duty `r` of the day `p` from its trip `from` (an index into its
/// trips: the AI hands back there, `HostState::take_back`).
fn start_from(c: &Company, here: &Here, p: &DayPlan, r: &DutyRef, from: usize) -> Result<DriveStart, &'static str> {
    if here.lines.is_empty() {
        return Err(READING);
    }
    let mut sheet = dl::sheet(c, p, r, clock::now(c), &[]);
    sheet.hitches.retain(|h| !matches!(h, Hitch::NotYours(_) | Hitch::NoDriver | Hitch::Over | Hitch::NotInstalled(_) | Hitch::Driven));
    sheet.planned = sheet.planned.split_off(from.min(sheet.planned.len()));
    let st = dl::start(&sheet, here.lines, here.lead).map_err(|h| h.words().0)?;
    Ok(DriveStart::of(&st))
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
    let mut res = SnapResult { duty: r.clone(), number: claim_args(c, p, r)[0].1.clone(), ..Default::default() };
    let mut total = 0u32;
    let late = c.together.claim_of(r).is_some_and(|x| x.late);
    for t in book::trips_of(c, r) {
        let ev = super::super::career::evaluate(&t.trip);
        // (as the close books it: `day::close_day`; a closed day's had the duty covered)
        if !late && t.duty.date == c.date {
            res.fares += book::trip_money(c, &t.trip);
        }
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
    res.done = c.together.claim_of(r).is_none_or(|x| x.state == ClaimState::Done);
    // (the trips the AI drives since he left it)
    if let Some(x) = c.together.claim_of(r).filter(|x| !x.ai.is_empty()) {
        let trips: Vec<super::super::network::PlannedTrip> = match p.and_then(|p| r.find(p).map(|(ti, k)| (p, ti, k))) {
            Some((p, ti, k)) => {
                let d = &p.tours[ti].duties[k];
                p.tours[ti].tour.trips[d.start..d.end].to_vec()
            }
            None => x.sheet.clone(),
        };
        let deps: Vec<i32> = book::trips_of(c, r).map(|t| (t.trip.departure / 60.0).round() as i32).collect();
        res.ai_trips = trips.iter().enumerate().filter(|(i, t)| t.counts() && !t.is_second() && x.ai_drives(*i) && !deps.contains(&t.whole_dep())).count();
    }
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
/// notices after `since`: what his permissions let him see.
pub fn snapshot(c: &Company, here: &Here, st: &HostState, guest: Option<u32>, since: u64) -> Snapshot {
    let perms = match guest {
        Some(g) => c.together.perms_of(g),
        None => Perm::ALL.to_vec(),
    };
    let can = |p: Perm| perms.contains(&p);
    let mut s = Snapshot {
        card: Card { id: c.id.clone(), name: c.name.clone(), short: c.short.clone(), colours: c.colours.clone(), map: c.map.clone(), map_name: c.map_name.clone(), boss: st.boss.clone(), logo: None },
        date: c.date.clone(),
        minute: c.clock.minute as i32,
        open_claims: c.together.open_claims,
        lines: c.lines.iter().map(|l| SnapLine { line: l.name.clone(), number: l.number.clone(), colour: l.colour.clone() }).collect(),
        reading: here.plan.is_none(),
        ..Default::default()
    };
    let now = clock::now(c);
    let colour_of = |line: &str| c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)).map(|l| l.colour.clone()).unwrap_or_default();
    if let Some(p) = here.plan {
        for (ti, t) in p.tours.iter().enumerate() {
            if t.tour.unplanned {
                continue;
            }
            let colour = colour_of(&t.tour.line);
            let (bus, file) = match t.bus {
                Some(BusOf::Own(id)) => c.vehicle(id).map(|v| (format!("{} {}", v.number, v.name).trim().to_string(), v.bus.clone())).unwrap_or_default(),
                Some(BusOf::Rental) => ("Rental bus".into(), String::new()),
                None => (String::new(), String::new()),
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
                    file: file.clone(),
                    guest: claim.map(|x| x.guest).unwrap_or(0),
                    ..Default::default()
                };
                let begun = now >= clock::moment(&p.date, d.from as i64);
                // (the AI on a duty its player left: from where, and whether he may take it back)
                if let Some(a) = claim.and_then(|x| x.ai_now()) {
                    sd.ai = true;
                    sd.ai_from = trips.get(a.from).map(|x| x.dep);
                    sd.back = mine && book::back_at(c, Some(p), &r, now).is_some();
                }
                (sd.state, sd.who, sd.why) = match claim {
                    Some(x) if mine => (state_of(x), c.together.name_of(x.guest), String::new()),
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
    }
    // (the duties of days closed since that are still the player's to drive)
    for x in c.together.claims.iter().filter(|x| x.late && Some(x.guest) == guest) {
        s.duties.push(SnapDuty {
            duty: x.duty.clone(),
            number: x.number.clone(),
            colour: colour_of(&x.duty.line),
            from: x.from,
            to: x.to,
            first: x.sheet.first().map(|t| t.from.clone()).unwrap_or_default(),
            last: x.sheet.last().map(|t| t.end().to_string()).unwrap_or_default(),
            trips: x.sheet.iter().filter(|t| t.counts()).count(),
            km: x.sheet.iter().map(|t| t.km).sum::<f64>().round(),
            file: x.bus.clone(),
            state: if x.state == ClaimState::Driving { DutyState::Driving } else { DutyState::Late },
            who: c.together.name_of(x.guest),
            guest: x.guest,
            ..Default::default()
        });
    }
    s.duties.sort_by(|a, b| (&a.duty.date, a.from, &a.number, &a.duty.tour).cmp(&(&b.duty.date, b.from, &b.number, &b.duty.tour)));
    // (the boss first, then the players online, then those with a duty today)
    s.players.push(SnapPlayer { name: st.boss.clone(), boss: true, online: true, role: Role::CoOwner, ..Default::default() });
    let label = |r: &DutyRef| (duty_args(here.plan, r)[0].1.clone(), r.tour.clone(), r.duty + 1);
    let role_of = |g: u32| c.together.guest(g).map(|x| x.role).unwrap_or_default();
    for o in st.online_now(here.now) {
        s.players.push(SnapPlayer { no: o.guest, name: o.name.clone(), me: Some(o.guest) == guest, online: true, driving: o.driving.as_ref().map(label), boss: false, role: role_of(o.guest) });
    }
    for g in &c.together.guests {
        let busy = c.together.claims.iter().any(|x| x.guest == g.no && x.duty.date == c.date);
        if busy && !g.banned && !st.is_online(g.no, here.now) {
            s.players.push(SnapPlayer { no: g.no, name: g.name.clone(), me: Some(g.no) == guest, online: false, role: g.role, ..Default::default() });
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
        if let Some(me) = c.together.guest(g) {
            s.you = Some(SnapYou { no: g, name: me.name.clone(), role: me.role, perms: perms.clone(), stats: me.stats.clone() });
        }
    }
    // (what managing the players and their duties needs)
    if can(Perm::Assign) || can(Perm::Players) || can(Perm::Approve) {
        s.members = c
            .together
            .guests
            .iter()
            .filter(|g| !g.banned || can(Perm::Players))
            .map(|g| SnapMember {
                no: g.no,
                name: g.name.clone(),
                role: g.role,
                perms: g.perms(),
                online: st.is_online(g.no, here.now),
                banned: g.banned,
                duties: c.together.claims.iter().filter(|x| x.guest == g.no && x.duty.date == c.date && x.state != ClaimState::Asked).count(),
                stats: g.stats.clone(),
            })
            .collect();
    }
    if can(Perm::Approve) {
        s.asks = c
            .together
            .claims
            .iter()
            .filter(|x| x.state == ClaimState::Asked && x.duty.date == c.date)
            .map(|x| SnapAsk { duty: x.duty.clone(), number: x.number.clone(), from: x.from, to: x.to, no: x.guest, name: c.together.name_of(x.guest) })
            .collect();
    }
    s
}

/// How a duty a player has stands, as his page shows it.
fn state_of(x: &book::Claim) -> DutyState {
    if x.ai_now().is_some() {
        return DutyState::Ai;
    }
    match x.state {
        ClaimState::Planned if x.late => DutyState::Late,
        ClaimState::Planned => DutyState::Mine,
        ClaimState::Asked => DutyState::Asked,
        ClaimState::Driving => DutyState::Driving,
        ClaimState::Done => DutyState::Done,
    }
}
