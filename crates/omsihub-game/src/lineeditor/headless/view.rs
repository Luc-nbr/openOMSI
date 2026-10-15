//! What the page shows of the line editor (`view`): the lines of the map, the line's settings,
//! each tab as its panel lays it out, the table over the map, the company's figures - worked
//! out as the game-drawn launcher's panels work them out, their texts in the page's language.

use super::*;
use crate::theme::{DANGER, OK, TEXT_DIM, TEXT_FAINT, TEXT_SOFT, WARN};
use omsi_ui::Color;

/// A colour of the Rust launcher's as the page's look names it.
fn tone(c: Color) -> &'static str {
    if c == WARN {
        "warn"
    } else if c == OK {
        "ok"
    } else if c == DANGER {
        "bad"
    } else if c == TEXT_FAINT {
        "faint"
    } else if c == TEXT_DIM {
        "dim"
    } else if c == TEXT_SOFT {
        "soft"
    } else {
        "text"
    }
}

pub(super) fn view(s: &mut Session) -> Value {
    // (a line whose depot group the map lacks - or none - is driven by the map's first, as the
    // window's left panel sets it: a change to be saved)
    let first = s.v.groups.first().map(|g| g.0.clone());
    let unknown = s.v.line().is_some_and(|l| !s.v.groups.iter().any(|g| g.0.eq_ignore_ascii_case(&l.ai_group)));
    if let (Some(first), true) = (first, unknown) {
        if let Some(line) = s.v.line_mut() {
            line.ai_group = first;
            s.v.touched();
        }
    }
    let status = s.status.take();
    let go = s.go.take().or_else(|| s.v.go_timeplan.take().map(|t| json!({ "timeplan": { "id": t.id, "name": t.name, "hint": t.hint.map(|h| omsi_ui::tr(h).into_owned()) } })));
    let go = go.or_else(|| s.v.go_depots.take().map(|k| json!({ "depots": k })));
    // (what reads the depot files first: their caches are the view's own)
    let stops = stops_tab(&mut s.v);
    let shown_displays = if s.v.tab == 1 { displays(&mut s.v) } else { Value::Null };
    let shown_timetable = if s.v.tab == 2 { timetable(s) } else { Value::Null };
    let shown_service = if s.v.tab == 3 { service_tab(s) } else { Value::Null };
    let shown_table = if s.v.table_open { table(s) } else { Value::Null };
    let company_view = super::company::view(s);
    let v = &s.v;
    let origin = s.map.as_ref().map(|m| m.origin).unwrap_or(DVec2::ZERO);
    let r1 = |x: f64| (x * 10.0).round() / 10.0;
    let rel = |p: DVec2| [r1(p.x - origin.x), r1(p.y - origin.y)];
    let shapes: Vec<Vec<Vec<f64>>> = v.shapes.iter().map(|legs| legs.iter().map(|pts| pts.iter().flat_map(|p| rel(*p)).collect()).collect()).collect();
    let line = v.line();
    let line_json = line.map(|l| {
        let mut j = serde_json::to_value(l).unwrap_or(Value::Null);
        // (the stops and dragged points in the page's metres too)
        if let Some(dirs) = j.get_mut("directions").and_then(Value::as_array_mut) {
            for (d, src) in dirs.iter_mut().zip(&l.directions) {
                d["stops_at"] = json!(src.stops.iter().map(|st| rel(dv(st.at))).collect::<Vec<_>>());
                d["vias_at"] = json!(src.legs.iter().map(|g| g.vias.iter().map(|p| rel(dv(*p))).collect::<Vec<_>>()).collect::<Vec<_>>());
            }
        }
        // (what the page does not need: the map's tracks, the lanes)
        if let Some(dirs) = j.get_mut("directions").and_then(Value::as_array_mut) {
            for d in dirs {
                d["map_track"] = json!([]);
                if let Some(legs) = d.get_mut("legs").and_then(Value::as_array_mut) {
                    for g in legs {
                        g["steps"] = json!([]);
                    }
                }
            }
        }
        if let Some(live) = j.get_mut("live") {
            *live = Value::Null;
        }
        j
    });
    // the stops of the map the line has, by their place in the map's list (the page's dots)
    let on_map = |st: &StopRef| v.stops.iter().position(|m| m.id == st.id && m.tile == st.tile);
    let line_stops: Vec<Vec<Option<usize>>> = line.map(|l| l.directions.iter().map(|d| d.stops.iter().map(on_map).collect()).collect()).unwrap_or_default();
    json!({
        "status": status.map(|(t, e)| json!([t, e])),
        "go": go,
        "maps": s.maps.iter().map(|m| m.0.clone()).collect::<Vec<_>>(),
        "map": v.map,
        "map_key": s.map.as_ref().map(|m| m.key.clone()),
        "reading": s.reading.is_some(),
        "ready": v.router.is_some(),
        "company": v.company.as_ref().map(|fc| json!({ "id": fc.id, "text": format!("{}  ·  {}", fc.name, s.maps.get(v.map).map(|m| m.0.clone()).unwrap_or_default()), "rosterflow": fc.rosterflow })),
        "rows": rows(s),
        "map_rows": maplines::shown(v).iter().map(|r| json!({ "ttl": r.ttl, "number": r.number, "label": r.label, "colour": r.colour, "edited": r.edited })).collect::<Vec<_>>(),
        "none": omsi_ui::tr(if v.company.is_some() { "The company has no line of its own yet" } else { "No lines of your own on this map yet" }),
        "sel": v.sel,
        "sel_ttl": line.and_then(|x| x.map.as_ref()).map(|m| m.ttl.clone()),
        "line": line_json,
        "line_stops": line_stops,
        "dirty": v.dirty,
        "fresh": v.fresh.is_some() && v.fresh == v.sel,
        "delete_armed": v.delete_armed,
        "restore_armed": v.restore_armed,
        "colour_open": v.colour_open,
        "palette": palette(v),
        "groups": v.groups.iter().map(|g| g.0.clone()).collect::<Vec<_>>(),
        "dir": v.dir,
        "sel_stop": v.sel_stop,
        "tab": v.tab,
        "white": v.white,
        "problems": v.problems().iter().map(|p| omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string())).collect::<Vec<_>>(),
        "map_problems": line.map(reg::problems).unwrap_or_default().iter().map(|p| omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string())).collect::<Vec<_>>(),
        "shapes": shapes,
        "stops_tab": stops,
        "displays": shown_displays,
        "timetable": shown_timetable,
        "service": shown_service,
        "table": shown_table,
        "company_view": company_view,
        "directions": directions(v),
    })
}

