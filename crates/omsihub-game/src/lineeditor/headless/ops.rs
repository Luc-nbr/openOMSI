//! The page's changes of the line editor (`op`), each as the game-drawn launcher's panels make
//! it - the left panel's (the map, the lines, the line's settings, saving), the Stops tab's, the
//! map's (a stop clicked, a leg dragged), the Displays', the Timetable's, the Kind's, the
//! table's and the company's.

use super::*;

type R = Result<(), String>;

fn s_of(req: &Value, k: &str) -> String {
    req.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn u_of(req: &Value, k: &str) -> usize {
    req.get(k).and_then(Value::as_u64).unwrap_or(0) as usize
}

fn f_of(req: &Value, k: &str) -> f32 {
    req.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn b_of(req: &Value, k: &str) -> bool {
    req.get(k).and_then(Value::as_bool).unwrap_or(false)
}

pub(super) fn apply(s: &mut Session, op: &str, req: &Value) -> R {
    match op {
        "view" => Ok(()),
        "open" => open(s, req),
        // (the page left: nothing of it is kept here)
        "leave" => Ok(()),
        _ if op.starts_with("t_") => table(s, op, req),
        _ if op.starts_with("c_") => super::company::act(s, op, req),
        _ => edit(s, op, req),
    }
}

/// The page opened (`open_for_company`, `open_new_for_company_with`,
/// `open_map_line_for_company`, or the editor's own tile): the maps it lists, the game's folder,
/// the day, the Drive page's map - and, for the bus company, its id and what to show.
fn open(s: &mut Session, req: &Value) -> R {
    s.maps = req.get("maps").and_then(Value::as_array).map(|a| a.iter().filter_map(|m| Some((m.get(0)?.as_str()?.to_string(), m.get(1)?.as_str()?.to_string()))).collect()).unwrap_or_default();
    s.root = PathBuf::from(s_of(req, "root"));
    s.date = s_of(req, "date");
    let v = &mut s.v;
    let company = s_of(req, "company");
    if company.is_empty() {
        v.leave_company();
        s.company = None;
    } else {
        let c = co::store::load(&core::data_dir(), &company).map_err(|e| format!("{e:#}"))?;
        let runs = c.lines.iter().filter(|x| !x.own).map(|x| x.name.clone()).collect();
        let rosterflow = co::autopilot::rosterflow(&c);
        let line = req.get("line").and_then(Value::as_u64);
        let days: Option<Vec<DayPattern>> = req.get("new_days").filter(|d| !d.is_null()).and_then(|d| serde_json::from_value(d.clone()).ok());
        let map_line = s_of(req, "map_line");
        let mut fc = ForCompany { id: c.id.clone(), name: c.name.clone(), map: c.map.clone(), colours: c.colours.clone(), depot: c.depot.clone(), start: Some(line), stop_lines: None, cache: None, details: false, runs, start_map: None, days, rosterflow };
        if !map_line.is_empty() {
            (fc.start, fc.start_map) = (None, Some(map_line));
        }
        v.company = Some(fc);
        v.company_act = None;
        v.go_timeplan = None;
        v.loaded = None;
        s.company = Some(c);
    }
    let maps = s.maps.clone();
    if maps.is_empty() {
        return Ok(());
    }
    // (working for the bus company: its map, and no other)
    let company_map = s.v.company.as_ref().map(|f| f.map.clone());
    let k = if let Some(cm) = company_map {
        match maps.iter().position(|m| same_map(&m.1, &cm)) {
            Some(i) => i,
            None => {
                s.v.leave_company();
                s.company = None;
                return Err(omsi_ui::tr("The company's map is not installed.").into_owned());
            }
        }
    } else if s.v.loaded.is_none() {
        let chosen = s_of(req, "choice_map");
        maps.iter().position(|m| m.1 == chosen).unwrap_or(0)
    } else {
        s.v.map.min(maps.len() - 1)
    };
    if s.v.loaded.as_deref() != Some(maps[k].1.as_str()) {
        load(s, k);
    }
    Ok(())
}

/// What the panels and the map ask for.
fn edit(s: &mut Session, op: &str, req: &Value) -> R {
    let v = &mut s.v;
    let mut status: Option<(String, bool)> = None;
    match op {
        // --- the left panel ------------------------------------------------------------
        "pick_map" => {
            let k = u_of(req, "k");
            if k != v.map || v.loaded.as_deref() != s.maps.get(k).map(|m| m.1.as_str()) {
                if v.dirty {
                    status = Some(("The changes to the line on the map before were not saved".into(), true));
                }
                load(s, k);
            }
        }
        "pick_line" => {
            let id = req.get("id").and_then(Value::as_u64).unwrap_or(0);
            if v.sel != Some(id) {
                v.leave_fresh();
                (v.sel, v.dir, v.sel_stop, v.delete_armed) = (Some(id), 0, None, false);
                v.table_reset();
            }
        }
        "pick_map_line" => {
            if let Some(x) = maplines::pick(v, &s_of(req, "ttl")) {
                status = Some(x);
            }
        }
        "new_line" => {
            v.leave_fresh();
            new_line(v);
            status = Some(("Now click the line's stops on the map, in the order the bus calls at them".into(), false));
        }
        "number" | "name" | "colour" | "ai_group" | "title" => {
            let value = s_of(req, "value");
            if let Some(line) = v.line_mut() {
                match op {
                    "number" => line.number = value,
                    "name" => line.name = value,
                    "colour" => line.colour = value,
                    "ai_group" => line.ai_group = value,
                    _ => line.title = value,
                }
                v.touched();
            }
        }
        "live_displays" => {
            if let Some(line) = v.line_mut() {
                line.live_displays = b_of(req, "on");
                v.touched();
            }
        }
        "colour_open" => v.colour_open = !v.colour_open,
        "save" => status = Some(save(s)),
        "delete" => {
            let draft = v.company.is_some();
            if v.delete_armed {
                status = Some(delete(s));
            } else {
                v.delete_armed = true;
                status = Some((if draft { "Press \"Delete draft\" again: the draft goes" } else { "Press \"Delete line\" again: the line and its files go" }.into(), false));
            }
        }
        "drive" => status = drive(s),
        "restore" => {
            if !v.restore_armed {
                v.restore_armed = true;
                status = Some(("Press again: the edit goes and the game runs the map's own line as before".into(), false));
            } else if v.company.is_some() {
                return super::company::act(s, "c_restore_map", req);
            } else {
                status = Some(restore(s));
            }
        }
        "save_map" => {
            if v.company.is_some() {
                return super::company::act(s, "c_save_map", req);
            }
            status = Some(save_map(s));
        }
        // --- the right panel: tabs, direction, stops ------------------------------------
        "tab" => v.tab = u_of(req, "t").min(3),
        "dir" => {
            (v.dir, v.sel_stop) = (u_of(req, "d"), None);
            v.revision += 1;
        }
        "sel_stop" => {
            let i = req.get("i").and_then(Value::as_u64).map(|x| x as usize);
            v.sel_stop = if v.sel_stop == i { None } else { i };
            v.revision += 1;
        }
        "up" | "down" => {
            let (d_idx, i) = (v.dir, u_of(req, "i"));
            let Some(n) = v.line().and_then(|l| l.directions.get(d_idx)).map(|d| d.stops.len()) else { return Ok(()) };
            let j = if op == "up" { i.checked_sub(1) } else { (i + 1 < n).then_some(i + 1) };
            if let (Some(j), true) = (j, i < n) {
                let d = &mut v.line_mut().unwrap().directions[d_idx];
                d.stops.swap(i, j);
                for g in &mut d.legs {
                    g.vias.clear();
                }
                v.sel_stop = Some(j);
                v.reroute(d_idx);
            }
        }
        "remove" => {
            let (d_idx, i) = (v.dir, u_of(req, "i"));
            if let Some(d) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)).filter(|d| i < d.stops.len()) {
                d.stops.remove(i);
                // (the legs either side become one, without their dragged points)
                if i < d.legs.len() {
                    d.legs.remove(i);
                } else if !d.legs.is_empty() {
                    d.legs.pop();
                }
                if i > 0 && i - 1 < d.legs.len() {
                    d.legs[i - 1].vias.clear();
                }
                v.sel_stop = None;
                v.reroute(d_idx);
            }
        }
        "shift" => {
            let (d_idx, i, by) = (v.dir, u_of(req, "i"), f_of(req, "by"));
            if let Some(d) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)) {
                d.shift_from(i, by);
                v.touched();
            }
        }
        "clear_vias" => {
            let (d_idx, i) = (v.dir, u_of(req, "i"));
            if let Some(g) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)).and_then(|d| d.legs.get_mut(i)) {
                g.vias.clear();
                v.reroute(d_idx);
            }
        }
        "total" => {
            let (d_idx, by) = (v.dir, f_of(req, "by"));
            if let Some(d) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)) {
                let total = d.minutes();
                if by > 0.0 || total > 1.0 {
                    d.set_total(total + by);
                    v.touched();
                }
            }
        }
        "times_auto" => {
            let d_idx = v.dir;
            if let Some(d) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)) {
                d.manual_times = false;
                d.refresh_times();
                v.touched();
            }
        }
        "reverse" => reverse(v, &mut status),
        "loop" => {
            let d_idx = v.dir;
            let is_loop = v.line().and_then(|x| x.directions.get(d_idx)).is_some_and(Direction::is_loop);
            if let Some(d) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)) {
                if is_loop {
                    d.stops.pop();
                    d.fit_legs();
                } else {
                    d.close_loop();
                }
                v.sel_stop = None;
                status = Some(if is_loop { ("The loop is open: the line ends at its last stop again".into(), false) } else { ("The loop is closed: the bus drives back to its first stop and starts again there".into(), false) });
                v.reroute(d_idx);
            }
        }
        "dest" => {
            let d_idx = v.dir;
            if let Some(d) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)) {
                d.set_destination(&s_of(req, "value"));
                v.touched();
            }
        }
        "dest_pick" => {
            let (d_idx, k) = (v.dir, u_of(req, "k"));
            let termini = v.termini();
            let mine = v.mine();
            if let Some(d) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)) {
                if k == 0 {
                    d.set_destination("");
                } else if let Some(t) = termini.get(k - 1) {
                    d.set_destination(&t.0);
                } else if let Some(m) = mine.get(k - 1 - termini.len()) {
                    d.take_destination(m);
                }
                v.touched();
            }
        }
        // --- the map ---------------------------------------------------------------------
        "click_stop" => click_stop(s, u_of(req, "i")),
        "drag_via" | "drag_new" => {
            let Some(at) = super::world(s, req.get("at").unwrap_or(&Value::Null)) else { return Ok(()) };
            let v = &mut s.v;
            let (d_idx, k) = (v.dir, u_of(req, "k"));
            if let Some(d) = v.line_mut().and_then(|x| x.directions.get_mut(d_idx)) {
                if op == "drag_via" {
                    if let Some(p) = d.legs.get_mut(k).and_then(|g| g.vias.get_mut(u_of(req, "j"))) {
                        *p = at;
                    }
                } else if k < d.legs.len() {
                    // (in its place along the leg: after the points nearer its start)
                    let a = dv(d.stops[k].at);
                    let along = |p: DVec2| (p - a).length();
                    let g = &mut d.legs[k];
                    let pos = g.vias.iter().position(|p| along(dv(*p)) > along(dv(at))).unwrap_or(g.vias.len());
                    g.vias.insert(pos, at);
                }
            }
            v.reroute(d_idx);
        }
        // --- the Displays tab -------------------------------------------------------------
        "sign_mode" => v.sign_advanced = b_of(req, "advanced"),
        "white" => v.white = !v.white,
        "more_texts" => v.more_texts = !v.more_texts,
        "ibis_open" => v.ibis_stops_open = !v.ibis_stops_open,
        "depot_choice" => status = depot_choice(v, u_of(req, "k")),
        "open_depot" => {
            let key = v.line().map(|l| l.depot_file.trim().to_string()).unwrap_or_default();
            s.go = Some(json!({ "depots": key }));
        }
        "use_similar" => {
            let d_idx = v.dir;
            let similar = v.line().map(|l| l.directions[d_idx].destination()).and_then(|dest| v.line_depot().and_then(|depot| depot.similar(&dest).map(|t| t.texture_id.trim().to_string())));
            if let (Some(ident), Some(line)) = (similar, v.line_mut()) {
                line.directions[d_idx].set_destination(&ident);
                v.touched();
            }
        }
        "sign_text" | "sign_reset" | "route" | "ibis_stop" => displays_change(v, op, req),
        "keep_this" | "keep_new" | "drop_mine" => status = Some(mine_change(v, op, req)),
        // --- the Timetable tab -------------------------------------------------------------
        "table_on" => {
            if let Some(line) = v.line_mut() {
                line.table_on = b_of(req, "on");
                if line.table_on && line.table.is_empty() {
                    line.table = reg::table_from_patterns(line);
                }
                v.touched();
            }
        }
        "table_open" => v.table_open = true,
        "run_total" => {
            let (dir, t) = (u_of(req, "d"), f_of(req, "t"));
            if let Some(line) = v.line_mut() {
                line.set_run_total(dir, t.round());
                v.touched();
            }
        }
        "follow" => {
            if let Some(line) = v.line_mut() {
                line.set_back_own(!b_of(req, "on"));
                if line.table_on {
                    for k in 0..DAY_GROUPS.len() {
                        line.refill_table(k, &[1]);
                    }
                    (v.table_sel, v.table_text) = (None, String::new());
                }
                v.touched();
            }
        }
        "pattern" => status = pattern_change(s, req),
        "timeplan" => {
            if let Some(line) = v.line() {
                let to = ToTimeplan { id: line.map.is_none().then_some(line.id), name: line.map.as_ref().map(|m| m.ttl.clone()).unwrap_or_default(), hint: (!reg::has_departures(line)).then_some(NEW_LINE_HINT) };
                if v.dirty {
                    status = Some((omsi_ui::tr("Save the line's changes first (or undo them): the Timeplan opens the line as it is saved.").into_owned(), true));
                } else {
                    v.go_timeplan = Some(to);
                }
            }
        }
        // --- the Kind tab -------------------------------------------------------------------
        "kind" | "typical" | "class" | "take_suits" | "bus_remove" | "pick" | "pick_add" | "holidays" => status = kind_change(s, op, req),
        other => return Err(format!("line editor: what is {other}?")),
    }
    if let Some((t, e)) = status {
        s.say(omsi_ui::tr(&t).into_owned(), e);
    }
    Ok(())
}

