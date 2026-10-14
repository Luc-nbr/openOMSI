//! The bus step's bus options (Trello "Tauri 3/6"), as our game-drawn launcher's
//! (`crates/omsi-launcher-ui/src/busoptions.rs`, after Omsi-Hub's paint studio): the
//! `[setvar]` variables of the chosen bus's liveries - its mirrors, rims, seats, door types -
//! offered to choose; "As the livery" leaves one to the livery chosen. A value chosen is kept
//! for the bus file (`omsi_launcher_lib::busoptions`, the file the game-drawn launcher keeps
//! them in) and goes to the game as `--setvar` after the paint.
//!
//! An option is "appearance" when no script of the bus reads it and a mesh's `[visible]` or a
//! material's `[matl_change]` does; every other one is "technical". What most liveries give an
//! option (one that does not set it counting as 0) is its usual value. The words are the page's
//! (it translates them); this answers what is.

use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::busoptions::BusOptions;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// A livery as the options see it: its name and what its `[setvar]` lines set, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Livery {
    pub name: String,
    pub vars: Vec<(String, f32)>,
}

impl Livery {
    /// The value it gives `var` (its last line for it: the game sets them in order).
    fn value(&self, var: &str) -> Option<f32> {
        self.vars.iter().rev().find(|(n, _)| n.trim().eq_ignore_ascii_case(var.trim())).map(|(_, v)| *v)
    }
}

/// A value an option takes, and the liveries that give it.
#[derive(Clone, Debug, PartialEq)]
struct Seen {
    value: f32,
    liveries: Vec<String>,
}

/// One bus option: a variable the liveries set (`technical`: a script works with it).
#[derive(Clone, Debug, PartialEq)]
struct BusOption {
    var: String,
    label: String,
    technical: bool,
    values: Vec<Seen>,
    usual: f32,
}

impl BusOption {
    /// Seen only as 0 and 1: off or on.
    fn is_switch(&self) -> bool {
        self.values.iter().all(|s| s.value == 0.0 || s.value == 1.0)
    }
}

/// What a bus offers: its liveries (as the game lists them) and the options they make.
#[derive(Clone, Debug, Default, PartialEq)]
struct Catalogue {
    liveries: Vec<Livery>,
    options: Vec<BusOption>,
}

impl Catalogue {
    fn livery(&self, name: &str) -> Option<&Livery> {
        let n = name.trim();
        if n.is_empty() {
            return None;
        }
        self.liveries.iter().find(|l| l.name.trim().eq_ignore_ascii_case(n))
    }
}

/// A variable's name as a person reads it (`busoptions::label_of`): the setvar prefixes off,
/// the words apart, in sentence case - an abbreviation, a lone letter or a short code as it is.
fn label_of(var: &str) -> String {
    let mut rest = var.trim();
    loop {
        let lower = rest.to_ascii_lowercase();
        match ["vis_", "cti_", "sv_", "setvar_"].iter().find(|p| lower.starts_with(**p) && rest.len() > p.len()) {
            Some(p) => rest = &rest[p.len()..],
            None => break,
        }
    }
    let words = split_words(rest);
    if words.is_empty() {
        return var.trim().to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
        let letters = w.chars().filter(|c| c.is_alphabetic()).count();
        let digits = w.chars().filter(|c| c.is_ascii_digit()).count();
        let caps = letters > 0 && w.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
        let word = if (caps && letters > 1) || (letters == 1 && w.chars().count() == 1) || (digits > 0 && letters > 0 && w.chars().count() <= 3) {
            w.to_uppercase()
        } else if i == 0 {
            let lower = w.to_lowercase();
            let mut c = lower.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        } else {
            w.to_lowercase()
        };
        out.push(word);
    }
    out.join(" ")
}