/// The player's lines the list shows (`left_panel`): its plate, its colour, a draft's mark, its
/// kind's icon.
fn rows(s: &Session) -> Vec<Value> {
    let v = &s.v;
    v.reg
        .lines
        .iter()
        .filter(|x| v.shows(x))
        .map(|x| json!({ "id": x.id, "number": x.number, "name": x.name, "colour": x.colour, "draft": x.draft && v.company.is_some(), "kind": if x.service != ServiceKind::Regular { Some(json!({ "icon": x.service.icon(), "label": omsi_ui::tr(x.service.label()) })) } else { None } }))
        .collect()
}

/// The colours a line may have: the company's two first, working for one.
fn palette(v: &LineEditorView) -> Vec<String> {
    match &v.company {
        Some(fc) => fc.colours.iter().cloned().chain(PALETTE.iter().take(6).map(|s| s.to_string())).collect(),
        None => PALETTE.iter().map(|s| s.to_string()).collect(),
    }
}

/// The directions as the choice shows them (`maplines::direction_choice`): two halves, or a
/// list of a line of the map's with more trips than two.
fn directions(v: &LineEditorView) -> Value {
    let Some(line) = v.line() else { return json!({ "list": false, "labels": [] }) };
    if line.directions.len() <= 2 {
        return json!({ "list": false, "labels": [omsi_ui::tr("Outbound"), omsi_ui::tr("Return")] });
    }
    let labels: Vec<String> = line
        .directions
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let what = match k {
                0 => omsi_ui::tr("Outbound").into_owned(),
                1 => omsi_ui::tr("Return").into_owned(),
                _ => omsi_ui::tr("Variant %{n}").replace("%{n}", &(k + 1).to_string()),
            };
            let from = d.stops.first().map(|s| s.name.trim().to_string()).unwrap_or_default();
            let shown = if d.number.trim().is_empty() { String::new() } else { format!("{} · ", d.number.trim()) };
            format!("{what}: {shown}{from} – {}", d.destination())
        })
        .collect();
    json!({ "list": true, "labels": labels })
}