/// A stop of the map clicked (`map_pointer`): into the direction (after the stop chosen in the
/// list, else at its end), chosen in the list when the line has it - or, the first stop clicked
/// with the last one chosen, the loop closed.
fn click_stop(s: &mut Session, i: usize) {
    let v = &mut s.v;
    let Some(st) = v.stops.get(i).cloned() else { return };
    let (d_idx, sel) = (v.dir, v.sel_stop);
    let Some(line) = v.line_mut() else { return };
    if d_idx >= line.directions.len() {
        return;
    }
    let d = &mut line.directions[d_idx];
    let last = d.stops.len().checked_sub(1);
    if d.stops.first().is_some_and(|x| x.id == st.id && x.tile == st.tile) && sel.is_some() && sel == last && d.close_loop() {
        v.sel_stop = None;
        v.reroute(d_idx);
        return;
    }
    if let Some(k) = d.stops.iter().position(|x| x.id == st.id && x.tile == st.tile) {
        v.sel_stop = Some(k);
        v.revision += 1;
        return;
    }
    let at_k = match sel {
        Some(k) if k + 1 < d.stops.len() => k + 1,
        _ => d.stops.len(),
    };
    d.stops.insert(at_k, st);
    // (the leg it lands in is two legs now, its dragged points gone)
    d.fit_legs();
    if at_k > 0 && at_k < d.stops.len() - 1 {
        d.legs.insert(at_k - 1, Default::default());
        d.legs.truncate(d.stops.len() - 1);
        if let Some(g) = d.legs.get_mut(at_k) {
            g.vias.clear();
        }
    }
    v.sel_stop = if sel.is_some() { Some(at_k) } else { None };
    v.reroute(d_idx);
}

