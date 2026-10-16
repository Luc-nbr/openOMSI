//! The updates in the desktop launcher (Trello "Tauri 6/6"), as our game-drawn launcher runs
//! them (`crates/omsi-launcher-ui/src/update.rs` over `omsi_platform::updater`): a look at
//! this fork's GitHub releases a second after the start, again every half hour and when a game
//! started from here has ended; the offer as a dialog over the page, its download and its
//! installation; "install updates without asking" while no game runs; and after the swap the
//! new launcher started and this one ended. The page asks for the state about once a second
//! (`hub_update`) and answers the dialog (`hub_update_act`); every text comes translated.

use super::company::words::{set_language, tr};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_platform::updater::{self, Status, Updater};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The updater of this launcher, and when the launcher started.
struct State {
    u: Updater,
    started: Instant,
    /// "Updated to Omsi-Hub x" not said yet (`updater::just_updated`, read once).
    updated: Option<String>,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        // (the files an update set aside stay until this version has drawn its first frames:
        // the start guard goes back to them otherwise - the game's own does that)
        let updated = updater::just_updated();
        Mutex::new(State { u: Updater::default(), started: Instant::now(), updated })
    })
    .lock()
    .unwrap_or_else(|e| e.into_inner())
}

fn setting(key: &str, default: bool) -> bool {
    core::get_settings().ok().and_then(|v| v.get(key).and_then(Value::as_bool)).unwrap_or(default)
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    Some(match cmd {
        "hub_update" => Ok(tick()),
        "hub_update_act" => {
            act(args.get("what").and_then(Value::as_str).unwrap_or(""));
            Ok(tick())
        }
        _ => return None,
    })
}

/// What the dialog's buttons do: Update now, Not now / Close, Try again, Check now (the
/// settings), What's new and the releases on GitHub.
fn act(what: &str) {
    let mut s = state();
    match what {
        "install" => {
            if let Status::Available(r) | Status::Downloaded(r) = s.u.status() {
                s.u.install(r);
            }
        }
        "dismiss" => s.u.dismiss(),
        "check" => s.u.check(),
        "page" => {
            if let Status::Available(r) = s.u.status() {
                updater::open_url(&r.page);
            }
        }
        "github" => updater::open_url(&format!("{}/releases/latest", updater::REPO_URL)),
        "repo" => updater::open_url(updater::REPO_URL),
        _ => {}
    }
}