/// The Stops tab (`right_panel`): the direction's stops with the minutes to each and the legs
/// between them, the whole trip's minutes, whether it is a loop, and the destination's field.
fn stops_tab(v: &mut LineEditorView) -> Value {
    let dest = dest_field(v, v.dir);
    let Some(line) = v.line() else { return Value::Null };
    let two = line.directions.len() > 1;
    // (how the way back is made: a line of the map's keeps the map's)
    let back = (!line.is_map_line() && v.dir == 1).then(|| back_view(v, line));
    let Some(d) = line.directions.get(v.dir) else { return json!({ "two": two, "none": true, "back": back }) };
    let legs: Vec<Value> = d
        .legs
        .iter()
        .map(|g| {
            let text = match (g.ok, g.map_way.is_some()) {
                (true, true) => omsi_ui::tr("%{n} m · as the map drives it").replace("%{n}", &format!("{:.0}", g.length)),
                (true, false) => format!("{:.0} m", g.length),
                _ => omsi_ui::tr("no way over the roads").to_string(),
            };
            json!({ "ok": g.ok, "text": text, "vias": g.vias.len() })
        })
        .collect();
    json!({
        "two": two,
        "rows": d.stops.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "times": d.times.iter().map(|t| format!("{t:.0}'")).collect::<Vec<_>>(),
        "legs": legs,
        "total": omsi_ui::tr("%{n} min from the first stop to the last").replace("%{n}", &format!("{:.0}", d.minutes())),
        "is_loop": d.is_loop(),
        "dest": dest,
        "back": back,
    })
}

/// The way back's choice on the Stops tab (`back_mode`): whether the line editor makes it
/// (`auto`), "Automatic" pressed once (`armed`) - and, while its stops are the ones made from
/// the outbound, each one's mark (a stop of a name like the outbound's across the road, or none
/// there: the outbound's own) and what the tab says of them and of its legs without a way.
pub(super) fn back_view(v: &LineEditorView, line: &LineDesign) -> Value {
    use reg::Across;
    let tr = |s: &str| omsi_ui::tr(s).into_owned();
    let auto = line.back_route == BackRoute::Auto;
    let mut j = json!({ "auto": auto, "armed": v.back_armed, "marks": [], "notes": [] });
    let (Some(out), Some(back)) = (line.directions.first(), line.directions.get(1)) else { return j };
    let note = |tone: &str, text: String| json!({ "tone": tone, "text": text });
    if auto && out.stops.len() < 2 {
        j["notes"] = json!([note("soft", tr("Give the outbound its stops: the way back is made from them"))]);
        return j;
    }
    let made = reg::across_the_road(&out.stops, &v.stops, ACROSS);
    if v.stops.is_empty() || !same_stops(&back.stops, &made.iter().map(|m| m.0.clone()).collect::<Vec<_>>()) {
        return j;
    }
    let from: Vec<&StopRef> = out.stops.iter().rev().collect();
    j["marks"] = made
        .iter()
        .zip(&from)
        .map(|((_, k), o)| match k {
            Across::Named => Value::Null,
            Across::Alike => json!({ "kind": "alike", "text": tr("Across the road from %{name}: a name like it").replace("%{name}", o.name.trim()) }),
            Across::Missing => json!({ "kind": "missing", "text": tr("No stop across the road: the bus calls at this one both ways") }),
        })
        .collect();
    let missing: Vec<&str> = made.iter().filter(|m| m.1 == Across::Missing).map(|m| m.0.name.trim()).collect();
    let alike = made.iter().filter(|m| m.1 == Across::Alike).count();
    let bad = back.legs.iter().filter(|g| !g.ok).count();
    let mut notes = Vec::new();
    if !missing.is_empty() {
        notes.push(note("warn", tr("No stop across the road for %{n} stop(s): the bus calls at the outbound's own (%{names})").replace("%{n}", &missing.len().to_string()).replace("%{names}", &missing.join(", "))));
    }
    if bad > 0 {
        notes.push(note("warn", tr("%{n} leg(s) have no way over the roads (red): drag them, or set the way back by hand").replace("%{n}", &bad.to_string())));
    }
    if alike > 0 {
        notes.push(note("soft", tr("%{n} stop(s) taken by a name like the outbound's (marked): check them on the map").replace("%{n}", &alike.to_string())));
    }
    if notes.is_empty() && v.router.is_some() {
        notes.push(note("ok", tr("Every stop has its stop across the road, and every leg a way over the roads")));
    }
    j["notes"] = json!(notes);
    j
}

