//! The line editor working for the bus company (`ForCompany`), without its window: its start
//! (a new line, or the one asked for), the figures of the line shown (`company_figures`), the
//! estimate over the map and the left panel's foot (`estimate_card`, `company_foot`), and what
//! its buttons ask for (`company_action`, `withdraw`, `change_dialog`). The company is read
//! from its file when the page opens the editor for it, and written back after each act.

use super::*;
use omsi_launcher_lib::company::ownline as own;

/// The company's start once its map is read (`company_start`): a new line, or the line asked
/// for; a line of the map's it runs opened once the map's stops are read.
pub(super) fn start(s: &mut Session) {
    let v = &mut s.v;
    if v.loaded.is_none() {
        return;
    }
    if let Some(name) = v.company.as_mut().and_then(|f| f.start_map.take()) {
        v.opening = Some(name);
        return;
    }
    let Some(start) = v.company.as_mut().and_then(|f| f.start.take()) else { return };
    match start {
        Some(id) if v.reg.line(id).is_some() => {
            (v.sel, v.dir, v.sel_stop) = (Some(id), 0, None);
            v.table_reset();
        }
        Some(_) => {}
        None => {
            new_line(v);
            s.say(omsi_ui::tr("Click the line's stops on the map; the estimate shows what it costs and brings.").into_owned(), false);
        }
    }
}

/// The lines of the company's map in the timetable of its day (`company::map_lines`), read once
/// a day and map.
fn day_lines(s: &mut Session) -> Option<Arc<Vec<core::LineInfo>>> {
    let c = s.company.as_ref()?;
    let key = format!("{}|{}", c.map, c.date);
    if s.day_lines.as_ref().is_none_or(|d| d.0 != key) {
        let lines = core::list_lines(&c.map, &c.date).unwrap_or_default();
        s.day_lines = Some((key, Arc::new(lines)));
    }
    s.day_lines.as_ref().map(|d| d.1.clone())
}

/// The figures of the line shown (`company_figures`), made again when the line or the
/// company's money changed.
pub(super) fn figures(s: &mut Session) {
    if s.v.company.is_none() || s.company.is_none() {
        return;
    }
    let lines = day_lines(s);
    let (Some(fc), Some(c)) = (s.v.company.as_mut(), s.company.as_ref()) else { return };
    let v_sel = s.v.sel;
    let Some(line) = v_sel.and_then(|id| s.v.reg.lines.iter().find(|x| x.id == id)).filter(|x| !x.is_map_line()) else {
        fc.cache = None;
        return;
    };
    if fc.stop_lines.as_ref().map(|x| x.0) != Some(line.id) {
        if let Some(lines) = lines.as_ref() {
            let stem = reg::stems(&s.v.reg).get(&line.id).cloned().unwrap_or_default();
            fc.stop_lines = Some((line.id, own::StopLines::of(lines, &stem)));
        }
    }
    let known = fc.stop_lines.as_ref().is_some_and(|x| x.0 == line.id);
    let key = (line.id, s.v.revision * 2 + known as u64, c.cash, c.lines.len());
    if fc.cache.as_ref().is_some_and(|f| f.key == key) {
        return;
    }
    let sl = fc.stop_lines.as_ref().filter(|x| x.0 == line.id).map(|x| &x.1);
    let shape = own::shape_of(line, &|st| sl.map(|x| x.count(st)).unwrap_or(0));
    let priced = own::route_only(c, line);
    let est = own::estimate(c, &own::as_priced(c, line), &shape);
    let plan = own::line_of(c, line.id).and_then(|x| x.plan.as_ref());
    let (confirmed, paid) = (plan.is_some(), plan.map(|p| p.paid).unwrap_or(0));
    let fee = if confirmed { own::change_fee(c, line, &shape) } else { None };
    fc.cache = Some(Figures { key, shape, est, confirmed, fee, cash: c.cash, paid, priced });
}

