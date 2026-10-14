//! A change of an own line waiting for its day (`ownline::Pending`, Trello "Tauri 6/6"), as the
//! game-drawn launcher's company takes it every frame (`company::changes_due` and `refill`): on
//! that day the map's timetable and the company's line take it - the line written as the line
//! editor writes it (`lineeditor::take_effect_in_timetable`) - and the tours it changed are
//! planned anew once the timetable is read again. A change whose timetable could not be written
//! keeps waiting (the map's timetable and the company's line stay as one), tried again the next
//! day.

use super::*;
use omsi_launcher_lib::lines as reg;
use omsi_launcher_lib::{linehof, owndepot};
use std::path::{Path, PathBuf};

/// The changes whose day has come, taken (`changes_due`).
pub(super) fn changes_due(s: &mut Session) {
    let Some(c) = s.company.as_ref() else { return };
    if s.closing {
        return;
    }
    let (company, date) = (c.id.clone(), c.date.clone());
    let due: Vec<u64> = co::ownline::due(c).into_iter().filter(|id| !s.unwritten.iter().any(|u| u.0 == company && u.1 == *id && u.2 == date)).collect();
    if due.is_empty() {
        return;
    }
    let map = c.map.clone();
    let mut failed = Vec::new();
    let mut wrote = false;
    for id in due {
        if let Err(e) = take_effect_in_timetable(&map, id) {
            let t = format!("{}: {e}", tr("The timetable files could not be written"));
            s.say(t, true);
            failed.push((company.clone(), id, date.clone()));
            continue;
        }
        wrote = true;
        if let Ok(Some((name, _))) = act(s, |c| Ok(co::ownline::take_effect(c, id))) {
            s.refill.push(name);
        }
    }
    if wrote {
        reload_timetable(s);
        // (the day's timetable as it is now, before the tours are planned)
        s.today = None;
    }
    s.unwritten.retain(|u| u.2 == date);
    s.unwritten.extend(failed);
}

/// The tours of lines whose change took effect, planned anew on the company's day (as "Fill the
/// roster" does, for them only), and said (`refill`).
pub(super) fn refill(s: &mut Session) {
    if s.refill.is_empty() {
        return;
    }
    let (Some(c), Some(t)) = (s.company.as_ref(), s.today.as_ref()) else { return };
    // (a timetable that could not be read plans nothing: the tours wait for one that can)
    if t.map != c.map || t.date != c.date || t.error.is_some() {
        return;
    }
    let tours = co::network::tours_of_day(c, &t.lines, &c.date);
    let date = c.date.clone();
    for name in std::mem::take(&mut s.refill) {
        let number = s.company.as_ref().and_then(|c| c.lines.iter().find(|x| x.name == name)).map(|x| x.number.clone()).unwrap_or_default();
        let ours = tours.clone();
        let Ok(f) = act(s, |c| Ok(co::plan::fill_line(c, &date, ours, &name))) else { continue };
        let mut text = trf("Line %{n} runs its new timetable from today: %{d} duties and %{b} buses given anew.", &[("n", &number), ("d", &f.duties.to_string()), ("b", &f.buses.to_string())]);
        let open = more::open_text(&f);
        if !open.is_empty() {
            text = format!("{text} {open}");
        }
        s.say(text, false);
    }
    s.plan = None;
}

/// The line `id` of the map's registry as it runs from today: its waiting change made its
/// version, the map's timetable, the line's depot files and the player's own written
/// (`lineeditor::take_effect_in_timetable`).
fn take_effect_in_timetable(map: &str, id: u64) -> Result<(), String> {
    let folder = reg::map_folder(map);
    let path = reg::registry_path(&folder);
    let mut r = reg::load_registry(&path);
    let Some(x) = r.line_mut(id) else { return Ok(()) };
    x.live = None;
    x.pending_from.clear();
    let content = core::content_dir().ok_or("no content folder")?;
    let root = core::load_config().root;
    let global = omsi_cfg::find_in_roots(map).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(Path::new(&root), map));
    let map_dir = global.parent().map(Path::to_path_buf).unwrap_or_else(PathBuf::new);
    // the depot files follow the version that runs now, as a save writes them: the codes given
    // before the registry is saved, the blocks after the timetable
    let groups = linehof::group_depots(&map_dir);
    let (bases, original) = core::depot_roots();
    let plan = linehof::prepare(&mut r, &groups, &bases);
    owndepot::assign(&mut r, &owndepot::dir(), &groups, &bases);
    reg::save_registry(&path, &r)?;
    reg::export_to_map(&content, &map_dir, &r)?;
    let (_, err) = linehof::write(&content, original.as_deref(), &r, &groups, &plan);
    let (_, own_err) = owndepot::write_lines(&r, &owndepot::dir(), Some(&content));
    let err = err.or(own_err);
    if let Some(e) = &err {
        core::log_to_file(&format!("company: the depot files of line {id} as it runs now: {e}"));
    }
    // (the registry knows its texts are written, as `lineeditor::texts_written` keeps it)
    if err.is_none() && r.texts < reg::TEXTS_VERSION {
        r.texts = reg::TEXTS_VERSION;
        if let Err(e) = reg::save_registry(&path, &r) {
            core::log_to_file(&format!("company: the registry {} not saved after its depot files: {e}", path.display()));
        }
    }
    omsi_cfg::content_changed();
    Ok(())
}
