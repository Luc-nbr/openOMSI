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
/// the installed buses, the map's roads, the portraits) - and RosterFlow's (`relayed_rf`).
pub const RELAYED: [&str; 6] = ["co_tick", "co_view", "co_act", "co_more_view", "co_more_act", "co_line_of"];

/// Whether a player's launcher relays RosterFlow's command `cmd` (all but its tutorial, which is
/// his launcher's own: what he has seen of it, his lessons' place).
pub fn relayed_rf(cmd: &str) -> bool {
    cmd.starts_with("rf_") && !cmd.starts_with("rf_tut")
}

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
/// RosterFlow and the planning seen (changed: `PLANS`, a timetable `LINES`): who plans, runs the
/// lines or the day's service, or gives and approves the players' duties.
pub const RF_VIEW: Need = Any(&[Perm::Plans, Perm::Lines, Perm::Service, Perm::Assign, Perm::Approve]);
/// A duty given to a player of the session.
const ASSIGN: Need = Any(&[Perm::Assign]);

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
        // (the planning - RosterFlow with its add-on on - seen read-only without PLANS)
        5 => RF_VIEW,
        14 => PLANS,
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
        // (a duty card, read-only: a player's own duty's whatever his role, another's for who
        // may see the planning - `company::dutycard::relayed` checks which)
        "dutycard" => Member,
        _ => Never,
    }
}

/// What a view of the company's later pages (`co_more_view`) needs.
fn more_view_need(view: &str) -> Need {
    match view {
        "depot" | "workshop" | "research" | "concessions" | "contracts" | "variation" => SPEND,
        "fleetmap" => Member,
        "planning" | "plan_edit" => RF_VIEW,
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
        "logo_remove" | "rosterflow" | "realtime" | "game_mode" | "sandbox" | "currency" => SETTINGS,
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
        "line_title" | "line_type" | "line_hof" | "line_colour" | "add_line" | "remove_line" => LINES,
        "set_fare" | "set_cap" => Any(&[Perm::Lines, Perm::Spend]),
        "inbox_read" | "inbox_unread" | "inbox_delete" | "inbox_mark_all" | "inbox_delete_read" => SETTINGS,
        "complaint_reply" | "ops_notice" | "ops_apply" | "ops_cover" | "ops_break" => SERVICE,
        // (the control room's messages to the drivers: who runs the day's service sends them,
        // by his name)
        "ops_msg" | "ops_msg_ack" => SERVICE,
        "sandbox_knob" | "sandbox_preset" | "sandbox_balance" | "sandbox_reputation" | "sandbox_reset" => SETTINGS,
        // (the boss's clock, his logo file, his game, his liveries' editor and his career)
        _ => Never,
    }
}

/// What an action on the company's later pages (`co_more_act`) needs.
fn more_act_need(act: &str) -> Need {
    match act {
        // (the planning's sheet of a line: what it shows is the player's own, its trips and times
        // the lines' or the planners')
        "pe_day" | "pe_sel" | "pe_pdir" | "pe_from" | "pe_close" => RF_VIEW,
        a if a.starts_with("pe_") => Any(&[Perm::Lines, Perm::Plans]),
        "depot_build" | "depot_cancel" | "depot_order" | "parts_auto" | "parts_buy" | "research_info" | "research_start" | "research_pause" | "research_stop" => SPEND,
        "tender_pick" | "tender_bid" | "tender_buy" | "tender_step" => SPEND,
        "goal_take" | "goal_give_up" | "net_take" | "net_cancel" | "net_top_up" | "fuel_sign" | "cleaning_sign" | "cleaning_end" | "contract_hand_back" => SPEND,
        // (a tendered line's contract changed, or an offer for it: what the contracts cost)
        "contract_vary" | "contract_offer" => SPEND,
        "map_play" | "map_speed" | "map_now" | "map_pick" => Member,
        "plan_give_driver" | "plan_give_bus" | "plan_set_fill" | "plan_fill" | "plan_clear" | "plan_repeat" | "plan_fold" | "plan_fold_all" | "plan_stop" | "plan_start" => PLANS,
        // (a line's trips and timetable: the planning's sheet of the player's own, or RosterFlow's
        // Timeplan with the add-on on - its changes are checked as they are made)
        "plan_timetable" => RF_VIEW,
        // (the boss drives)
        _ => Never,
    }
}