/// The sizes of bus the company's level does not open yet (`company::sizes_locked`).
pub(super) fn locked(s: &Session) -> Vec<BusSize> {
    let Some(c) = s.company.as_ref().filter(|_| s.v.company.is_some()) else { return Vec::new() };
    [BusSize::Articulated, BusSize::Double].into_iter().filter(|x| co::market::size_allowed(c, *x).is_err()).collect()
}

/// A size the level keeps locked asked for: why not (`company::size_locked`'s popup).
pub(super) fn refusal(s: &mut Session, size: BusSize) -> Option<(String, bool)> {
    let reason = s.company.as_ref().and_then(|c| co::market::size_allowed(c, size).err())?;
    Some((omsi_ui::tr(reason).into_owned(), true))
}

/// What working for the company shows: the estimate over the map, the left panel's foot,
/// the question before a change of a line in service, and what saving a line of the map's
/// costs.
pub(super) fn view(s: &Session) -> Value {
    let v = &s.v;
    let (Some(_), Some(c)) = (v.company.as_ref(), s.company.as_ref()) else { return Value::Null };
    let eur = crate::words::eur;
    let fee_map = v.line().filter(|x| x.is_map_line()).map(|line| {
        let old = v.running.get(&line.id).unwrap_or(line);
        (own::map_change_fee(c, old, line).unwrap_or(0), c.cash)
    });
    let figures = v.company.as_ref().and_then(|f| f.cache.clone()).filter(|f| Some(f.key.0) == v.sel);
    let mut out = json!({ "cash": eur(c.cash) });
    if let Some((fee, cash)) = fee_map {
        out["map_fee"] = json!({ "fee": fee, "label": eur(fee), "short": fee > cash, "why": (fee > cash).then(|| omsi_ui::tr("The company has %{cash}: not enough for the change.").replace("%{cash}", &eur(cash))) });
    }
    if let Some(f) = figures.as_ref() {
        out["estimate"] = estimate(s, f);
        out["foot"] = foot(s, f);
        out["live_label"] = json!(omsi_ui::tr("Live displays at %{n} transfer stops").replace("%{n}", &f.shape.transfer_stops().to_string()));
    } else if v.line().is_some_and(|x| !x.is_map_line()) {
        out["foot"] = json!({ "reading": omsi_ui::tr("Reading the company…") });
    }
    if let Some(ask) = v.change_ask.as_ref() {
        let number = v.line().map(|x| x.number.trim().to_string()).unwrap_or_default();
        let d = &ask.diff;
        let replan = d.replan().len() + d.added.len();
        let days = co::dates::between(&ask.today, &ask.from);
        let when = if days == 1 { format!("{}  ({})", crate::words::day_label(&ask.from), omsi_ui::tr("tomorrow")) } else { crate::words::day_label(&ask.from) };
        out["change"] = json!({
            "title": omsi_ui::tr("Change line %{n} in service").replace("%{n}", &number),
            "fee": if ask.fee > 0 { eur(ask.fee) } else { omsi_ui::tr("none").into_owned() },
            "tours": omsi_ui::tr("%{k} kept, %{c} changed, %{a} new, %{g} dropped").replace("%{k}", &d.kept.len().to_string()).replace("%{c}", &d.changed.len().to_string()).replace("%{a}", &d.added.len().to_string()).replace("%{g}", &d.gone.len().to_string()),
            "replan": omsi_ui::tr("%{n} tours to plan anew").replace("%{n}", &replan.to_string()), "replan_warn": replan > 0,
            "when": when, "earlier": days > 1, "later": days < 60,
            "save": if ask.fee > 0 { omsi_ui::tr("Pay %{amount} and save").replace("%{amount}", &eur(ask.fee)) } else { omsi_ui::tr("Save the change").into_owned() },
        });
    }
    out
}