// --- saving, deleting, driving -----------------------------------------------------------------

/// The registry written, and every line of it that can be into the map's timetable - but a line
/// of the map's opened and not changed (`save`, `save_registry_and_timetable`). What to say.
pub(super) fn save(s: &mut Session) -> (String, bool) {
    let v = &mut s.v;
    let problems = v.problems();
    let fresh = v.fresh.and_then(|id| v.reg.lines.iter().position(|l| l.id == id)).map(|k| v.reg.lines.remove(k));
    let done = save_now(v, &problems);
    if let Some(l) = fresh {
        v.reg.lines.push(l);
    }
    s.go = Some(json!({ "reload_lines": true }));
    done
}

fn save_now(v: &mut LineEditorView, problems: &[reg::Problem]) -> (String, bool) {
    if let Some(line) = v.line_mut() {
        line.modified = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    }
    let prepared = v.prepare_depots();
    if let Err(e) = reg::save_registry(&v.reg_path, &v.reg) {
        return (format!("Not saved: {e}"), true);
    }
    v.dirty = false;
    let Some(content) = core::content_dir() else { return ("Saved in your lines, but there is no content folder to write the timetable into".into(), true) };
    match reg::export_to_map(&content, &v.map_dir, &v.reg) {
        Ok(_) => {
            maplines::saved(v);
            let depot_error = v.write_depots(&content, &prepared);
            texts_written(&v.reg_path, &mut v.reg, depot_error.is_none());
            omsi_cfg::content_changed();
            let number = v.line().map(|x| x.number.clone()).unwrap_or_default();
            if let Some(e) = depot_error {
                (format!("{}: {e}", omsi_ui::tr("The line is saved, but a depot file could not be written (displays and IBIS)")), true)
            } else if let Some(p) = problems.first() {
                (omsi_ui::tr("Line %{n} kept as a draft: %{why}").replace("%{n}", &number).replace("%{why}", &omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string())), false)
            } else {
                (omsi_ui::tr("Line %{n} saved: it is in the map's timetable now (the next game start picks it up)").replace("%{n}", &number), false)
            }
        }
        Err(e) => (format!("{}: {e}", omsi_ui::tr("The timetable files could not be written")), true),
    }
}