/// The destination's field (`destination_field`): what is typed, the placeholder - the last stop
/// or the loop's word - and the list of what the buses know, the one chosen in it.
fn dest_field(v: &mut LineEditorView, d_idx: usize) -> Value {
    if v.line().and_then(|l| l.directions.get(d_idx)).is_none() {
        return Value::Null;
    }
    let termini = v.termini();
    let mine = v.mine();
    let Some(d) = v.line().and_then(|l| l.directions.get(d_idx)) else { return Value::Null };
    let last = if d.is_loop() { omsi_ui::tr("Loop: %{name}").replace("%{name}", reg::RING) } else { omsi_ui::tr("Last stop: %{name}").replace("%{name}", d.stops.last().map(|s| s.name.trim()).unwrap_or("")) };
    let mut options = vec![last.clone()];
    options.extend(termini.iter().map(|t| t.1.clone()));
    options.extend(mine.iter().map(|m| omsi_ui::tr("Mine: %{name}").replace("%{name}", &m.name)));
    let now = d.terminus.trim();
    let k = if now.is_empty() {
        0
    } else {
        termini.iter().position(|t| t.0.eq_ignore_ascii_case(now)).map(|i| i + 1).or_else(|| mine.iter().position(|m| m.name.trim().eq_ignore_ascii_case(now)).map(|i| i + 1 + termini.len())).unwrap_or(options.len())
    };
    json!({ "typed": d.terminus, "last": last, "options": options, "k": k, "tip": omsi_ui::tr("Choose one the buses know") })
}

/// The depot file's choice (`depot_choice`): the map's, the player's own, a new one.
fn depot_options(v: &mut LineEditorView) -> Value {
    let Some(l) = v.line() else { return Value::Null };
    let (group, chosen) = (l.ai_group.clone(), l.depot_file.trim().to_string());
    let hof = v.hof_of(&group);
    let own: Vec<(String, String)> = v.own_entries().iter().map(|e| (e.key.clone(), e.name.clone())).collect();
    let mut options = vec![if hof.is_empty() { omsi_ui::tr("The map's (it names none)").into_owned() } else { omsi_ui::tr("The map's: %{hof}").replace("%{hof}", &hof) }];
    options.extend(own.iter().map(|(_, name)| omsi_ui::tr("Mine: %{name}").replace("%{name}", name)));
    options.push(omsi_ui::tr("A new one of my own, from the map's").into_owned());
    let at = own.iter().position(|(key, _)| key.eq_ignore_ascii_case(&chosen));
    json!({ "options": options, "k": at.map(|i| i + 1).unwrap_or(0), "open": at.is_some(), "hint": omsi_ui::tr(depot_hint(&chosen)) })
}