/// The words of a name (`busoptions::split_words`).
fn split_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for piece in s.split(['_', '-', ' ', '.']).filter(|p| !p.is_empty()) {
        let cs: Vec<char> = piece.chars().collect();
        let mut cur = String::new();
        for (i, &c) in cs.iter().enumerate() {
            if i > 0 {
                let p = cs[i - 1];
                let next_small = cs.get(i + 1).is_some_and(|n| n.is_lowercase());
                let apart = (p.is_lowercase() && c.is_uppercase()) || (p.is_alphabetic() && c.is_ascii_digit() && cur.chars().count() > 1) || (p.is_ascii_digit() && c.is_alphabetic()) || (p.is_uppercase() && c.is_uppercase() && next_small);
                if apart && !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out
}

/// Whether a script's text (lower case) works with `var` (lower case).
fn mentioned(text: &str, var: &str) -> bool {
    let needle = format!(".l.{var}");
    let mut from = 0;
    while let Some(i) = text[from..].find(&needle) {
        let end = from + i + needle.len();
        if !text[end..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            return true;
        }
        from = end;
    }
    false
}

/// Whether an option is technical (`busoptions::kind_of`): a script (not a "visual" one)
/// works with it, or nothing shows it.
fn technical(var: &str, scripts: &[(String, String)], shown: &HashSet<String>) -> bool {
    let v = var.trim().to_ascii_lowercase();
    let scripted = scripts.iter().any(|(name, text)| !name.contains("visual") && mentioned(text, &v));
    scripted || !shown.contains(&v)
}

/// What most liveries give `var`, one that does not set it counting as 0 (`usual_of`).
fn usual_of(liveries: &[Livery], var: &str) -> f32 {
    let given: Vec<f32> = liveries.iter().map(|l| l.value(var).unwrap_or(0.0)).map(|v| if v == 0.0 { 0.0 } else { v }).collect();
    let mut count: HashMap<u32, usize> = HashMap::new();
    for v in &given {
        *count.entry(v.to_bits()).or_default() += 1;
    }
    let most = count.values().copied().max().unwrap_or(0);
    given.iter().copied().find(|v| count.get(&v.to_bits()) == Some(&most)).unwrap_or(0.0)
}

/// The options `liveries` make (`gather`): every variable one of them sets (`Colorscheme`
/// aside), with its values; the appearance ones first, each kind by its label.
fn gather(liveries: &[Livery], scripts: &[(String, String)], shown: &HashSet<String>) -> Vec<BusOption> {
    let mut order: Vec<String> = Vec::new();
    let mut seen: HashMap<String, Vec<Seen>> = HashMap::new();
    for l in liveries {
        let mut own: Vec<(String, f32)> = Vec::new();
        for (n, v) in &l.vars {
            let n = n.trim();
            if n.is_empty() || n.eq_ignore_ascii_case("Colorscheme") || !v.is_finite() {
                continue;
            }
            match own.iter_mut().find(|(o, _)| o.eq_ignore_ascii_case(n)) {
                Some(o) => o.1 = *v,
                None => own.push((n.to_string(), *v)),
            }
        }
        for (n, v) in own {
            let list = seen.entry(n.to_ascii_lowercase()).or_insert_with(|| {
                order.push(n.clone());
                Vec::new()
            });
            match list.iter_mut().find(|s| s.value == v) {
                Some(s) => {
                    if !s.liveries.iter().any(|x| x.eq_ignore_ascii_case(&l.name)) {
                        s.liveries.push(l.name.clone());
                    }
                }
                None => list.push(Seen { value: v, liveries: vec![l.name.clone()] }),
            }
        }
    }
    let mut out: Vec<BusOption> = order
        .into_iter()
        .map(|var| {
            let mut values = seen.remove(&var.to_ascii_lowercase()).unwrap_or_default();
            values.sort_by(|a, b| a.value.total_cmp(&b.value));
            BusOption { label: label_of(&var), technical: technical(&var, scripts, shown), usual: usual_of(liveries, &var), values, var }
        })
        .collect();
    out.sort_by(|a, b| a.technical.cmp(&b.technical).then_with(|| super::name_cmp(&a.label, &b.label)).then_with(|| a.var.to_ascii_lowercase().cmp(&b.var.to_ascii_lowercase())));
    out
}

fn same(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn pick(picks: &BTreeMap<String, f32>, var: &str) -> Option<f32> {
    picks.iter().find(|(k, _)| k.eq_ignore_ascii_case(var)).map(|(_, v)| *v)
}

/// What the game gets (`busoptions::setvars`): every value chosen that the livery does not set
/// to the same itself; before the bus was read, every value chosen.
fn setvars(cat: Option<&Catalogue>, livery: &str, picks: &BTreeMap<String, f32>) -> Vec<(String, f32)> {
    let Some(c) = cat else {
        return picks.iter().map(|(k, v)| (k.clone(), *v)).collect();
    };
    let lv = c.livery(livery);
    c.options
        .iter()
        .filter_map(|o| {
            let v = pick(picks, &o.var)?;
            (!lv.and_then(|l| l.value(&o.var)).is_some_and(|l| same(l, v))).then(|| (o.var.clone(), v))
        })
        .collect()
}

/// The vehicle file the game drives for `bus` (`spawn::player_bus_path`): a rear section's
/// front.
pub(super) fn player_bus_path(root: &Path, bus: &str) -> anyhow::Result<PathBuf> {
    let path = omsi_cfg::resolve_path(root, bus);
    let def = omsi_vehicle::Vehicle::load(&path)?;
    let fronts = if def.coupling_front.is_some() { omsi_vehicle::vehicle::front_sections_of(&path) } else { Vec::new() };
    if (fronts.is_empty() && !def.is_rear_section()) || def.is_rail() {
        return Ok(path);
    }
    fronts.first().cloned().ok_or_else(|| anyhow::anyhow!("{} is a rear section", path.display()))
}

/// Read what the bus offers (`busoptions::read`): its liveries as the game makes them, its
/// scripts and its model's `[visible]` and `[matl_change]` variables.
fn read(root: &Path, bus: &str) -> anyhow::Result<Catalogue> {
    use anyhow::Context;
    let path = player_bus_path(root, bus)?;
    let def = omsi_vehicle::Vehicle::load(&path).with_context(|| format!("loading {}", path.display()))?;
    let model_rel = def.model.clone().context("the bus has no [model]")?;
    let model_path = omsi_cfg::resolve_path(def.dir(), &model_rel);
    let model = omsi_model::Model::load(&model_path).with_context(|| format!("loading {}", model_path.display()))?;
    let liveries: Vec<Livery> = omsi_sim::vehicle::model_paint_schemes(def.dir(), &model).into_iter().map(|s| Livery { name: s.name.trim().to_string(), vars: s.set_vars }).collect();
    let scripts: Vec<(String, String)> = def
        .scripts
        .scripts
        .iter()
        .filter_map(|p| {
            let text = omsi_cfg::codepage::decode(&omsi_cfg::vfs::read(p).ok()?);
            Some((p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default(), text.to_lowercase()))
        })
        .collect();
    let shown: HashSet<String> = model
        .meshes
        .iter()
        .flat_map(|m| m.visible.iter().map(|v| v.0.trim().to_ascii_lowercase()).chain(m.materials.iter().filter_map(|t| t.change.as_ref().map(|c| c.2.trim().to_ascii_lowercase()))))
        .collect();
    let options = gather(&liveries, &scripts, &shown);
    Ok(Catalogue { liveries, options })
}

enum Slot {
    Reading,
    Ready(Arc<Catalogue>),
    Failed,
}

fn slots() -> &'static Mutex<HashMap<String, Slot>> {
    static S: OnceLock<Mutex<HashMap<String, Slot>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// What `bus` offers: read on a thread the first time it is asked for (None meanwhile, `Err`
/// when it could not be read).
fn catalogue(bus: &str) -> Result<Option<Arc<Catalogue>>, ()> {
    let Ok(root) = core::root_dir() else { return Err(()) };
    let key = format!("{}|{}", root.display(), core::busoptions::bus_key(bus));
    let mut m = slots().lock().unwrap_or_else(|e| e.into_inner());
    match m.get(&key) {
        Some(Slot::Ready(c)) => return Ok(Some(c.clone())),
        Some(Slot::Failed) => return Err(()),
        Some(Slot::Reading) => return Ok(None),
        None => {}
    }
    m.insert(key.clone(), Slot::Reading);
    let bus = bus.to_string();
    std::thread::spawn(move || {
        let slot = match std::panic::catch_unwind(|| read(&root, &bus)) {
            Ok(Ok(c)) => Slot::Ready(Arc::new(c)),
            _ => Slot::Failed,
        };
        slots().lock().unwrap_or_else(|e| e.into_inner()).insert(key, slot);
    });
    Ok(None)
}

fn arg_str(args: &Value, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The bus options of `bus` in livery `paint`, as the section shows them: `{ reading }` while
/// the bus is read, `{ options: [] }` for a bus without any (or one that could not be read),
/// else each option with its values (and the liveries giving each), what the livery gives it,
/// its usual value and the value chosen.
pub fn view(args: &Value) -> Answer {
    let (bus, paint) = (arg_str(args, "bus"), arg_str(args, "paint"));
    let cat = match catalogue(&bus) {
        Ok(Some(c)) => c,
        Ok(None) => return Ok(json!({ "reading": true })),
        Err(()) => return Ok(json!({ "options": [] })),
    };
    let picks = BusOptions::load().of(&bus);
    let lv = cat.livery(&paint);
    let options: Vec<Value> = cat
        .options
        .iter()
        .map(|o| {
            let chosen = pick(&picks, &o.var);
            json!({
                "var": o.var, "label": o.label, "technical": o.technical, "switch": o.is_switch(), "usual": o.usual,
                "livery": lv.and_then(|l| l.value(&o.var)), "chosen": chosen,
                "values": o.values.iter().map(|s| json!({ "value": s.value, "liveries": s.liveries })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({ "options": options }))
}

/// A value chosen for `var` of `bus` (`value` null: as the livery again), or `reset`: every
/// option as the livery. Kept at once.
pub fn set(args: &Value) -> Answer {
    let bus = arg_str(args, "bus");
    let mut picks = BusOptions::load();
    if args.get("reset").and_then(Value::as_bool).unwrap_or(false) {
        picks.reset(&bus);
    } else {
        let var = arg_str(args, "var");
        picks.set(&bus, &var, args.get("value").and_then(Value::as_f64).map(|v| v as f32));
    }
    picks.save().map_err(|e| e.to_string())?;
    Ok(json!(true))
}

/// What the game gets for `bus` in livery `paint` (`--setvar`): `[[var, value]]`.
pub fn for_game(args: &Value) -> Answer {
    let (bus, paint) = (arg_str(args, "bus"), arg_str(args, "paint"));
    let picks = BusOptions::load().of(&bus);
    if picks.is_empty() {
        return Ok(json!([]));
    }
    let cat = catalogue(&bus).ok().flatten();
    Ok(json!(setvars(cat.as_deref(), &paint, &picks)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn livery(name: &str, vars: &[(&str, f32)]) -> Livery {
        Livery { name: name.into(), vars: vars.iter().map(|(n, v)| (n.to_string(), *v)).collect() }
    }

    #[test]
    fn the_options_read_and_go_to_the_game_as_the_game_drawn_launchers() {
        assert_eq!(label_of("vis_mirrors"), "Mirrors");
        assert_eq!(label_of("vis_CTI_LEDColor"), "LED color");
        assert_eq!(label_of("Rad_V"), "Rad V");
        let lv = [livery("BVG", &[("vis_mirrors", 1.0), ("Colorscheme", 3.0)]), livery("HHA", &[("vis_mirrors", 0.0), ("vis_seats", 2.0)]), livery("Plain", &[])];
        let shown: HashSet<String> = ["vis_mirrors".to_string()].into_iter().collect();
        let scripts = vec![("main.osc".to_string(), "(l.l.vis_seats)".to_string())];
        let o = gather(&lv, &scripts, &shown);
        assert_eq!(o.iter().map(|x| (x.var.as_str(), x.technical)).collect::<Vec<_>>(), vec![("vis_mirrors", false), ("vis_seats", true)], "the appearance ones first, Colorscheme left out");
        assert!(o[0].is_switch() && !o[1].is_switch());
        assert_eq!(o[0].usual, 0.0, "a livery that sets nothing counts as 0");
        let cat = Catalogue { liveries: lv.to_vec(), options: o };
        let picks: BTreeMap<String, f32> = [("vis_mirrors".to_string(), 1.0), ("vis_seats".to_string(), 2.0)].into_iter().collect();
        assert_eq!(setvars(Some(&cat), "BVG", &picks), vec![("vis_seats".to_string(), 2.0)], "what the livery sets the same is not passed");
        assert_eq!(setvars(None, "BVG", &picks).len(), 2, "before the bus was read: every choice");
    }
}