/// The shown line out of the registry and its files out of the map's timetable (`delete`).
fn delete(s: &mut Session) -> (String, bool) {
    let v = &mut s.v;
    let Some(id) = v.sel else { return (String::new(), false) };
    let number = v.line().map(|x| x.number.clone()).unwrap_or_default();
    v.reg.lines.retain(|x| x.id != id);
    v.sel = v.first_shown();
    (v.dir, v.sel_stop, v.delete_armed, v.dirty) = (0, None, false, false);
    v.table_reset();
    v.shapes_for = None;
    v.revision += 1;
    let prepared = v.prepare_depots();
    if let Err(e) = reg::save_registry(&v.reg_path, &v.reg) {
        return (format!("Not deleted: {e}"), true);
    }
    let Some(content) = core::content_dir() else { return ("Deleted from your lines, but there is no content folder to take it out of the timetable".into(), true) };
    if let Err(e) = reg::export_to_map(&content, &v.map_dir, &v.reg) {
        return (format!("{}: {e}", omsi_ui::tr("The timetable files could not be written")), true);
    }
    if let Some(e) = v.write_depots(&content, &prepared) {
        return (format!("{}: {e}", omsi_ui::tr("A depot file could not be written (displays and IBIS)")), true);
    }
    texts_written(&v.reg_path, &mut v.reg, true);
    omsi_cfg::content_changed();
    s.go = Some(json!({ "reload_lines": true }));
    (omsi_ui::tr("Line %{n} deleted").replace("%{n}", &number), false)
}