/// The Displays tab (`displays_simple`, `displays_advanced`).
fn displays(v: &mut LineEditorView) -> Value {
    let d_idx = v.dir;
    let Some(l) = v.line() else { return Value::Null };
    let two = l.directions.len() > 1;
    let (group, number) = (l.ai_group.clone(), l.number.trim().to_string());
    if d_idx == 1 && !two {
        return json!({ "advanced": v.sign_advanced, "round": true });
    }
    let Some(d) = l.directions.get(d_idx).cloned() else { return Value::Null };
    let dest = d.destination();
    let depot = v.line_depot();
    let no_depot = depot.is_none().then(|| no_depot_text(v, &group));
    let ready = depot.is_some() && d.stops.len() >= 2 && !dest.is_empty();
    let dest_json = dest_field(v, d_idx);
    let depot_json = depot_options(v);
    let mut out = json!({ "advanced": v.sign_advanced, "dest": dest_json, "no_depot": no_depot, "ready": ready, "depot": depot_json, "number": number });
    let (strings, new) = depot.as_ref().filter(|_| ready).map(|x| x.sign_of(&d)).unwrap_or_default();
    if !v.sign_advanced {
        if let Some(depot) = depot.as_ref().filter(|_| ready) {
            let (l1, l2) = depot.front(&strings);
            let (side, ibis) = depot.shown(&strings);
            let or_dash = |s: String| if s.is_empty() { "-".to_string() } else { s };
            let caption = omsi_ui::tr("Side: %{side} · IBIS: %{ibis}").replace("%{side}", &or_dash(side)).replace("%{ibis}", &or_dash(ibis));
            let hint = if !new {
                omsi_ui::tr("%{name} is in the depot file: the buses show its own texts.").replace("%{name}", &dest)
            } else if d.sign.iter().any(|x| !x.trim().is_empty()) {
                omsi_ui::tr("Some texts are your own (Advanced); the others are made from this destination.").into_owned()
            } else {
                omsi_ui::tr("Every display of the bus is filled from this destination. Advanced: each text on its own, the IBIS route and the stops' names.").into_owned()
            };
            let similar = if new { depot.similar(&dest).map(|t| t.texture_id.trim().to_string()) } else { None };
            out["sign"] = json!({ "l1": l1, "l2": l2, "caption": caption, "hint": hint, "known": similar.as_ref().map(|x| omsi_ui::tr("The depot file knows it as %{name}").replace("%{name}", x)), "shortened": shortened_on_front(depot, &d) });
        }
        return out;
    }
    // Advanced: each text of the depot file on its own
    let defaults = match &depot {
        Some(x) if ready && new => x.sign_defaults(&dest),
        _ => Vec::new(),
    };
    let mut sign_vals = d.sign.clone();
    sign_vals.resize(defaults.len(), String::new());
    let (mut main, mut other) = (Vec::new(), Vec::new());
    if let Some(x) = depot.as_ref().filter(|_| ready) {
        let firsts: Vec<usize> = [linehof::Role::IbisDisplay, linehof::Role::FrontTop, linehof::Role::FrontBottom, linehof::Role::Side, linehof::Role::ClearName].iter().filter_map(|role| x.roles.iter().position(|r| r == role)).collect();
        let count = if new { defaults.len() } else { strings.len() };
        for k in 0..count {
            let role = x.roles.get(k).copied().unwrap_or(linehof::Role::Other);
            let note = x.notes.get(k).map(|n| n.trim()).unwrap_or("");
            let (title, help) = text_words(x, k);
            let shown = if new { defaults[k].clone() } else { strings.get(k).cloned().unwrap_or_default() };
            let text = matches!(x.cols.get(k), Some(linehof::Column::Text { .. }));
            let max = match x.cols.get(k) {
                Some(linehof::Column::Text { max, .. }) => Some(*max),
                Some(linehof::Column::File(_)) => None,
                _ => owndepot::limit(role, note),
            };
            let note = if note.is_empty() { String::new() } else { omsi_ui::tr("Depot file: “%{note}”").replace("%{note}", &note.chars().take(80).collect::<String>()) };
            let filled = !shown.trim().is_empty() || sign_vals.get(k).is_some_and(|s| !s.is_empty());
            let t = json!({ "k": k, "title": title, "help": help, "note": note, "max": max, "shown": shown, "value": if new { sign_vals.get(k).cloned() } else { None } });
            if text && (firsts.contains(&k) || filled) {
                main.push(t);
            } else if new || filled {
                other.push(t);
            }
        }
    }
    let intro = if new { omsi_ui::tr("Each field is one text of the depot file; the bus reads each one for one display. Empty: the grey text made from the destination. Another destination makes them again.").into_owned() } else { omsi_ui::tr("These are the depot file's own texts for %{name}: change them in a depot file of your own (depot editor).").replace("%{name}", &dest) };
    let line_no = linehof::line_number(&number);
    let route = if d.ibis_route > 0 { d.ibis_route.to_string() } else { String::new() };
    let taken: std::collections::HashSet<u32> = depot.as_ref().map(|x| x.hof.info_trips.iter().filter_map(|t| t.code.trim().parse().ok()).collect()).unwrap_or_default();
    let code: u32 = route.trim().parse().unwrap_or(0);
    let (hint, colour) = if route.trim().is_empty() {
        (omsi_ui::tr("Given when the line is saved").to_string(), TEXT_DIM)
    } else if code / 100 != line_no || code % 100 == 0 {
        (omsi_ui::tr("Not a route of line %{n} (%{n}01 - %{n}99): another is given on saving").replace("%{n}", &line_no.to_string()), WARN)
    } else if taken.contains(&code) {
        (omsi_ui::tr("The depot file has this code already: another is given on saving").to_string(), WARN)
    } else {
        (omsi_ui::tr("Line %{l}, route %{r} on the IBIS").replace("%{l}", &line_no.to_string()).replace("%{r}", &format!("{:02}", code % 100)), TEXT_DIM)
    };
    let stops: Vec<Value> = d.stops.iter().map(|st| json!({ "name": st.name, "def": depot.as_ref().map(|x| x.stop_display(&st.name)).unwrap_or_default(), "value": st.ibis })).collect();
    let (target, mine, is_mine) = {
        let k = v.kept();
        (k.target.clone(), k.mine.clone(), k.names.contains(&dest.trim().to_lowercase()))
    };
    let where_kept = match target.as_ref() {
        Some((_, name)) => omsi_ui::tr("In your depot file %{name} (Editor, Depot editor); chosen under Stops, Destination.").replace("%{name}", name),
        None => omsi_ui::tr("Kept in a depot file of your own for this map, made from the map's when you keep the first; chosen under Stops, Destination.").into_owned(),
    };
    out["adv"] = json!({
        "intro": intro, "main": main, "more": other.len(), "other": other,
        "more_label": if v.more_texts { omsi_ui::tr("Hide the texts for other kinds of sign").into_owned() } else { omsi_ui::tr("%{n} more texts, for other kinds of sign").replace("%{n}", &other.len().to_string()) },
        "more_open": v.more_texts,
        "can_reset": new && !d.sign.iter().all(|x| x.is_empty()),
        "route": route, "route_hint": (line_no * 100 + d_idx as u32 + 1).to_string(), "route_note": hint, "route_tone": tone(colour),
        "stops": stops, "stops_open": v.ibis_stops_open,
        "stops_label": if v.ibis_stops_open { omsi_ui::tr("Hide the stops").into_owned() } else { omsi_ui::tr("Show the %{n} stops").replace("%{n}", &d.stops.len().to_string()) },
        "mine": mine.iter().map(|m| json!({ "name": m.name, "code": m.code })).collect::<Vec<_>>(),
        "is_mine": is_mine, "where_kept": where_kept, "dest_new": v.dest_new,
    });
    out
}