/// What an action of RosterFlow's Vehicles and Drivers tabs (`rf_act`) needs.
fn rf_act_need(act: &str) -> Need {
    match act {
        // (what the page holds or asks again: no change of the plan)
        "pick" | "revalidate" | "keep_mine" => RF_VIEW,
        // (the line's colour is the line's: plates and the map take it)
        "colour" => LINES,
        "undo" | "redo" | "back_to" | "save" | "day_reset" | "day_unfollow" | "copy_rows" | "discard" | "prefs" | "opt_flags" | "colour_by" => PLANS,
        "optimise" | "accept" | "move" | "unassign" | "unassign_key" | "assign" | "swap" | "cut" | "uncut" | "cut_mid" | "join_piece" | "detach" | "link" | "unlink" => PLANS,
        "lock_block" | "lock_duty" | "lock_rows" | "unassign_rows" | "unassign_duties" | "spare" | "spare_name" | "spare_remove" | "custom_delete" | "note" | "travel_event" | "dismiss" | "drop" => PLANS,
        _ => Never,
    }
}

/// What an action of RosterFlow's Roster tab (`rf_roster_act`, `args` its arguments) needs.
fn rf_roster_need(act: &str, args: &Value) -> Need {
    match act {
        // (the page's own: a line's tours folded)
        "fold" | "fold_all" => RF_VIEW,
        // (a duty given to a player of the session: who may give the players theirs)
        "give_driver" if args.pointer("/who/guest").is_some() => ASSIGN,
        // (a bus sent for one that broke down today: the day's service)
        "break_choose" => Any(&[Perm::Service, Perm::Plans]),
        "undo" | "redo" | "stop" | "set_fill" | "clear" | "repeat" | "fill" | "service_start" | "give_driver" | "give_bus" => PLANS,
        "off_driver" | "off_bus" | "off_many" | "off_staff" | "off_vehicle" => PLANS,
        _ => Never,
    }
}

/// What an action of RosterFlow's Timeplan (`rf_tp_act`) needs: the line shown and its edits in
/// hand are the player's own, what changes a timetable is the lines'.
fn rf_tp_need(act: &str) -> Need {
    match act {
        "line" | "drop" | "undo" | "redo" | "from" => RF_VIEW,
        "save" | "copy_to" | "template_save" | "template_rename" | "template_drop" | "refresh" | "apply" | "nudge" | "band_add" | "band_remove" | "timepoint" | "add_one" | "add_many" | "delete" => LINES,
        "bulk_delete" | "bulk_shift" | "bulk_pattern" | "bulk_revert" | "revert" | "copy_run" => LINES,
        _ => Never,
    }
}

/// What a view of RosterFlow's (`rf_view`) needs.
fn rf_view_need(view: &str) -> Need {
    match view {
        "prefs" | "apply" | "candidates" | "zones" | "why" | "duty" | "prefs_check" => RF_VIEW,
        _ => Never,
    }
}