/// The estimate over the map's foot (`estimate_card`): four figures, and opened every item and
/// the parts of a working day.
fn estimate(s: &Session, f: &Figures) -> Value {
    let eur = crate::words::eur;
    let e = &f.est;
    let result = e.result();
    let fleet: Vec<String> = e.fleet.iter().map(|(sz, n)| format!("{n} × {}", size_label(Some(*sz)))).collect();
    let kind = s.v.line().map(|x| x.service).unwrap_or_default();
    let n = |x: f64| format!("{x:.0}");
    let riders: (&str, String, String) = match kind {
        ServiceKind::School => ("Pupils a school day", n(e.passengers[0]), omsi_ui::tr("on school days only").into_owned()),
        ServiceKind::Leisure => ("Passengers a Sunday", n(e.passengers[2]), omsi_ui::tr("Saturday %{s}").replace("%{s}", &n(e.passengers[1]))),
        _ => ("Passengers a day", n(e.passengers[0]), omsi_ui::tr("Sat %{s} · Sun %{u}").replace("%{s}", &n(e.passengers[1])).replace("%{u}", &n(e.passengers[2]))),
    };
    let fig = |label: &str, value: String, under: String, tone: &str| json!({ "label": omsi_ui::tr(label).to_uppercase(), "value": value, "under": under, "tone": tone });
    let strip = vec![
        fig(if f.confirmed { "Paid to start" } else { "To start" }, eur(if f.confirmed { f.paid } else { e.one_off_total() }), if f.confirmed { omsi_ui::tr("the company runs it").into_owned() } else { omsi_ui::tr("licence, stops, tariff, launch").into_owned() }, "text"),
        fig("A month", eur(result), format!("{} {}  ·  {} {}", omsi_ui::tr("in"), eur(e.revenue()), omsi_ui::tr("out"), eur(e.costs())), if result >= 0 { "ok" } else { "warn" }),
        fig(riders.0, riders.1, riders.2, "text"),
        fig("Buses peak / midday", format!("{} / {}", e.buses_peak, e.buses_offpeak), if fleet.is_empty() { omsi_ui::tr("none yet").into_owned() } else { fleet.join(", ") }, "text"),
    ];
    let item = |text: &str, amount: Cents| json!([omsi_ui::tr(text), eur(amount), amount < 0]);
    let bands: Vec<Value> = e
        .bands
        .iter()
        .enumerate()
        .map(|(k, b)| {
            let (a, z) = own::BAND_HOURS[k];
            let (note, tone) = if b.trips == 0 {
                (omsi_ui::tr("no buses").into_owned(), "faint")
            } else if b.crowded > 0 {
                (omsi_ui::tr("%{bus}: full on %{n} trips, %{need} would fit").replace("%{bus}", &size_label(b.size)).replace("%{n}", &b.crowded.to_string()).replace("%{need}", &size_label(b.needed)), "warn")
            } else if b.oversized > b.trips / 2 {
                (omsi_ui::tr("%{bus}: bigger than needed, %{need} would do").replace("%{bus}", &size_label(b.size)).replace("%{need}", &size_label(b.needed)), "dim")
            } else {
                (omsi_ui::tr("%{bus}, up to %{n} on board").replace("%{bus}", &size_label(b.size)).replace("%{n}", &format!("{:.0}", b.load)), "dim")
            };
            json!({ "label": format!("{}  {:02}–{:02}", omsi_ui::tr(own::BANDS[k]), a, z), "passengers": format!("{:.0}", b.passengers), "note": note, "tone": tone })
        })
        .collect();
    let details = s.v.company.as_ref().is_some_and(|fc| fc.details);
    json!({
        "strip": strip, "details": details,
        "priced": f.priced.then(|| omsi_ui::tr("Estimated with a usual timetable: you make the line's own in RosterFlow's Timeplan").into_owned()),
        "heads": [omsi_ui::tr("Once").to_uppercase(), omsi_ui::tr("A month").to_uppercase(), omsi_ui::tr("A working day").to_uppercase()],
        "once": e.one_off.iter().map(|(t, a)| item(t, *a)).collect::<Vec<_>>(), "together": item("Together", e.one_off_total()),
        "monthly": e.monthly.iter().map(|(t, a)| item(t, *a)).collect::<Vec<_>>(), "result": item("Result", result),
        "bands": bands,
        "foot": omsi_ui::tr("Passengers from the stops, the transfers at them and the service; the rush hours busiest. A month: the drivers, buses and running at full cost."),
    })
}