/// The Drive page set to drive the shown line (`drive_it`): the page is told what to choose -
/// a free drive along its first direction on its map - and goes to the start point.
fn drive(s: &mut Session) -> Option<(String, bool)> {
    let v = &s.v;
    let line = v.line()?;
    let Some((free_line, free_route)) = drive_target(v) else {
        return Some(("Save the line first: the game drives what is saved".into(), true));
    };
    s.go = Some(json!({ "drive": { "map": v.reg.global, "free": true, "composed": false, "own_line": true, "my_lines": !line.is_map_line(), "free_line": free_line, "free_route": free_route } }));
    None
}

/// A line of the map's saved (`maplines::save_map`), said as such.
pub(super) fn save_map(s: &mut Session) -> (String, bool) {
    let number = s.v.line().map(|x| x.number.trim().to_string()).unwrap_or_default();
    let problems = s.v.line().map(reg::problems).unwrap_or_default();
    let (text, err) = save(s);
    if err || !problems.is_empty() {
        return (text, err);
    }
    s.v.restore_armed = false;
    (omsi_ui::tr("Line %{n} saved: the game drives it instead of the map's original (the next game start picks it up)").replace("%{n}", &number), false)
}

/// The edit of the shown line of the map's dropped (`maplines::restore`).
pub(super) fn restore(s: &mut Session) -> (String, bool) {
    let v = &mut s.v;
    let Some(line) = v.line().cloned() else { return (String::new(), false) };
    let Some(ttl) = line.map.as_ref().map(|m| m.ttl.clone()) else { return (String::new(), false) };
    v.reg.restore_map_line(&ttl);
    v.running.remove(&line.id);
    v.fresh = None;
    v.sel = v.first_shown();
    (v.dir, v.sel_stop, v.delete_armed, v.restore_armed, v.dirty) = (0, None, false, false, false);
    v.table_reset();
    v.shapes_for = None;
    v.revision += 1;
    let prepared = v.prepare_depots();
    if let Err(e) = reg::save_registry(&v.reg_path, &v.reg) {
        return (format!("{}: {e}", omsi_ui::tr("Not restored")), true);
    }
    let Some(content) = core::content_dir() else { return ("Taken out of your lines, but there is no content folder to put the map's line back into".into(), true) };
    if let Err(e) = reg::export_to_map(&content, &v.map_dir, &v.reg) {
        return (format!("{}: {e}", omsi_ui::tr("The timetable files could not be written")), true);
    }
    maplines::saved(v);
    if let Some(e) = v.write_depots(&content, &prepared) {
        return (format!("{}: {e}", omsi_ui::tr("A depot file could not be written (displays and IBIS)")), true);
    }
    texts_written(&v.reg_path, &mut v.reg, true);
    omsi_cfg::content_changed();
    s.go = Some(json!({ "reload_lines": true }));
    (omsi_ui::tr("Line %{n} is the map's own again: the game runs its original as before").replace("%{n}", &ttl), false)
}

// --- the Displays tab ---------------------------------------------------------------------------

/// The depot file chosen for the line (`depot_choice`): the map's (0), one of the player's own,
/// or a new one of his own made from the map's (the last).
fn depot_choice(v: &mut LineEditorView, k: usize) -> Option<(String, bool)> {
    let own: Vec<(String, String)> = v.own_entries().iter().map(|e| (e.key.clone(), e.name.clone())).collect();
    let mut status = None;
    let key = if k == 0 {
        Ok(String::new())
    } else if let Some((key, _)) = own.get(k - 1) {
        Ok(key.clone())
    } else {
        let base = v.map_depot_bytes(&v.reg.global.clone());
        let name = omsi_ui::tr("%{map} - mine").replace("%{map}", &v.folder);
        owndepot::create_from(&owndepot::dir(), &name, &v.folder, base.as_deref()).map(|p| {
            status = Some((omsi_ui::tr("Your depot file %{name} is made from the map's: the line's destinations go into it when it is saved").replace("%{name}", &name), false));
            p.file_stem().unwrap_or_default().to_string_lossy().into_owned()
        })
    };
    match key {
        Ok(key) => {
            if let Some(l) = v.line_mut() {
                l.depot_file = key;
            }
            v.own_changed();
            v.touched();
            status
        }
        Err(e) => Some((e, true)),
    }
}

