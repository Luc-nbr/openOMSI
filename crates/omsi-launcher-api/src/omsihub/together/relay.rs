//! The company's own pages for a player whose role allows them (Beta 61, Luc: "higher roles see
//! the company pages their permissions allow"): his launcher shows the company's pages as the
//! boss's does, and relays their commands (`RELAYED`) to the boss's launcher, which runs them on
//! its company (`host`) - after checking here what each needs (`need`): a view of a page, an
//! action on it. What a player may never do from afar (the boss's clock, his game, his career,
//! his editors) is refused, and so is anything this table does not know.

use super::super::company::{CONCESSIONS_TAB, CONTRACTS_TAB, DEPOT_TAB, INBOX_TAB, MAP_TAB, OPS_TAB, REPUTATION_TAB, RESEARCH_TAB, SANDBOX_TAB, WORKSHOP_TAB};
use omsi_launcher_lib::company::together::Perm;
use serde_json::{json, Value};

/// The company's commands a player's launcher relays to the boss's (the rest run on his own:
/// the installed buses, the map's roads, the portraits).
pub const RELAYED: [&str; 6] = ["co_tick", "co_view", "co_act", "co_more_view", "co_more_act", "co_line_of"];

/// Why a relayed command is refused (English: keys of the translations).
pub const FROM_AFAR: &str = "Only the boss can do this, on his own launcher.";

/// What a command needs of a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Any player of the company.
    Member,
    /// One of these permissions.
    Any(&'static [Perm]),
    /// Never from afar.
    Never,
}

use Need::{Any, Member, Never};

const FINANCES: Need = Any(&[Perm::Finances]);
const SPEND: Need = Any(&[Perm::Spend]);
const STAFF: Need = Any(&[Perm::Staff]);
const LINES: Need = Any(&[Perm::Lines]);
const PLANS: Need = Any(&[Perm::Plans]);
const SERVICE: Need = Any(&[Perm::Service]);
const SETTINGS: Need = Any(&[Perm::Settings]);
/// The fleet's page: who buys, plans or runs the day's service.
const FLEET: Need = Any(&[Perm::Spend, Perm::Plans, Perm::Service]);

impl Need {
    /// Whether `perms` meet it.
    pub fn met(self, perms: &[Perm]) -> bool {
        match self {
            Member => true,
            Any(ps) => ps.iter().any(|p| perms.contains(p)),
            Never => false,
        }
    }

    /// The refusal when they do not.
    pub fn refusal(self) -> Value {
        match self {
            Any(ps) => json!({ "ok": false, "error": omsi_launcher_lib::company::together::host::NOT_ALLOWED, "perm": ps.first() }),
            _ => json!({ "ok": false, "error": FROM_AFAR }),
        }
    }
}

/// What a page of the company (`company::TABS`) needs to be seen.
pub fn tab_need(tab: usize) -> Need {
    match tab {
        // Overview, Finances, Reputation: the money
        0 | 4 | REPUTATION_TAB => FINANCES,
        1 => FLEET,
        2 => STAFF,
        3 => LINES,
        5 | 14 => PLANS,
        // the boss's own career
        6 => Never,
        DEPOT_TAB | CONCESSIONS_TAB | RESEARCH_TAB | WORKSHOP_TAB | CONTRACTS_TAB => SPEND,
        MAP_TAB => Member,
        INBOX_TAB | SANDBOX_TAB => SETTINGS,
        OPS_TAB => SERVICE,
        _ => Never,
    }
}

/// What a view of the company (`co_view`'s `view`) needs.
fn view_need(view: &str) -> Need {
    match view {
        "home" | "overview" | "money" | "late" | "late_advice" | "reputation" | "report" => FINANCES,
        "fleet" | "vehicle" | "fleet_value" => FLEET,
        "dealer" | "dealer_sheet" => SPEND,
        "people" => STAFF,
        "lines" | "add_line" | "fare" => LINES,
        "agenda" => PLANS,
        "ops" | "ask" => SERVICE,
        "inbox" | "sandbox" | "settings" => SETTINGS,
        // (a question asked before an action: the action is checked)
        "confirm" => Member,
        _ => Never,
    }
}