/// The Timetable tab (`timetable_tab`, `timeplan_note`).
fn timetable(s: &Session) -> Value {
    let v = &s.v;
    let Some(line) = v.line() else { return Value::Null };
    if v.route_only() {
        let runs = !line.draft || line.is_map_line();
        let none_yet = !reg::has_departures(line);
        return json!({ "timeplan": { "runs": runs, "none_yet": none_yet, "text": omsi_ui::tr(if runs { TIMETABLE_THERE } else { TIMETABLE_LATER }) } });
    }
    let run: Vec<f32> = reg::run_minutes(line);
    let for_company = v.company.is_some();
    let kind = line.service;
    let of_map = line.is_map_line() && line.table.iter().any(|t| !t.tour.is_empty());
    let two = line.directions.iter().filter(|d| d.stops.len() >= 2).count() >= 2;
    let run_dir = v.dir.min(line.directions.len().saturating_sub(1));
    let pdir = usize::from(two && run_dir == 1);
    let total = line.directions.get(run_dir).filter(|d| d.stops.len() >= 2).map(|d| reg::fitted_times(d).last().copied().unwrap_or(1.0).max(1.0));
    let back_own = line.back_own();
    let other: Option<Vec<DayPattern>> = (two && back_own).then(|| (0..DAY_GROUPS.len()).map(|k| line.pattern(1 - pdir, k).cloned().unwrap_or_default()).collect());
    let mut pats = line.clone();
    let days = pats.patterns_mut(pdir).clone();
    let groups: Vec<Value> = DAY_GROUPS
        .iter()
        .enumerate()
        .map(|(k, (name, _))| {
            let p = days.get(k).cloned().unwrap_or_default();
            if !kind.allows(k) {
                return json!({ "name": omsi_ui::tr(name), "allowed": false, "why": omsi_ui::tr("%{kind} does not run then").replace("%{kind}", &omsi_ui::tr(kind.label())) });
            }
            let counts = if p.on {
                let (buses, tours, trips) = if run.is_empty() {
                    (0, 0, 0)
                } else {
                    let pats: Vec<DayPattern> = match other.as_ref().and_then(|o| o.get(k)) {
                        Some(o) if pdir == 0 => vec![p.clone(), o.clone()],
                        Some(o) => vec![o.clone(), p.clone()],
                        None => vec![p.clone()],
                    };
                    let t = reg::tours_each(&run, &pats, false);
                    (reg::blocks_each(&run, &pats, false).len(), t.len(), t.iter().map(|b| b.len()).sum::<usize>())
                };
                let text = if tours > buses { omsi_ui::tr("%{b} buses in %{n} tours (some for the rush hours only), %{t} trips").replace("%{n}", &tours.to_string()) } else { omsi_ui::tr("%{b} buses, %{t} trips").into_owned() };
                text.replace("%{b}", &buses.to_string()).replace("%{t}", &trips.to_string())
            } else {
                String::new()
            };
            json!({ "name": omsi_ui::tr(name), "allowed": true, "p": p, "counts": counts })
        })
        .collect();
    let locked = super::company::locked(s);
    let sizes: Vec<Value> = SIZES
        .iter()
        .map(|sz| {
            let label = match sz {
                Some(x) if locked.contains(x) => format!("{}  ·  {}", size_label(*sz), omsi_ui::tr("locked")),
                _ => size_label(*sz),
            };
            json!([sz, label])
        })
        .collect();
    json!({
        "table_on": line.table_on, "of_map": of_map, "two": two, "pdir": pdir, "run_dir": run_dir,
        "total": total, "back_own": back_own, "groups": groups, "for_company": for_company, "sizes": sizes,
        "day_end": reg::DAY_END,
    })
}