/// Once a second from the page (`Launcher::update_tick`): the looks for an update, the
/// installation chosen beforehand, the new launcher started once it is in place - and what the
/// page shows of it.
fn tick() -> Value {
    let mut guard = state();
    let s = &mut *guard;
    let looking = setting("update_check", true) && !omsi_cfg::flags::OMSI_NO_UPDATE.is_set();
    let u = &mut s.u;
    if !u.checked_once && s.started.elapsed() > Duration::from_secs(1) {
        if looking {
            u.check();
        } else {
            u.checked_once = true;
        }
    }
    // (a game started from here is a program of its own: nothing is put in its place while it
    // runs - once it ends, the update the game downloaded in the background goes in)
    let game_running = core::list_instances().iter().any(|i| i.running);
    if u.game_was_running && !game_running && looking {
        core::log_to_file("update: the game has ended - looking for an update");
        u.dismissed_version = None;
        u.auto_started = false;
        if let Status::Failed(_) | Status::UpToDate = u.status() {
            u.dismiss();
        }
        u.check();
    }
    u.game_was_running = game_running;
    // and every half hour while the launcher stays open
    let idle = matches!(u.status(), Status::Idle | Status::UpToDate) || (matches!(u.status(), Status::Failed(_)) && u.dismissed);
    if looking && idle && u.last_check.is_some_and(|t| t.elapsed() > Duration::from_secs(30 * 60)) {
        if let Status::Failed(_) | Status::UpToDate = u.status() {
            u.dismiss();
        }
        u.check();
    }
    u.poll();
    // (an offer put aside comes back for a newer version only)
    if let Status::Available(r) = u.status() {
        if u.dismissed && u.dismissed_version.as_deref() != Some(r.version.as_str()) {
            u.dismissed = false;
        } else if !u.dismissed && u.dismissed_version.as_deref() == Some(r.version.as_str()) {
            u.dismissed = true;
        }
    }
    let auto = setting("update_auto", false);
    let mut note = None;
    match u.status() {
        // "install updates without asking" (not while a game runs)
        Status::Available(r) if auto && !u.dismissed && !u.auto_started && !game_running => {
            u.auto_started = true;
            core::log_to_file(&format!("update: installing {} by itself (update_auto)", r.version));
            u.install(r);
        }
        Status::Restarting(r) if !u.relaunched => {
            u.relaunched = true;
            match updater::install_place().and_then(|p| updater::relaunch(&p)) {
                Ok(()) => {
                    core::log_to_file(&format!("update: {} installed, the new launcher starts", r.version));
                    // (this launcher ends once the page has its answer: the helper it started
                    // ends with it, its input closed)
                    std::thread::spawn(|| {
                        std::thread::sleep(Duration::from_millis(400));
                        std::process::exit(0);
                    });
                }
                Err(e) => note = Some(tr("Omsi-Hub %{version} is installed; start it again yourself (%{error}).").replace("%{version}", &r.version).replace("%{error}", &e.to_string())),
            }
        }
        _ => {}
    }
    let status = u.status();
    // the dialog over the page (`update_dialog_open`: while a game runs the offer waits)
    let open = !u.game_was_running
        && match &status {
            Status::Available(_) | Status::Failed(_) => !u.dismissed,
            Status::Downloading { .. } | Status::Installing(_) | Status::WaitingForInstaller(_) | Status::Restarting(_) => true,
            _ => false,
        };
    let current = updater::current_version();
    let dialog = open.then(|| match &status {
        Status::Available(rel) => json!({
            "kind": "available", "icon": "system_update",
            "title": tr("Omsi-Hub %{version} is available").replace("%{version}", &rel.version),
            "text": tr("You have %{current}. Update now? The launcher downloads the new version (%{size}) from GitHub, puts it in place of this one and starts again - your mods and settings stay as they are.").replace("%{current}", current).replace("%{size}", &mb(rel.size)),
            "auto": auto,
        }),
        Status::Downloading { release, done, total } => json!({
            "kind": "busy", "icon": "download",
            "title": tr("Downloading Omsi-Hub %{version}").replace("%{version}", &release.version),
            "text": tr("%{done} of %{total} from %{source}").replace("%{done}", &mb(*done)).replace("%{total}", &mb(*total)).replace("%{source}", &format!("github.com/{}", updater::REPO)),
            "frac": if *total > 0 { *done as f64 / *total as f64 } else { 0.0 },
        }),
        Status::Installing(release) | Status::Restarting(release) => json!({
            "kind": "busy", "icon": "install_desktop",
            "title": tr("Installing Omsi-Hub %{version}").replace("%{version}", &release.version),
            "text": tr("The new version is put in place; the launcher starts again in a moment."),
            "frac": 1.0,
        }),
        Status::Failed(msg) => json!({ "kind": "failed", "icon": "error", "title": tr("Not updated"), "text": msg }),
        _ => Value::Null,
    });
    // the Settings page's line beside "Check now"
    let line = match &status {
        Status::UpToDate if updater::is_test_build(current) => format!("{current} is a test build: it is not updated"),
        Status::UpToDate => tr("%{version} is the latest version").replace("%{version}", current),
        Status::Available(rel) => tr("%{version} is available").replace("%{version}", &rel.version),
        Status::Failed(_) => tr("The last check failed"),
        _ => tr("This is Omsi-Hub %{version}").replace("%{version}", current),
    };
    let busy = matches!(status, Status::Checking | Status::Downloading { .. } | Status::Installing(_) | Status::WaitingForInstaller(_) | Status::Restarting(_));
    let updated = s.updated.take().map(|v| tr("Updated to Omsi-Hub %{version}").replace("%{version}", &v));
    json!({
        "dialog": dialog,
        "line": line,
        "busy": busy,
        "repo": format!("github.com/{}", updater::REPO),
        "updated": updated,
        "note": note,
        "shown": updater::shown_version(current),
    })
}

#[cfg(test)]
mod tests {
    /// The releases are this fork's alone, and a test build (`-beta<N>`) shows as "Beta N" and
    /// is never offered one: it would replace the build being tested.
    #[test]
    fn the_fork_s_releases_and_a_test_build() {
        use omsi_platform::updater as u;
        assert_eq!(u::REPO, "Luc-nbr/omsi-hub");
        assert!(u::REPO_URL.ends_with("/Luc-nbr/omsi-hub"));
        assert!(u::is_test_build("0.2.37-beta51"));
        assert_eq!(u::shown_version("0.2.37-beta51"), "Beta 51");
        assert_eq!(u::shown_version("0.2.37.70"), "0.2.37.70");
    }
}