/// What a view of the company's later pages (`co_more_view`) needs.
fn more_view_need(view: &str) -> Need {
    match view {
        "depot" | "workshop" | "research" | "concessions" | "contracts" => SPEND,
        "fleetmap" => Member,
        "planning" | "plan_edit" => PLANS,
        "service" => Any(&[Perm::Plans, Perm::Service]),
        _ => Never,
    }
}

/// What an action on the company's pages (`co_act`'s `act`; `what`: a confirmed action's) needs.
fn act_need(act: &str, what: &str) -> Need {
    match act {
        // (the pages' own: which report line is open, the day's minor events shown)
        "feed_minor" => Member,
        "close_reports" | "report_line" | "show_report" => FINANCES,
        "dispatcher" => SERVICE,
        // the confirmed action is what counts
        "confirmed" => match what {
            "sell" => SPEND,
            "dismiss" | "holiday" => STAFF,
            "remove_line" => LINES,
            "game_mode" | "sandbox" => SETTINGS,
            _ => Never,
        },
        "logo_remove" | "rosterflow" | "realtime" | "game_mode" | "sandbox" => SETTINGS,
        "sell" => SPEND,
        "dismiss" | "holiday" => STAFF,
        "ask_spare" | "ask_rental" | "ask_drop" | "ask_on" | "ask_dispatcher" => SERVICE,
        "morning_fill" => PLANS,
        // the fleet
        "bus_livery" | "bus_service" | "new_take" | "bus_age" | "advert_end" => SPEND,
        // the dealer
        "buying_mode" | "open_model" | "open_offer" | "open_contract" | "open_loan" | "open_repay" | "open_loan_contract" | "open_advert" => SPEND,
        a if a.starts_with("sheet_") => SPEND,
        // the staff
        "hire" | "raise" | "advertise" | "keep" | "grant_raise" | "refuse_raise" | "course_lock" | "book_course" | "train" | "set_hours" | "staff_rule" | "auto" | "follow" | "holiday_answer" | "holiday_limit" | "agency_book" | "agency_cancel" | "staff_minimum" => STAFF,
        // the lines, their fares
        "line_title" | "line_hof" | "line_colour" | "add_line" | "remove_line" => LINES,
        "set_fare" => Any(&[Perm::Lines, Perm::Spend]),
        "inbox_read" | "inbox_unread" | "inbox_delete" | "inbox_mark_all" | "inbox_delete_read" => SETTINGS,
        "complaint_reply" | "ops_notice" | "ops_apply" | "ops_cover" | "ops_break" => SERVICE,
        "sandbox_knob" | "sandbox_balance" | "sandbox_reputation" | "sandbox_reset" => SETTINGS,
        // (the boss's clock, his logo file, his game, his liveries' editor and his career)
        _ => Never,
    }
}

/// What an action on the company's later pages (`co_more_act`) needs.
fn more_act_need(act: &str) -> Need {
    match act {
        a if a.starts_with("pe_") => PLANS,
        "depot_build" | "depot_cancel" | "depot_order" | "parts_auto" | "parts_buy" | "research_info" | "research_start" | "research_stop" => SPEND,
        "tender_pick" | "tender_bid" | "tender_buy" | "tender_step" => SPEND,
        "goal_take" | "goal_give_up" | "net_take" | "net_cancel" | "net_top_up" | "fuel_sign" | "cleaning_sign" | "cleaning_end" => SPEND,
        "map_play" | "map_speed" | "map_now" | "map_pick" => Member,
        "plan_give_driver" | "plan_give_bus" | "plan_set_fill" | "plan_fill" | "plan_clear" | "plan_repeat" | "plan_fold" | "plan_fold_all" | "plan_stop" | "plan_start" => PLANS,
        // (the boss drives, or opens the timetable's editor)
        _ => Never,
    }
}

/// What a relayed command needs (`args`: its arguments).
pub fn need(cmd: &str, args: &Value) -> Need {
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match cmd {
        "co_tick" | "co_line_of" => Member,
        "co_view" => view_need(&s("view")),
        "co_act" => act_need(&s("act"), &s("what")),
        "co_more_view" => more_view_need(&s("view")),
        "co_more_act" => more_act_need(&s("act")),
        _ => Never,
    }
}

