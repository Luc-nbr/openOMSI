//! The options' Add-ons page: a switch for each add-on of the catalogue the game comes with
//! (`assets/addons.json`, see `omsi_plugin::addons`). Switched on the first time, the add-on's
//! archive is downloaded in the background, checked and unpacked into the content folder's
//! `Plugins`; switched off, it goes on `Plugins/disabled.txt`. The plugins are loaded when the
//! game starts, so a switch takes effect the next time.

use crate::App;
use omsi_plugin::addons::{self, Addon};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const CATALOGUE: &str = include_str!("../../../assets/addons.json");

/// Most an archive may weigh (as `omsi_plugin::addons` takes it).
const MAX_DOWNLOAD: u64 = 64 << 20;

fn catalogue() -> &'static [Addon] {
    static LIST: OnceLock<Vec<Addon>> = OnceLock::new();
    LIST.get_or_init(|| addons::parse_catalogue(CATALOGUE))
}

fn find(id: &str) -> Option<&'static Addon> {
    catalogue().iter().find(|a| a.id == id)
}

/// The downloads under way or failed, by add-on: what the row says of it.
fn jobs() -> &'static Mutex<HashMap<String, Job>> {
    static JOBS: OnceLock<Mutex<HashMap<String, Job>>> = OnceLock::new();
    JOBS.get_or_init(Default::default)
}

#[derive(Clone)]
enum Job {
    Downloading,
    Failed(String),
}

/// The content folder's plugins folder (in the case it has on disk).
fn plugins_dir() -> Option<PathBuf> {
    let content = omsi_cfg::content_dir()?;
    Some(omsi_plugin::resolve_path(&content, "plugins").filter(|p| p.is_dir()).unwrap_or_else(|| content.join("Plugins")))
}

/// The page's rows, one switch an add-on (none: the catalogue lists none).
pub(crate) fn rows(app: &App) -> Vec<(String, String)> {
    catalogue()
        .iter()
        .filter_map(|a| {
            let job = jobs().lock().ok().and_then(|j| j.get(&a.id).cloned());
            let desc = match job {
                Some(Job::Downloading) => omsi_ui::tr("Downloading...").to_string(),
                Some(Job::Failed(why)) => omsi_ui::tr("The download failed: %{reason}").replace("%{reason}", &why),
                None => a.description.clone(),
            };
            crate::game_lists::switch_row(app, &format!("addon:{}", a.id), &a.name, &desc)
        })
        .collect()
}

/// Whether the add-on `id` is switched on (or on its way); None: the catalogue has no such
/// add-on.
pub(crate) fn is_on(id: &str) -> Option<bool> {
    let a = find(id)?;
    let downloading = jobs().lock().is_ok_and(|j| matches!(j.get(id), Some(Job::Downloading)));
    Some(downloading || plugins_dir().is_some_and(|p| addons::enabled(&p, a)))
}

/// Switch the add-on `id` on (downloading it the first time) or off.
pub(crate) fn switch(app: &mut App, id: &str, on: bool) {
    let (Some(a), Some(plugins)) = (find(id), plugins_dir()) else { return };
    if !on || addons::installed(&plugins, a) {
        if let Err(e) = addons::set_enabled(&plugins, a, on) {
            log::warn!("add-on {id}: {e}");
        }
    } else if let Ok(mut j) = jobs().lock() {
        if matches!(j.get(id), Some(Job::Downloading)) {
            return;
        }
        j.insert(id.to_string(), Job::Downloading);
        let a = a.clone();
        std::thread::spawn(move || {
            let done = download(&a.url).and_then(|bytes| addons::install(&plugins, &a, &bytes));
            if let Ok(mut j) = jobs().lock() {
                match done {
                    Ok(()) => {
                        log::info!("add-on {} {} installed into {}", a.id, a.version, plugins.display());
                        j.remove(&a.id);
                    }
                    Err(e) => {
                        log::warn!("add-on {}: {e}", a.id);
                        j.insert(a.id.clone(), Job::Failed(e));
                    }
                }
            }
        });
    }
    app.service_msg = Some((omsi_ui::tr("The add-ons change when the game starts the next time").to_string(), 4.0));
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url).timeout(std::time::Duration::from_secs(600)).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_reader().take(MAX_DOWNLOAD + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