/// The left panel's foot working for the company (`company_foot`): why it cannot be confirmed or
/// saved, and its buttons' words.
fn foot(s: &Session, f: &Figures) -> Value {
    let v = &s.v;
    let eur = crate::words::eur;
    let problems = v.problems();
    let total = f.est.one_off_total();
    let why: Option<String> = if let Some(p) = problems.first() {
        Some(omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string()))
    } else if !f.confirmed && f.cash < total {
        Some(omsi_ui::tr("The company has %{cash}: not enough for the line. A loan on the Finances page helps.").replace("%{cash}", &eur(f.cash)))
    } else if f.confirmed && f.fee.is_some_and(|x| x > f.cash) {
        Some(omsi_ui::tr("The company has %{cash}: not enough for the change.").replace("%{cash}", &eur(f.cash)))
    } else {
        None
    };
    let waiting = v.line().map(|x| x.pending_from.clone()).filter(|d| !d.is_empty());
    json!({
        "why": why,
        "confirmed": f.confirmed,
        "confirm": omsi_ui::tr("Confirm and pay %{amount}").replace("%{amount}", &eur(total)),
        "save_change": match f.fee {
            Some(fee) if v.dirty => omsi_ui::tr("Pay %{amount} and save").replace("%{amount}", &eur(fee)),
            _ if v.dirty => omsi_ui::tr("Save line").into_owned(),
            _ => omsi_ui::tr("Saved").into_owned(),
        },
        "waiting": waiting.map(|d| omsi_ui::tr("The new timetable begins on %{date}.").replace("%{date}", &crate::words::day_label(&d))),
    })
}

/// Something done to the company (`company::act`): written back when it went, its refusal said
/// when not.
fn company_do<T>(s: &mut Session, f: impl FnOnce(&mut Company) -> Result<T, &'static str>) -> Option<T> {
    let c = s.company.as_mut()?;
    match f(c) {
        Ok(x) => {
            if let Err(e) = co::store::save(&core::data_dir(), c) {
                s.say(format!("{e:#}"), true);
            }
            Some(x)
        }
        Err(e) => {
            s.say(omsi_ui::tr(e).into_owned(), true);
            None
        }
    }
}

/// The company's buttons (`company_action`, `withdraw`, `change_dialog`).
pub(super) fn act(s: &mut Session, op: &str, req: &Value) -> Result<(), String> {
    match op {
        "c_details" => {
            if let Some(fc) = s.v.company.as_mut() {
                fc.details = !fc.details;
            }
            return Ok(());
        }
        "c_change_cancel" => {
            s.v.change_ask = None;
            return Ok(());
        }
        "c_change_day" => {
            if let Some(ask) = s.v.change_ask.as_mut() {
                let d = req.get("d").and_then(Value::as_i64).unwrap_or(0);
                let days = co::dates::between(&ask.today, &ask.from);
                if (d < 0 && days > 1) || (d > 0 && days < 60) {
                    ask.from = co::dates::add(&ask.from, d);
                }
            }
            return Ok(());
        }
        "c_change_save" => {
            let Some(ask) = s.v.change_ask.take() else { return Ok(()) };
            return action(s, CompanyAct::Schedule(ask.from));
        }
        "c_confirm" => return action(s, CompanyAct::Confirm),
        "c_save_change" => return action(s, CompanyAct::SaveChange),
        "c_withdraw" => return action(s, CompanyAct::Withdraw),
        "c_discard" => return action(s, CompanyAct::Discard),
        "c_save_map" => return action(s, CompanyAct::SaveMap),
        "c_restore_map" => return action(s, CompanyAct::RestoreMap),
        _ => {}
    }
    Err(format!("line editor: what is {op}?"))
}