/// The company's shell (`company::shell`) as a player with `perms` sees it: the pages he may
/// see, the money only when he may see it, no clock to run and nothing of the boss's own
/// (his reports, what happened while he was away, his duty launcher).
pub fn shell_for(mut sh: Value, perms: &[Perm]) -> Value {
    let may = |tab: usize| tab_need(tab).met(perms);
    if let Some(rail) = sh.get_mut("rail").and_then(Value::as_array_mut) {
        for g in rail.iter_mut() {
            if let Some(items) = g.as_array_mut() {
                items.retain(|it| it.get("tab").and_then(Value::as_u64).is_some_and(|t| may(t as usize)));
            }
        }
        rail.retain(|g| g.as_array().is_some_and(|a| !a.is_empty()));
    }
    if let Some(pages) = sh.get_mut("pages").and_then(Value::as_array_mut) {
        pages.retain(|t| t.as_u64().is_some_and(|t| may(t as usize)));
    }
    if let Some(status) = sh.get_mut("status").and_then(Value::as_array_mut) {
        for it in status.iter_mut() {
            let tab = it.get("tab").and_then(Value::as_u64);
            if tab.is_some_and(|t| !may(t as usize)) {
                it["tab"] = Value::Null;
            }
        }
        if !Need::met(FINANCES, perms) {
            status.retain(|it| it.get("icon").and_then(Value::as_str) != Some("payments"));
        }
    }
    if let Some(k) = sh.get_mut("clock") {
        k["speed"] = json!(0.0);
        k["tip"] = Value::Null;
    }
    for key in ["reports", "away", "launch", "asking"] {
        sh[key] = json!(false);
    }
    sh["casual_pages"] = Value::Null;
    sh["together"] = json!({ "on": false });
    sh["remote"] = json!({ "perms": perms.iter().map(|p| p.key()).collect::<Vec<_>>() });
    sh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The match arms of a page's `act` (the names of its actions), read from its source.
    fn acts_in(src: &str) -> Vec<String> {
        let Some(i) = src.find("fn act(") else { return Vec::new() };
        let body = &src[i..src[i..].find("\n}\n").map_or(src.len(), |j| i + j)];
        let mut out = Vec::new();
        for line in body.lines() {
            let t = line.trim_start();
            // (`"a" | "b" =>`, maybe with a guard)
            if !t.starts_with('"') || !t.contains("=>") {
                continue;
            }
            let head = &t[..t.find("=>").unwrap()];
            if head.split('|').all(|p| p.trim().starts_with('"') || p.contains(" if ")) {
                for part in head.split('|') {
                    let p = part.trim();
                    if let Some(name) = p.strip_prefix('"').and_then(|x| x.split('"').next()) {
                        out.push(name.to_string());
                    }
                }
            }
        }
        out
    }

    /// Every action of the company's pages is in the table (a new one is refused until it is
    /// given a permission here), and none of the boss's own gets through.
    #[test]
    fn every_action_of_the_pages_has_its_permission() {
        let pages = [
            ("dialogs", include_str!("../company/dialogs.rs")),
            ("fleet", include_str!("../company/fleet.rs")),
            ("dealer", include_str!("../company/dealer.rs")),
            ("people", include_str!("../company/people.rs")),
            ("lines", include_str!("../company/lines.rs")),
            ("money", include_str!("../company/money.rs")),
            ("inbox", include_str!("../company/inbox.rs")),
            ("reputation", include_str!("../company/reputation.rs")),
            ("ops", include_str!("../company/ops.rs")),
            ("sandbox", include_str!("../company/sandbox.rs")),
        ];
        // (the boss's own: never from afar)
        let own = ["logo", "move_date", "design_livery"];
        let mut missing = Vec::new();
        for (page, src) in pages {
            for a in acts_in(src) {
                let n = act_need(&a, "");
                if n == Never && !own.contains(&a.as_str()) && a != "confirmed" {
                    missing.push(format!("{page}: {a}"));
                }
            }
        }
        let more = [
            ("depot", include_str!("../company_more/depot.rs")),
            ("workshop", include_str!("../company_more/workshop.rs")),
            ("research", include_str!("../company_more/research.rs")),
            ("concessions", include_str!("../company_more/concessions.rs")),
            ("contracts", include_str!("../company_more/contracts.rs")),
            ("fleetmap", include_str!("../company_more/fleetmap.rs")),
            ("planning", include_str!("../company_more/planning.rs")),
            ("plan_edit", include_str!("../company_more/plan_edit.rs")),
        ];
        let own_more = ["plan_drive", "plan_timetable"];
        for (page, src) in more {
            for a in acts_in(src) {
                if more_act_need(&a) == Never && !own_more.contains(&a.as_str()) {
                    missing.push(format!("{page}: {a}"));
                }
            }
        }
        assert!(acts_in(include_str!("../company/people.rs")).len() > 10, "the actions are read");
        assert!(missing.is_empty(), "actions without a permission in together/relay.rs:\n{}", missing.join("\n"));
    }

    #[test]
    fn a_role_meets_what_its_pages_need() {
        let driver = [Perm::Drive];
        let planner = omsi_launcher_lib::company::together::Role::Planner.preset();
        let manager = omsi_launcher_lib::company::together::Role::Manager.preset();
        let ask = |cmd: &str, args: Value, perms: &[Perm]| need(cmd, &args).met(perms);
        // a driver: the map, nothing of the money or the planning
        assert!(ask("co_tick", json!({}), &driver));
        assert!(ask("co_more_view", json!({ "view": "fleetmap" }), &driver));
        assert!(!ask("co_view", json!({ "view": "money" }), &driver));
        assert!(!ask("co_more_view", json!({ "view": "planning" }), &driver));
        // a planner plans and runs the service, but sees no money and buys nothing
        assert!(ask("co_more_act", json!({ "act": "pe_save" }), planner));
        assert!(ask("co_act", json!({ "act": "ops_apply" }), planner));
        assert!(!ask("co_view", json!({ "view": "overview" }), planner));
        assert!(!ask("co_act", json!({ "act": "confirmed", "what": "sell" }), planner));
        // a manager buys and fires, but changes no settings
        assert!(ask("co_act", json!({ "act": "confirmed", "what": "sell" }), manager));
        assert!(ask("co_act", json!({ "act": "confirmed", "what": "dismiss" }), manager));
        assert!(!ask("co_act", json!({ "act": "confirmed", "what": "game_mode" }), manager));
        // nobody runs the boss's clock, game or career from afar - not even a co-owner
        for (cmd, args) in [("co_act", json!({ "act": "tomorrow" })), ("co_act", json!({ "act": "play", "speed": 10 })), ("co_act", json!({ "act": "launch_start" })), ("co_act", json!({ "act": "drive_duty" })), ("co_act", json!({ "act": "logo", "path": "C:/x.png" })), ("co_view", json!({ "view": "career" })), ("co_more_act", json!({ "act": "plan_drive" })), ("co_open", json!({})), ("co_hand_over", json!({}))] {
            assert_eq!(need(cmd, &args), Never, "{cmd} {args}");
        }
        assert_eq!(need("co_act", &json!({ "act": "sell" })).refusal()["perm"], "spend");
    }

    #[test]
    fn the_shell_shows_a_player_only_his_pages() {
        let sh = json!({
            "rail": [[{ "tab": 0 }, { "tab": 14 }, { "tab": 13 }], [{ "tab": 5 }, { "tab": 15 }, { "tab": 3 }, { "tab": 9 }, { "tab": 1 }], [{ "tab": 2 }]],
            "pages": [0, 14, 13, 5, 15, 3, 9, 1, 2],
            "status": [{ "icon": "payments", "tab": 4 }, { "icon": "star", "tab": 16 }],
            "clock": { "speed": 10.0 }, "reports": true, "away": true,
        });
        let planner = shell_for(sh.clone(), omsi_launcher_lib::company::together::Role::Planner.preset());
        assert_eq!(planner["pages"], json!([14, 5, 15, 3, 9, 1]));
        assert_eq!(planner["rail"].as_array().unwrap().len(), 2);
        assert_eq!(planner["status"], json!([{ "icon": "star", "tab": null }]));
        assert_eq!((planner["clock"]["speed"].as_f64(), planner["reports"].as_bool()), (Some(0.0), Some(false)));
        let driver = shell_for(sh, &[Perm::Drive]);
        assert_eq!(driver["pages"], json!([9]));
        assert_eq!(driver["remote"]["perms"], json!(["drive"]));
    }
}