/// What a command of RosterFlow (`relayed_rf`) needs.
fn rf_need(cmd: &str, args: &Value) -> Need {
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match cmd {
        "rf_on" | "rf_board" | "rf_here" | "rf_print" | "rf_roster" | "rf_roster_service" | "rf_tp" | "rf_tp_guide" | "rf_tp_route" => RF_VIEW,
        // (the Routes strip's run times are the Timeplan's: set by who may edit the timetables)
        "rf_tp_route_act" => LINES,
        "rf_view" => rf_view_need(&s("view")),
        "rf_act" => rf_act_need(&s("act")),
        "rf_roster_act" => rf_roster_need(&s("act"), args),
        "rf_tp_act" => rf_tp_need(&s("act")),
        // (the tutorial is the player's launcher's own; its lessons put the boss's company back)
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
        c if relayed_rf(c) => rf_need(c, args),
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
    // (the bell is the boss's: his messages, his agenda)
    sh["bell"] = json!(0);
    sh["together"] = json!({ "on": false });
    sh["remote"] = json!({ "perms": perms.iter().map(|p| p.key()).collect::<Vec<_>>() });
    sh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The match arms of a page's `act` (the names of its actions), read from its source.
    fn acts_in(src: &str) -> Vec<String> {
        // (a Windows checkout has CRLF line ends: the body's end is found on LF alone)
        let src = &src.replace("\r\n", "\n");
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
            ("variations", include_str!("../company_more/variations.rs")),
            ("fleetmap", include_str!("../company_more/fleetmap.rs")),
            ("planning", include_str!("../company_more/planning.rs")),
            ("plan_edit", include_str!("../company_more/plan_edit.rs")),
        ];
        let own_more = ["plan_drive"];
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

    /// The names a function's outermost matches take (`"a" | "b" =>` at their least indent, the
    /// matches inside an arm left out), read from its source (`head`: how the function begins).
    fn arms_in(src: &str, head: &str) -> Vec<String> {
        // (a Windows checkout has CRLF line ends)
        let src = &src.replace("\r\n", "\n");
        let Some(i) = src.find(head) else { return Vec::new() };
        let body = &src[i..src[i..].find("\n}\n").map_or(src.len(), |j| i + j)];
        let mut arms: Vec<(usize, Vec<String>)> = Vec::new();
        for line in body.lines() {
            let t = line.trim_start();
            if !t.starts_with('"') || !t.contains("=>") {
                continue;
            }
            let head = &t[..t.find("=>").unwrap()];
            let names = head.split('|').filter_map(|p| p.trim().strip_prefix('"').and_then(|x| x.split('"').next())).map(str::to_string).collect();
            arms.push((line.len() - t.len(), names));
        }
        let least = arms.iter().map(|a| a.0).min().unwrap_or(0);
        arms.into_iter().filter(|a| a.0 == least).flat_map(|a| a.1).collect()
    }

    /// Every command of RosterFlow, every action of its tabs and every view of its cards has its
    /// permission here (a new one is refused to the players until it is given one), and nothing
    /// of the tutorial's goes to the boss's launcher.
    #[test]
    fn every_rosterflow_command_has_its_permission() {
        let main = include_str!("../rosterflow.rs");
        let roster = include_str!("../rosterflow/roster.rs");
        let timeplan = include_str!("../rosterflow/timeplan.rs");
        let tutorial = include_str!("../rosterflow/tutorial.rs");
        let mut missing = Vec::new();
        let mut cmds = arms_in(main, "fn run(");
        cmds.extend(arms_in(roster, "pub(super) fn run("));
        cmds.extend(arms_in(timeplan, "pub(super) fn run("));
        // (the print, which `call` takes first)
        cmds.push("rf_print".into());
        assert!(cmds.len() >= 12, "the commands are read: {cmds:?}");
        for c in &cmds {
            if need(c, &json!({})) == Never && !["rf_view", "rf_act", "rf_roster_act", "rf_tp_act"].contains(&c.as_str()) {
                missing.push(format!("command {c}"));
            }
        }
        let checks: [(&str, &str, &str, &str); 5] = [
            ("rf_act", "act", main, "fn act("),
            ("rf_view", "view", main, "fn view("),
            ("rf_roster_act", "act", roster, "fn act_on("),
            ("rf_roster_act", "act", roster, "fn off("),
            ("rf_tp_act", "act", timeplan, "fn act("),
        ];
        for (cmd, key, src, head) in checks {
            let names = arms_in(src, head);
            assert!(!names.is_empty(), "{cmd}: {head} is read");
            for a in names {
                let mut args = json!({});
                args[key] = json!(a);
                if need(cmd, &args) == Never {
                    missing.push(format!("{cmd}: {a}"));
                }
            }
        }
        for a in ["undo", "redo"] {
            assert_ne!(need("rf_roster_act", &json!({ "act": a })), Never);
        }
        assert!(missing.is_empty(), "RosterFlow without a permission in together/relay.rs:\n{}", missing.join("\n"));
        // the tutorial is the player's launcher's own: never relayed, never run from afar
        for c in arms_in(tutorial, "pub fn call(") {
            assert!(!relayed_rf(&c) && need(&c, &json!({ "act": "end" })) == Never, "{c}");
        }
        let planner = omsi_launcher_lib::company::together::Role::Planner.preset();
        let ask = |cmd: &str, args: Value, perms: &[Perm]| need(cmd, &args).met(perms);
        // a planner plans; a dispatcher looks; a driver sees nothing of it
        assert!(ask("rf_act", json!({ "act": "drop" }), planner) && ask("rf_roster_act", json!({ "act": "fill" }), planner));
        assert!(ask("rf_board", json!({}), &[Perm::Service]) && !ask("rf_act", json!({ "act": "save" }), &[Perm::Service]));
        assert!(!ask("rf_on", json!({}), &[Perm::Drive]));
        // a timetable is the lines'; a duty given to a player, the assigner's
        assert!(ask("rf_tp_act", json!({ "act": "save" }), &[Perm::Lines]) && !ask("rf_tp_act", json!({ "act": "save" }), &[Perm::Plans]));
        assert!(ask("rf_tp_act", json!({ "act": "line" }), &[Perm::Plans]));
        assert!(ask("rf_roster_act", json!({ "act": "give_driver", "who": { "guest": 2 } }), &[Perm::Assign]));
        assert!(!ask("rf_roster_act", json!({ "act": "give_driver", "who": { "guest": 2 } }), &[Perm::Plans]));
        assert!(ask("rf_roster_act", json!({ "act": "give_driver", "who": { "staff": 4 } }), &[Perm::Plans]));
        // the planning's sheet of a line opens for who may look (its changes are the planners')
        assert!(ask("co_more_act", json!({ "act": "plan_timetable" }), &[Perm::Lines]));
        assert!(ask("co_more_view", json!({ "view": "planning" }), &[Perm::Service]));
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
        // (his duty card: asked of the boss's launcher, which shows him his own duties')
        assert!(ask("co_view", json!({ "view": "dutycard" }), &driver));
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

    /// A page run for a player on the boss's session: his own corner and popup, the boss's left
    /// as they were; his shell shows his pages.
    #[test]
    fn a_players_pages_keep_their_own_state_and_leave_the_bosss() {
        use super::super::super::company::{relayed, Session, Viewer};
        use omsi_launcher_lib::company::{found, Founding, GameMode};
        super::super::super::company::words::set_language("en");
        let c = found(&Founding { name: "Stadtbus".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let mut s = Session::default();
        s.company = Some(c);
        s.say("for the boss", false);
        let mut v = Viewer::default();
        let planner = omsi_launcher_lib::company::together::Role::Planner.preset();
        let sh = relayed(&mut s, &mut v, "co_tick", &json!({}), planner).unwrap();
        assert_eq!(sh["remote"]["perms"].as_array().map(Vec::len), Some(planner.len()));
        assert!(sh["notices"].as_array().is_some_and(Vec::is_empty), "the boss's corner is not the player's");
        assert!(sh["pages"].as_array().unwrap().iter().all(|t| tab_need(t.as_u64().unwrap() as usize).met(planner)));
        assert_eq!(s.notices.len(), 1);
        // (what is said to the player stays his, for his next shell)
        let lines = relayed(&mut s, &mut v, "co_view", &json!({ "view": "lines" }), planner).unwrap();
        assert!(lines.get("none").is_none());
        assert_eq!(s.notices.len(), 1);
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

    #[test]
    fn only_who_runs_the_day_s_service_writes_to_the_drivers() {
        let service = omsi_launcher_lib::company::together::Role::Planner.preset();
        for act in ["ops_msg", "ops_msg_ack"] {
            let n = need("co_act", &json!({ "act": act }));
            assert!(n.met(&[Perm::Service]) && n.met(service), "{act}");
            assert!(!n.met(&[Perm::Drive]) && !n.met(&[Perm::Plans, Perm::Finances]), "{act}");
            assert_eq!(n.refusal()["perm"], json!("service"));
        }
    }
}