fn action(s: &mut Session, act: CompanyAct) -> Result<(), String> {
    let Some(id) = s.v.sel else { return Ok(()) };
    if matches!(act, CompanyAct::SaveMap | CompanyAct::RestoreMap) {
        map_act(s, act == CompanyAct::RestoreMap);
        return Ok(());
    }
    if act == CompanyAct::Discard {
        let v = &mut s.v;
        let (map, global) = (v.reg.map.clone(), v.reg.global.clone());
        v.reg = reg::load_registry(&v.reg_path);
        (v.reg.map, v.reg.global) = (map, global);
        if v.reg.line(id).is_none() {
            v.sel = v.first_shown();
        }
        (v.dirty, v.sel_stop, v.delete_armed) = (false, None, false);
        v.shapes_for = None;
        v.revision += 1;
        s.say(omsi_ui::tr("The changes are undone").into_owned(), false);
        return Ok(());
    }
    let v = &s.v;
    let Some(line) = v.reg.line(id).cloned() else { return Ok(()) };
    let Some(f) = v.company.as_ref().and_then(|f| f.cache.clone()).filter(|f| f.key.0 == id) else { return Ok(()) };
    let stem = reg::stems(&v.reg).get(&id).cloned().unwrap_or_default();
    let live = reg::load_registry(&v.reg_path).line(id).map(|x| x.live.as_deref().cloned().unwrap_or_else(|| x.clone()));
    let Some(c) = s.company.as_ref() else { return Ok(()) };
    let running = own::line_of(c, id).filter(|x| x.service_from.is_some()).map(|x| x.name.clone());
    let diff = live.as_ref().filter(|old| reg::timetable_differs(old, &line)).map(|old| reg::tour_diff(old, &line));
    match act {
        CompanyAct::Withdraw => {
            withdraw(s, id);
            return Ok(());
        }
        // a line in service whose timetable changes: asked first from when
        CompanyAct::SaveChange if running.is_some() && diff.is_some() => {
            let today = c.date.clone();
            let from = Some(line.pending_from.clone()).filter(|d| co::dates::between(&today, d) >= 1).unwrap_or_else(|| co::dates::add(&today, 1));
            s.v.change_ask = Some(ChangeAsk { fee: f.fee.unwrap_or(0), diff: diff.unwrap_or_default(), today, from });
            return Ok(());
        }
        CompanyAct::Schedule(from) => {
            let (Some(old), Some(d)) = (live, diff) else { return Ok(()) };
            let replan = d.replan();
            let Some(paid) = company_do(s, |c| own::schedule_change(c, &line, &stem, &f.shape, &from, replan.clone())) else { return Ok(()) };
            if let Some(x) = s.v.reg.line_mut(id) {
                (x.draft, x.live, x.pending_from) = (false, Some(Box::new(old)), from.clone());
            }
            s.v.dirty = true;
            let (text, err) = ops::save(s);
            let n = line.number.trim().to_string();
            let mut t = omsi_ui::tr("Line %{n}: the new timetable begins on %{date}; today's tours run as they are.").replace("%{n}", &n).replace("%{date}", &crate::words::day_label(&from));
            if paid > 0 {
                t = format!("{t} {}", omsi_ui::tr("Its route is approved for %{amount}.").replace("%{amount}", &crate::words::eur(paid)));
            }
            company_changed(s);
            if err {
                s.say(text, true);
            } else {
                s.say(t, false);
            }
            return Ok(());
        }
        _ => {}
    }
    let paid = if act == CompanyAct::Confirm {
        company_do(s, |c| own::confirm(c, &line, &stem, &f.shape))
    } else {
        let replan = diff.as_ref().map(|d| d.replan()).unwrap_or_default();
        company_do(s, |c| {
            let paid = own::apply_change(c, &line, &stem, &f.shape)?;
            co::plan::forget_tours(c, &stem, &replan);
            Ok(paid)
        })
    };
    let Some(paid) = paid else { return Ok(()) };
    if let Some(x) = s.v.reg.line_mut(id) {
        (x.live, x.pending_from, x.draft) = (None, String::new(), false);
    }
    s.v.dirty = true;
    let (text, err) = ops::save(s);
    if act == CompanyAct::Confirm {
        s.v.go_timeplan = s.v.company.as_ref().and_then(|fc| after_confirm(fc, &line));
    }
    let timeplan = s.v.go_timeplan.is_some();
    company_changed(s);
    let number = line.number.trim().to_string();
    let eur = crate::words::eur;
    let said = if err {
        (text, true)
    } else if act == CompanyAct::Confirm && timeplan {
        (omsi_ui::tr("Line %{n} is the company's now: %{amount} paid. Make its timetable in RosterFlow's Timeplan.").replace("%{n}", &number).replace("%{amount}", &eur(paid)), false)
    } else if act == CompanyAct::Confirm {
        (omsi_ui::tr("Line %{n} is the company's now: %{amount} paid. Its tours are on the Planning page.").replace("%{n}", &number).replace("%{amount}", &eur(paid)), false)
    } else if paid > 0 {
        (omsi_ui::tr("Line %{n} saved: its new route is approved for %{amount}.").replace("%{n}", &number).replace("%{amount}", &eur(paid)), false)
    } else {
        (text, false)
    };
    s.say(said.0, said.1);
    Ok(())
}

