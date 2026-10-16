//! The direction of the dependencies between openOMSI's crates and Omsi-Hub's
//! (docs/omsihub/ADDON_PLAN.md, phase 2): Omsi-Hub is an add-on of openOMSI, so no crate of the
//! game may depend on an Omsi-Hub crate. Omsi-Hub's crates may use openOMSI's library crates.
//!
//! * Omsi-Hub's crates are `omsihub-*`.
//! * The launcher's crates (`omsi-launcher-core`, `-api`, `-model`) are openOMSI's own launcher,
//!   which Omsi-Hub's page extends; the game reaches them as upstream's game does
//!   (`UPSTREAM`), and nothing else of the game may.
//! * Every other `omsi-*` crate is the game or one of its libraries.
//!
//! `STILL_BUNDLED` lists what the migration has not moved out of the game yet, each with the
//! reason; it only shrinks. The test reads the `Cargo.toml` files themselves (`\r\n` as well),
//! so it needs no network and no `cargo metadata`.

use std::path::{Path, PathBuf};

/// Dependencies upstream openOMSI's own crates have on its launcher crates.
const UPSTREAM: &[(&str, &str)] = &[
    ("omsi-app", "omsi-launcher-core"),
    ("omsi-app", "omsi-launcher-api"),
    ("omsi-app", "omsi-launcher-model"),
    ("omsi-platform", "omsi-launcher-core"),
];

/// What the game still carries of Omsi-Hub until the migration moves it (see the plan's status
/// table for why each stays).
const STILL_BUNDLED: &[(&str, &str)] = &[
    // the navigator's duty sheet, the stop signs, the loading screen, the launcher service's
    // editors (steps 4, 7 and 8, E12)
    ("omsi-app", "omsihub-game"),
];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The crate's name and every crate it names in any of its dependency tables.
fn manifest(path: &Path) -> (String, Vec<String>) {
    let text = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    let mut name = String::new();
    let mut deps = Vec::new();
    let mut section = String::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_string();
            // (`[dependencies.wgpu]`: one dependency as a table of its own)
            if let Some(d) = section.rsplit_once("dependencies.").map(|x| x.1) {
                deps.push(d.trim().to_string());
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim().trim_matches('"');
        if section == "package" && key == "name" && name.is_empty() {
            name = value.trim().trim_matches('"').to_string();
        } else if section.ends_with("dependencies") && !key.starts_with('#') {
            deps.push(key.to_string());
        }
    }
    (name, deps)
}

fn crates() -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for dir in [workspace().join("crates"), workspace().join("tools")] {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let m = e.path().join("Cargo.toml");
            if m.is_file() {
                out.push(manifest(&m));
            }
        }
    }
    out.sort();
    out
}

fn is_omsihub(name: &str) -> bool {
    name.starts_with("omsihub-") || name.starts_with("omsihub_")
}

fn is_launcher(name: &str) -> bool {
    name.starts_with("omsi-launcher-")
}

#[test]
fn no_crate_of_openomsi_depends_on_omsi_hub() {
    let all = crates();
    assert!(all.iter().any(|c| c.0 == "omsi-app"), "the workspace's crates were not found");
    let mut wrong = Vec::new();
    for (name, deps) in &all {
        if is_omsihub(name) || is_launcher(name) {
            continue;
        }
        for d in deps {
            let edge = (name.as_str(), d.as_str());
            let allowed = UPSTREAM.contains(&edge) || STILL_BUNDLED.contains(&edge);
            if (is_omsihub(d) || is_launcher(d)) && !allowed {
                wrong.push(format!("{name} -> {d}"));
            }
        }
    }
    assert!(wrong.is_empty(), "openOMSI crates depending on Omsi-Hub (or on the launcher beyond upstream's own edges):\n{}", wrong.join("\n"));
}

#[test]
fn omsi_hub_uses_openomsi_s_libraries_never_the_game() {
    let wrong: Vec<String> = crates().into_iter().filter(|(n, _)| is_omsihub(n) || is_launcher(n)).flat_map(|(n, d)| d.into_iter().filter(|d| d == "omsi-app").map(move |d| format!("{n} -> {d}"))).collect();
    assert!(wrong.is_empty(), "Omsi-Hub crates depending on the game program:\n{}", wrong.join("\n"));
}

#[test]
fn the_lists_name_only_edges_that_exist() {
    // (a stale entry would let a dependency come back unnoticed)
    let all = crates();
    for (from, to) in UPSTREAM.iter().chain(STILL_BUNDLED) {
        assert!(all.iter().any(|(n, d)| n == from && d.iter().any(|x| x == to)), "{from} -> {to} is listed but not there: take it off the list");
    }
}