/// A text of the Advanced view typed (`sign_text`, `k`), the texts made again (`sign_reset`),
/// the IBIS route (`route`) or a stop's name on the IBIS (`ibis_stop`, `i`).
fn displays_change(v: &mut LineEditorView, op: &str, req: &Value) {
    let d_idx = v.dir;
    let depot = v.line_depot();
    let Some(d) = v.line().and_then(|l| l.directions.get(d_idx)).cloned() else { return };
    let dest = d.destination();
    let ready = depot.is_some() && d.stops.len() >= 2 && !dest.is_empty();
    let new = depot.as_ref().filter(|_| ready).map(|x| x.sign_of(&d).1).unwrap_or(false);
    let defaults = match &depot {
        Some(x) if ready && new => x.sign_defaults(&dest),
        _ => Vec::new(),
    };
    let Some(dd) = v.line_mut().and_then(|l| l.directions.get_mut(d_idx)) else { return };
    match op {
        "sign_text" => {
            let mut vals = dd.sign.clone();
            vals.resize(defaults.len(), String::new());
            if let Some(x) = vals.get_mut(u_of(req, "k")) {
                *x = s_of(req, "value");
            }
            // (all empty: the defaults, which follow the destination)
            if vals.iter().all(|s| s.is_empty()) {
                vals.clear();
            }
            dd.sign = vals;
        }
        "sign_reset" => dd.sign.clear(),
        "route" => dd.ibis_route = s_of(req, "value").trim().parse().unwrap_or(0),
        _ => {
            if let Some(st) = dd.stops.get_mut(u_of(req, "i")) {
                st.ibis = s_of(req, "value");
            }
        }
    }
    v.touched();
}

/// The player's own destinations (`displays_advanced`): this destination kept, one typed kept,
/// one taken out - in his depot file of the map (made from the map's for the first). What to say.
fn mine_change(v: &mut LineEditorView, op: &str, req: &Value) -> (String, bool) {
    let d_idx = v.dir;
    let depot = v.line_depot();
    let Some(d) = v.line().and_then(|l| l.directions.get(d_idx)).cloned() else { return (String::new(), false) };
    let dest = d.destination();
    let ready = depot.is_some() && d.stops.len() >= 2 && !dest.is_empty();
    let (strings, new) = depot.as_ref().filter(|_| ready).map(|x| x.sign_of(&d)).unwrap_or_default();
    let defaults = match &depot {
        Some(x) if ready && new => x.sign_defaults(&dest),
        _ => Vec::new(),
    };
    let mut sign_vals = d.sign.clone();
    sign_vals.resize(defaults.len(), String::new());
    let (target, mine) = {
        let k = v.kept();
        (k.target.clone(), k.mine.clone())
    };
    let path = match target.as_ref() {
        Some((path, _)) => Ok(path.clone()),
        None => {
            let base = v.map_depot_bytes(&v.reg.global.clone());
            owndepot::ensure_for_map(&owndepot::dir(), &v.folder, &omsi_ui::tr("%{map} - mine").replace("%{map}", &v.folder), base.as_deref())
        }
    };
    let content = core::content_dir();
    let done = path.and_then(|p| {
        let file = owndepot::load(&p).map(|l| l.doc.hof.name).unwrap_or_default();
        match op {
            "keep_this" => {
                let (sign, code) = if new { (sign_vals.clone(), d.terminus_code) } else { (strings.clone(), depot.as_ref().and_then(|x| x.terminus(&dest)).map(|t| t.code).unwrap_or(0)) };
                owndepot::keep_destination(&p, &reg::OwnDestination { name: dest.clone(), sign, code }, content.as_deref()).map(|c| omsi_ui::tr("%{name} kept in your depot file %{file} (code %{code})").replace("%{name}", &dest).replace("%{code}", &c.to_string()).replace("%{file}", &file))
            }
            "keep_new" => {
                let name = s_of(req, "name").trim().to_string();
                v.dest_new.clear();
                owndepot::keep_destination(&p, &reg::OwnDestination { name: name.clone(), ..Default::default() }, content.as_deref()).map(|c| omsi_ui::tr("%{name} kept in your depot file %{file} (code %{code})").replace("%{name}", &name).replace("%{code}", &c.to_string()).replace("%{file}", &file))
            }
            _ => {
                let name = mine.get(u_of(req, "k")).map(|m| m.name.clone()).unwrap_or_default();
                owndepot::drop_destination(&p, &name, content.as_deref()).map(|_| omsi_ui::tr("%{name} taken out of your depot file %{file}").replace("%{name}", &name).replace("%{file}", &file))
            }
        }
    });
    v.own_changed();
    omsi_cfg::content_changed();
    match done {
        Ok(text) => (text, false),
        Err(e) => (e, true),
    }
}

// --- the Timetable tab ----------------------------------------------------------------------------

