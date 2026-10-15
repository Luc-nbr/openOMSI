//! What a player of a company session may do (Luc 2026-10-15: "the host gives a player a role,
//! after which their screen changes ... the owner chooses per player which permissions they
//! get"): a role is a preset bundle of permissions, and the boss ticks or unticks single
//! permissions of a player on top of it. The boss himself (the host) may do everything; every
//! message a player's launcher sends is checked against his permissions on the boss's side
//! (`host`), whatever his page shows.

use serde::{Deserialize, Serialize};

/// One thing a player may do for the company.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Perm {
    /// Drive duties for the company, and ask for open ones.
    Drive,
    /// See the company's money: the finances, the overview, the reputation.
    Finances,
    /// Spend the company's money: buy and sell buses, the depot, research, contracts.
    Spend,
    /// Hire and fire the staff, their wages and courses.
    Staff,
    /// The lines and their timetables, fares and colours.
    Lines,
    /// The planning: blocks, duties and the roster (RosterFlow, the planning's pages).
    Plans,
    /// Give duties of the day to players, and take them off again.
    Assign,
    /// Say yes or no to the duties players ask for (and take open ones at once).
    Approve,
    /// Service control on the company's day.
    Service,
    /// The company's settings: its name, logo, game mode, the session's rules.
    Settings,
    /// The players: their roles and permissions, sending one away.
    Players,
}

impl Perm {
    pub const ALL: [Perm; 11] = [Perm::Drive, Perm::Finances, Perm::Spend, Perm::Staff, Perm::Lines, Perm::Plans, Perm::Assign, Perm::Approve, Perm::Service, Perm::Settings, Perm::Players];

    /// Its key (as the messages and the company file write it).
    pub fn key(self) -> &'static str {
        match self {
            Perm::Drive => "drive",
            Perm::Finances => "finances",
            Perm::Spend => "spend",
            Perm::Staff => "staff",
            Perm::Lines => "lines",
            Perm::Plans => "plans",
            Perm::Assign => "assign",
            Perm::Approve => "approve",
            Perm::Service => "service",
            Perm::Settings => "settings",
            Perm::Players => "players",
        }
    }

    pub fn of(key: &str) -> Option<Perm> {
        Perm::ALL.iter().copied().find(|p| p.key() == key.trim())
    }

    /// Its name (English: a key of the translations).
    pub fn label(self) -> &'static str {
        match self {
            Perm::Drive => "Drive and ask for duties",
            Perm::Finances => "See the finances",
            Perm::Spend => "Spend money (buy and sell buses)",
            Perm::Staff => "Hire and fire staff",
            Perm::Lines => "Edit lines and timetables",
            Perm::Plans => "Edit the planning (blocks, duties, roster)",
            Perm::Assign => "Give duties to players",
            Perm::Approve => "Approve duty requests",
            Perm::Service => "Run service control",
            Perm::Settings => "Change the company's settings",
            Perm::Players => "Manage players",
        }
    }

    /// Its icon (the launcher's icon font).
    pub fn icon(self) -> &'static str {
        match self {
            Perm::Drive => "directions_bus",
            Perm::Finances => "payments",
            Perm::Spend => "receipt_long",
            Perm::Staff => "badge",
            Perm::Lines => "route",
            Perm::Plans => "calendar_month",
            Perm::Assign => "sync_alt",
            Perm::Approve => "check_circle",
            Perm::Service => "traffic",
            Perm::Settings => "settings",
            Perm::Players => "group",
        }
    }
}