/// The installed buses by maker and model (`catalogue`), read once for each set of them.
pub(super) fn catalogue_of(s: &mut Session) -> Arc<Catalogue> {
    if s.vehicles.is_none() {
        s.vehicles = Some(Arc::new(core::list_vehicles().unwrap_or_default()));
    }
    let vehicles = s.vehicles.clone().unwrap_or_default();
    let set = crate::busclass::fingerprint(&vehicles);
    if s.v.catalogue.as_ref().map(|c| c.0) != Some(set) {
        s.v.catalogue = Some((set, Arc::new(catalogue(&vehicles))));
    }
    s.classes.want(&vehicles);
    s.v.catalogue.as_ref().map(|c| c.1.clone()).unwrap_or_default()
}

/// The Kind tab (`service_tab`).
fn service_tab(s: &mut Session) -> Value {
    let cat = catalogue_of(s);
    let locked = super::company::locked(s);
    let vehicles = s.vehicles.clone().unwrap_or_default();
    let fleet: Option<Vec<(String, String, BusSize)>> = s.company.as_ref().filter(|_| s.v.company.is_some()).map(|c| c.fleet.iter().map(|x| (x.bus.clone(), x.name.clone(), x.kind.size)).collect());
    let date = match (s.v.company.is_some(), s.company.as_ref()) {
        (true, Some(c)) => c.date.clone(),
        _ => s.date.clone(),
    };
    let v = &s.v;
    let Some(line) = v.line() else { return Value::Null };
    let kind = line.service;
    let want = line.vehicles.clone();
    let matching = s.classes.matching(&vehicles, &want).len();
    let in_fleet = fleet.as_ref().map(|fl| fl.iter().filter(|(file, name, size)| want.allows(&service::BusFacts::of_fleet(file, name, *size))).count());
    let cal = service::DayCalendar::new(&v.map_calendar, &v.reg.school_holidays);
    let periods = if v.reg.school_holidays.is_empty() { service::default_school_holidays() } else { v.reg.school_holidays.clone() };
    let school = (kind == ServiceKind::School).then(|| {
        let next = omsi_map::date_code(&date).and_then(|c| cal.next_school_holidays(c)).map(|(name, from, to)| {
            let name = if name.trim().is_empty() { "–".to_string() } else { omsi_ui::tr(name.trim()).into_owned() };
            omsi_ui::tr("Next: %{name}, %{from} to %{to}").replace("%{name}", &name).replace("%{from}", &day_of_code(from)).replace("%{to}", &day_of_code(to))
        });
        json!({ "map_school": cal.map_school(), "periods": periods.iter().map(|p| json!({ "name": p.name, "from": p.from, "to": p.to, "valid": p.valid() })).collect::<Vec<_>>(), "next": next })
    });
    let classes: Vec<Value> = VehicleClass::ALL.iter().map(|c| json!({ "id": c, "label": omsi_ui::tr(c.label()), "on": want.classes.contains(c), "shut": locked.contains(&c.size()) })).collect();
    let suits = (!kind.suits().is_empty() && want.classes != kind.suits()).then(|| {
        let names: Vec<String> = kind.suits().iter().map(|c| omsi_ui::tr(c.label()).into_owned()).collect();
        omsi_ui::tr("Suits this kind: %{buses}").replace("%{buses}", &names.join(", "))
    });
    let pick = v.pick;
    let makers: Vec<String> = std::iter::once(omsi_ui::tr("Choose a brand").into_owned()).chain(cat.iter().map(|m| m.0.clone())).collect();
    let models = pick.0.checked_sub(1).and_then(|k| cat.get(k)).map(|(_, models)| std::iter::once(omsi_ui::tr("All its models").into_owned()).chain(models.iter().map(|x| x.0.clone())).collect::<Vec<_>>());
    let versions = pick.0.checked_sub(1).and_then(|k| cat.get(k)).and_then(|(_, models)| pick.1.checked_sub(1).and_then(|k| models.get(k))).map(|(_, versions)| std::iter::once(omsi_ui::tr("All its versions").into_owned()).chain(versions.iter().map(|x| x.1.clone())).collect::<Vec<_>>());
    let (fit, fit_warn) = if want.open() {
        (omsi_ui::tr("Any bus of its depot group drives it. Choose kinds of bus or buses by name to have the line driven with them.").into_owned(), false)
    } else if s.classes.busy() {
        (omsi_ui::tr("Reading the buses…").into_owned(), false)
    } else {
        (omsi_ui::tr("%{n} of the installed buses fit. The timetable's buses on the line are such buses where the map's depots have them.").replace("%{n}", &matching.to_string()), matching == 0)
    };
    let fleet_text = match (in_fleet, want.open()) {
        (Some(0), false) => Some((omsi_ui::tr("None of the company's buses fits: the planning cannot put one on its tours.").into_owned(), true)),
        (Some(n), false) => Some((omsi_ui::tr("%{n} of the company's buses fit.").replace("%{n}", &n.to_string()), false)),
        _ => None,
    };
    json!({
        "title": line.title,
        "kinds": ServiceKind::ALL.iter().map(|k| json!({ "id": k, "icon": k.icon(), "label": omsi_ui::tr(k.label()), "on": *k == kind })).collect::<Vec<_>>(),
        "note": omsi_ui::tr(kind.note()),
        "route_only": v.route_only(),
        "school": school,
        "classes": classes,
        "suits": suits,
        "buses": want.buses.iter().map(|p| p.label.clone()).collect::<Vec<_>>(),
        "makers": makers, "models": models, "versions": versions, "pick": [pick.0, pick.1, pick.2],
        "fit": fit, "fit_warn": fit_warn, "fleet": fleet_text,
    })
}