/// A change of the day patterns (`timetable_tab`): `what` of the group `k` (and band `i`) of the
/// direction shown - the table filled anew for a group whose departures it changed. With a size
/// of bus the company's level keeps locked, its reason said instead.
fn pattern_change(s: &mut Session, req: &Value) -> Option<(String, bool)> {
    let locked = super::company::locked(s);
    let v = &mut s.v;
    let what = s_of(req, "what");
    let (k, i) = (u_of(req, "k"), u_of(req, "i"));
    let two = v.line().is_some_and(|x| x.directions.iter().filter(|d| d.stops.len() >= 2).count() >= 2);
    let run_dir = v.dir.min(v.line().map(|x| x.directions.len().saturating_sub(1)).unwrap_or(0));
    let pdir = usize::from(two && run_dir == 1);
    let line = v.line_mut()?;
    let moves: Vec<usize> = if pdir == 1 || line.back_own() { vec![pdir] } else { vec![0, 1] };
    let before: Vec<Vec<f32>> = line.patterns_mut(pdir).iter().map(|p| p.departures().iter().map(|d| d.0).collect()).collect();
    let days = line.patterns_mut(pdir);
    let p = days.get_mut(k)?;
    let num = f_of(req, "value");
    match what.as_str() {
        "on" => p.on = b_of(req, "on"),
        "mode" => p.bands = if b_of(req, "bands") { reg::default_bands(k) } else { Vec::new() },
        "first" => p.first = num,
        "last" => p.last = num,
        "every" => p.headway = num,
        "layover" => p.layover = num,
        "band_from" => p.bands.get_mut(i)?.from = num,
        "band_to" => p.bands.get_mut(i)?.to = if num <= 0.0 { reg::DAY_END } else { num },
        "band_every" => p.bands.get_mut(i)?.headway = num,
        "band_size" => {
            let size: Option<BusSize> = req.get("size").filter(|x| !x.is_null()).and_then(|x| serde_json::from_value(x.clone()).ok());
            if let Some(sz) = size.filter(|sz| locked.contains(sz)) {
                return super::company::refusal(s, sz);
            }
            p.bands.get_mut(i)?.size = size;
        }
        "band_remove" => {
            if i < p.bands.len() {
                p.bands.remove(i);
            }
        }
        "band_add" => {
            let from = p.bands.iter().map(|b| b.to).fold(0.0, f32::max).min(reg::DAY_END - 60.0);
            p.bands.push(TimeBand { from, to: (from + 120.0).min(reg::DAY_END), headway: 30.0, size: None });
        }
        "typical" => {
            let company = v.company.is_some();
            let line = v.line_mut()?;
            line.days = typical_days(line.service, company);
            line.back_days.clear();
            v.touched();
            return None;
        }
        _ => return None,
    }
    let line = v.line_mut()?;
    if line.table_on {
        let now: Vec<Vec<f32>> = line.patterns_mut(pdir).iter().map(|p| p.departures().iter().map(|d| d.0).collect()).collect();
        let moved: Vec<usize> = (0..now.len()).filter(|&k| before.get(k) != now.get(k)).collect();
        for &k in &moved {
            line.refill_table(k, &moves);
        }
        if !moved.is_empty() {
            (v.table_sel, v.table_text) = (None, String::new());
        }
    }
    v.touched();
    None
}

// --- the Kind tab -----------------------------------------------------------------------------------

/// A change of the line's kind of service and its buses (`service_tab`).
fn kind_change(s: &mut Session, op: &str, req: &Value) -> Option<(String, bool)> {
    let locked = super::company::locked(s);
    let cat = view::catalogue_of(s);
    let v = &mut s.v;
    let for_company = v.company.is_some();
    match op {
        "kind" => {
            let k: ServiceKind = serde_json::from_value(req.get("kind")?.clone()).ok()?;
            let line = v.line_mut()?;
            // (a timetable the player left as it came follows the kind - the return's own too)
            if line.days == typical_days(line.service, for_company) {
                line.days = typical_days(k, for_company);
            }
            if line.back_days == typical_days(line.service, for_company) {
                line.back_days = typical_days(k, for_company);
            }
            line.service = k;
        }
        "typical" => {
            let line = v.line_mut()?;
            line.days = typical_days(line.service, for_company);
            line.back_days.clear();
        }
        "class" => {
            let c: VehicleClass = serde_json::from_value(req.get("class")?.clone()).ok()?;
            let on = v.line()?.vehicles.classes.contains(&c);
            // (a locked kind may still be taken off a line that asks for it)
            if locked.contains(&c.size()) && !on {
                return super::company::refusal(s, c.size());
            }
            v.line_mut()?.vehicles.toggle_class(c);
        }
        "take_suits" => {
            let line = v.line_mut()?;
            line.vehicles.classes = line.service.suits().iter().copied().filter(|k| !locked.contains(&k.size())).collect();
        }
        "bus_remove" => {
            let i = u_of(req, "i");
            let line = v.line_mut()?;
            if i < line.vehicles.buses.len() {
                line.vehicles.buses.remove(i);
            }
        }
        "pick" => {
            let (m, k, j) = (u_of(req, "m"), u_of(req, "k"), u_of(req, "j"));
            v.pick = if m != v.pick.0 { (m, 0, 0) } else if k != v.pick.1 { (m, k, 0) } else { (m, k, j) };
            return None;
        }
        "pick_add" => {
            let p = v.pick.0.checked_sub(1).and_then(|m| pick_of(&cat, m, v.pick.1, v.pick.2))?;
            let line = v.line_mut()?;
            if line.vehicles.buses.iter().any(|x| x.maker == p.maker && x.model == p.model && x.file == p.file) {
                return None;
            }
            line.vehicles.buses.push(p);
        }
        "holidays" => {
            let periods: Vec<service::HolidayPeriod> = serde_json::from_value(req.get("periods")?.clone()).ok()?;
            v.reg.school_holidays = periods;
        }
        _ => return None,
    }
    v.touched();
    None
}