/// A change waiting for its day taken back (`withdraw`): the line stays as it runs.
fn withdraw(s: &mut Session, id: u64) {
    let Some(live) = s.v.reg.line(id).and_then(|x| x.live.as_deref().cloned()) else { return };
    let done = company_do(s, |c| {
        if let Some(x) = c.lines.iter_mut().find(|x| x.plan.as_ref().is_some_and(|p| p.line_id == id)) {
            x.pending = None;
        }
        Ok(())
    });
    if done.is_none() {
        return;
    }
    if let Some(x) = s.v.reg.line_mut(id) {
        *x = LineDesign { id, live: None, pending_from: String::new(), ..live };
    }
    s.v.dirty = true;
    s.v.shapes_for = None;
    s.v.revision += 1;
    let (text, err) = ops::save(s);
    company_changed(s);
    if err {
        s.say(text, true);
    } else {
        s.say(omsi_ui::tr("The change is taken back: the line stays as it runs.").into_owned(), false);
    }
}

/// The company's line of the map's saved or put back as the map has it (`maplines::company_act`).
fn map_act(s: &mut Session, restore_it: bool) {
    let v = &mut s.v;
    let Some(line) = v.line().cloned() else { return };
    let Some(ttl) = line.map.as_ref().map(|m| m.ttl.clone()) else { return };
    let old = v.running.get(&line.id).cloned().unwrap_or_else(|| line.clone());
    let new = if restore_it { maplines::import(v, &ttl).map(|x| x.0).unwrap_or_else(|| line.clone()) } else { line.clone() };
    let Some(paid) = company_do(s, |c| own::apply_map_change(c, &ttl, &old, &new, restore_it)) else { return };
    let (text, err) = if restore_it { ops::restore(s) } else { ops::save_map(s) };
    company_changed(s);
    let number = line.number.trim().to_string();
    if !err && paid > 0 {
        s.say(omsi_ui::tr("Line %{n} saved: its new route is approved for %{amount}.").replace("%{n}", &number).replace("%{amount}", &crate::words::eur(paid)), false);
    } else {
        s.say(text, err);
    }
}

/// The company's pages read it again (its file changed here).
fn company_changed(s: &mut Session) {
    let id = s.company.as_ref().map(|c| c.id.clone());
    let mut go = s.go.take().unwrap_or_else(|| json!({}));
    go["company_changed"] = json!(id);
    go["reload_lines"] = json!(true);
    s.go = Some(go);
}