/// A player's role in the company.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Drives the duties he is given or asks for.
    #[default]
    Driver,
    /// Dispatcher and planner: the lines, the planning and the day's service.
    Planner,
    /// Runs the company with the boss: money, staff, fleet, planning.
    Manager,
    /// May do everything the boss does in the company.
    CoOwner,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Driver, Role::Planner, Role::Manager, Role::CoOwner];

    pub fn key(self) -> &'static str {
        match self {
            Role::Driver => "driver",
            Role::Planner => "planner",
            Role::Manager => "manager",
            Role::CoOwner => "co_owner",
        }
    }

    pub fn of(key: &str) -> Option<Role> {
        Role::ALL.iter().copied().find(|r| r.key() == key.trim())
    }

    /// Its name and what it is for (English: keys of the translations).
    pub fn label(self) -> &'static str {
        match self {
            Role::Driver => "Driver",
            Role::Planner => "Dispatcher / planner",
            Role::Manager => "Manager",
            Role::CoOwner => "Co-owner",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Role::Driver => "Drives the duties they are given, and asks for open ones.",
            Role::Planner => "Plans lines, blocks, duties and the roster, gives duties to players and runs the day's service.",
            Role::Manager => "Runs the company with you: money, fleet, staff and planning.",
            Role::CoOwner => "May do everything you can do in the company.",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Role::Driver => "directions_bus",
            Role::Planner => "event",
            Role::Manager => "stars",
            Role::CoOwner => "key",
        }
    }

    /// The permissions the role brings.
    pub fn preset(self) -> &'static [Perm] {
        match self {
            Role::Driver => &[Perm::Drive],
            Role::Planner => &[Perm::Drive, Perm::Lines, Perm::Plans, Perm::Assign, Perm::Approve, Perm::Service],
            Role::Manager => &[Perm::Drive, Perm::Finances, Perm::Spend, Perm::Staff, Perm::Lines, Perm::Plans, Perm::Assign, Perm::Approve, Perm::Service],
            Role::CoOwner => &Perm::ALL,
        }
    }
}

/// A player's permissions: his role's, with what the boss gave (`grant`) and took (`deny`) on
/// top of it, in the order of `Perm::ALL`.
pub fn effective(role: Role, grant: &[Perm], deny: &[Perm]) -> Vec<Perm> {
    Perm::ALL.iter().copied().filter(|p| (role.preset().contains(p) || grant.contains(p)) && !deny.contains(p)).collect()
}

/// The ticks the boss set made into what is given and taken on top of `role`: those the role
/// has not but the player should (`grant`), and those it has but he should not (`deny`).
pub fn overrides(role: Role, want: &[Perm]) -> (Vec<Perm>, Vec<Perm>) {
    let preset = role.preset();
    let grant = Perm::ALL.iter().copied().filter(|p| want.contains(p) && !preset.contains(p)).collect();
    let deny = Perm::ALL.iter().copied().filter(|p| !want.contains(p) && preset.contains(p)).collect();
    (grant, deny)
}

/// Every English text of this file (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    Perm::ALL.iter().map(|p| p.label()).chain(Role::ALL.iter().flat_map(|r| [r.label(), r.about()])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_bring_their_permissions_and_the_boss_ticks_on_top() {
        assert_eq!(effective(Role::Driver, &[], &[]), vec![Perm::Drive]);
        let planner = effective(Role::Planner, &[], &[]);
        assert!(planner.contains(&Perm::Plans) && planner.contains(&Perm::Assign) && !planner.contains(&Perm::Finances) && !planner.contains(&Perm::Spend));
        let manager = effective(Role::Manager, &[], &[]);
        assert!(manager.contains(&Perm::Spend) && manager.contains(&Perm::Staff) && !manager.contains(&Perm::Settings) && !manager.contains(&Perm::Players));
        assert_eq!(effective(Role::CoOwner, &[], &[]), Perm::ALL.to_vec());
        // a driver who may also see the money, a manager who may not hire
        assert_eq!(effective(Role::Driver, &[Perm::Finances], &[]), vec![Perm::Drive, Perm::Finances]);
        assert!(!effective(Role::Manager, &[], &[Perm::Staff]).contains(&Perm::Staff));
        // (the ticks made into overrides and back)
        let want = vec![Perm::Drive, Perm::Finances, Perm::Plans];
        let (grant, deny) = overrides(Role::Planner, &want);
        assert_eq!(effective(Role::Planner, &grant, &deny), want);
        assert_eq!(overrides(Role::Manager, Role::Manager.preset()), (Vec::new(), Vec::new()));
    }

    #[test]
    fn keys_read_back() {
        for p in Perm::ALL {
            assert_eq!(Perm::of(p.key()), Some(p));
            assert_eq!(serde_json::to_string(&p).unwrap(), format!("\"{}\"", p.key()));
        }
        for r in Role::ALL {
            assert_eq!(Role::of(r.key()), Some(r));
            assert_eq!(serde_json::to_string(&r).unwrap(), format!("\"{}\"", r.key()));
        }
    }
}