// --- the table over the map ------------------------------------------------------------------------

/// The table's tools (`table_layer`'s `TableAct`).
fn table(s: &mut Session, op: &str, req: &Value) -> R {
    let v = &mut s.v;
    let Some(line) = v.line().cloned() else {
        v.table_open = false;
        return Ok(());
    };
    let kind = line.service;
    if !kind.allows(v.table_day) {
        v.table_day = (0..DAY_GROUPS.len()).find(|k| kind.allows(*k)).unwrap_or(0);
    }
    let (dir, day) = (v.dir.min(line.directions.len().saturating_sub(1)), v.table_day);
    let mut cols: Vec<usize> = (0..line.table.len()).filter(|&i| line.table[i].dir == dir && reg::trip_in_group(&line, &line.table[i], day)).collect();
    cols.sort_by(|a, b| line.table[*a].departure().total_cmp(&line.table[*b].departure()));
    let sel = v.table_sel.filter(|(i, _)| cols.contains(i));
    let stops = line.directions.get(dir).map(|d| d.stops.len()).unwrap_or(0);
    let headway = line.pattern(dir, day).map(|p| if p.bands.is_empty() { p.headway } else { p.clean_bands().first().map(|b| b.headway).unwrap_or(p.headway) }).unwrap_or(30.0).max(1.0);
    let mut changed = true;
    match op {
        "t_close" => {
            v.table_open = false;
            changed = false;
        }
        "t_dir" => {
            (v.dir, v.sel_stop, v.table_sel, v.table_col) = (u_of(req, "d"), None, None, 0.0);
            v.revision += 1;
            changed = false;
        }
        "t_day" => {
            (v.table_day, v.table_sel, v.table_col) = (u_of(req, "k"), None, 0.0);
            changed = false;
        }
        "t_select" => {
            v.table_sel = Some((u_of(req, "i"), u_of(req, "s")));
            v.table_text.clear();
            changed = false;
        }
        "t_use" => {
            let l2 = v.line_mut().unwrap();
            l2.table_on = b_of(req, "on");
            if l2.table_on && l2.table.is_empty() {
                l2.table = reg::table_from_patterns(l2);
            }
        }
        "t_fill" => {
            if v.table_fill_armed || line.table.is_empty() {
                let l2 = v.line_mut().unwrap();
                l2.table = reg::table_from_patterns(l2);
                (v.table_sel, v.table_col, v.table_fill_armed) = (None, 0.0, false);
            } else {
                v.table_fill_armed = true;
                changed = false;
            }
        }
        "t_add" => {
            let after = cols.last().map(|&i| line.table[i].clone());
            let rel = line.directions.get(dir).map(reg::fitted_times).unwrap_or_default();
            let first_dep = line.pattern(dir, day).map(|p| p.departures().first().map(|x| x.0).unwrap_or(p.first)).unwrap_or(360.0);
            let trip = match after {
                Some(mut t) => {
                    t.shift(headway);
                    (t.day, t.tour) = (day, String::new());
                    t
                }
                None => reg::TableTrip { dir, day, times: rel.iter().map(|x| first_dep + x).collect(), tour: String::new() },
            };
            let l2 = v.line_mut().unwrap();
            l2.table.push(trip);
            v.table_sel = Some((l2.table.len() - 1, 0));
        }
        "t_copy" => {
            if let Some((i, st)) = sel {
                let l2 = v.line_mut().unwrap();
                let mut t = l2.table[i].clone();
                t.shift(f_of(req, "m"));
                (t.day, t.tour) = (day, String::new());
                l2.table.push(t);
                v.table_sel = Some((l2.table.len() - 1, st));
            }
        }
        "t_shift" => {
            if let Some((i, _)) = sel {
                v.line_mut().unwrap().table[i].shift(f_of(req, "m"));
            }
        }
        "t_delete" => {
            if let Some((i, _)) = sel {
                v.line_mut().unwrap().table.remove(i);
                v.table_sel = None;
            }
        }
        "t_nudge" => {
            if let Some((i, st)) = sel {
                if let Some(t) = v.line_mut().unwrap().table[i].times.get_mut(st) {
                    *t = (*t + f_of(req, "m")).max(0.0);
                }
                v.table_text.clear();
            }
        }
        "t_typed" => {
            if let Some((i, st)) = sel {
                if let Some(t) = v.line_mut().unwrap().table[i].times.get_mut(st) {
                    // (a time after midnight in a trip of the evening is the next day's)
                    *t = typed_minutes(u_of(req, "m") as i32, *t);
                }
                v.table_text.clear();
                if b_of(req, "next") {
                    v.table_sel = Some((i, (st + 1).min(stops.saturating_sub(1))));
                }
            }
        }
        _ => changed = false,
    }
    if changed {
        v.table_fill_armed = false;
        v.touched();
    }
    Ok(())
}