/// The table over the map (`table_layer`).
fn table(s: &Session) -> Value {
    let v = &s.v;
    let Some(line) = v.line() else { return Value::Null };
    let kind = line.service;
    let day = if kind.allows(v.table_day) { v.table_day } else { (0..DAY_GROUPS.len()).find(|k| kind.allows(*k)).unwrap_or(0) };
    let dir = v.dir.min(line.directions.len().saturating_sub(1));
    let mut cols: Vec<usize> = (0..line.table.len()).filter(|&i| line.table[i].dir == dir && reg::trip_in_group(line, &line.table[i], day)).collect();
    cols.sort_by(|a, b| line.table[*a].departure().total_cmp(&line.table[*b].departure()));
    let sel = v.table_sel.filter(|(i, _)| cols.contains(i));
    let stops: Vec<String> = line.directions.get(dir).map(|d| d.stops.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
    let trips: Vec<Value> = cols
        .iter()
        .map(|&i| {
            let t = &line.table[i];
            let times: Vec<Value> = t.times.iter().enumerate().map(|(s, time)| json!([hm(*time), s > 0 && t.times.get(s - 1).is_some_and(|p| *time < *p - 1e-3)])).collect();
            json!({ "i": i, "times": times })
        })
        .collect();
    let falls: Vec<(usize, usize)> = cols.iter().enumerate().filter_map(|(k, &i)| line.table[i].first_fall().map(|s| (k, s))).collect();
    let (note, colour) = if let Some(&(k, st)) = falls.first() {
        let stop = stops.get(st).cloned().unwrap_or_default();
        (omsi_ui::tr("Trip %{n} reaches %{stop} before it left the stop before: the line is not saved so.").replace("%{n}", &(k + 1).to_string()).replace("%{stop}", &stop), WARN)
    } else if !line.table_on {
        (omsi_ui::tr("The day patterns make the line's timetable: switch the table on to make it this one.").into_owned(), TEXT_DIM)
    } else {
        let tours = reg::table_tours(line).into_iter().filter(|t| t.day == day).count();
        (omsi_ui::tr("%{t} trips on these days, driven in %{n} tours").replace("%{t}", &line.table.iter().filter(|t| reg::trip_in_group(line, t, day)).count().to_string()).replace("%{n}", &tours.to_string()), TEXT_SOFT)
    };
    let away = line.table.iter().filter(|t| !kind.allows(t.day)).count();
    json!({
        "title": omsi_ui::tr("Timetable of line %{n}").replace("%{n}", line.number.trim()),
        "dir": dir, "day": day,
        "days": DAY_GROUPS.iter().map(|g| omsi_ui::tr(g.0)).collect::<Vec<_>>(),
        "off": (0..DAY_GROUPS.len()).filter(|k| !kind.allows(*k)).collect::<Vec<_>>(),
        "use": line.table_on,
        "kind": { "icon": kind.icon(), "label": omsi_ui::tr(kind.label()), "note": omsi_ui::tr(kind.note()) },
        "fill_armed": v.table_fill_armed,
        "sel": sel.map(|(i, s)| [i, s]),
        "stops": stops, "trips": trips, "empty": line.table.is_empty(),
        "note": note, "note_tone": tone(colour),
        "away": (away > 0).then(|| omsi_ui::tr("%{n} trips on days %{kind} does not run: kept, not driven.").replace("%{n}", &away.to_string()).replace("%{kind}", &omsi_ui::tr(kind.label()))),
    })
}
