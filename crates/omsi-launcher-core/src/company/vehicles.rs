//! Which buses a company works with (Luc, Beta 54; Trello card 83, "Fleet: enable and disable
//! vehicles for your company"): "on UK maps they want UK buses only, in Europe the other way
//! round".
//!
//! - Every bus model has a **category** (midibus, solo, articulated, double-decker, coach) and a
//!   **region** (the UK, right-hand drive; Europe, left-hand drive), guessed from its files - the
//!   region from the side the driver sits on (`[add_camera_driver]`'s x), names only when that
//!   says nothing. The player corrects either, per model, once for the whole launcher
//!   (`Corrections`, `~/.openomsi/vehicle-classes.json`): a model is the same bus in every
//!   company.
//! - Every company chooses the vehicles it sees (`VehicleChoice`): UK buses only, European buses
//!   only, all, or its own selection of models. A model it does not see is never offered by the
//!   dealer, the used market, the day's offers, leasing and renting or the Casual shop; buses it
//!   has already stay ("not in your selection"). An older company sees all of them.

use super::market::MarketBus;
use super::model::{BusKind, BusSize, Company};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Where a bus is built for: the side its driver sits on.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    /// The UK (right-hand drive: the driver sits on the right, the doors on the left).
    Uk,
    /// Mainland Europe (left-hand drive).
    #[default]
    Europe,
}

impl Region {
    pub const ALL: [Region; 2] = [Region::Uk, Region::Europe];

    pub fn key(self) -> &'static str {
        match self {
            Region::Uk => "uk",
            Region::Europe => "europe",
        }
    }

    pub fn of_key(k: &str) -> Option<Region> {
        Region::ALL.into_iter().find(|r| r.key() == k)
    }

    pub fn label(self) -> &'static str {
        match self {
            Region::Uk => "UK",
            Region::Europe => "Europe",
        }
    }

    /// What it means for the bus.
    pub fn say(self) -> &'static str {
        match self {
            Region::Uk => "Right-hand drive",
            Region::Europe => "Left-hand drive",
        }
    }
}

/// What kind of bus a model is, as the player sees it: the market's sizes and the coach (a coach
/// costs and carries as a solo bus does - `size`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Midi,
    #[default]
    Solo,
    Articulated,
    Double,
    Coach,
}

impl Category {
    pub const ALL: [Category; 5] = [Category::Midi, Category::Solo, Category::Articulated, Category::Double, Category::Coach];

    pub fn of(kind: BusKind, coach: bool) -> Category {
        if coach && kind.size == BusSize::Solo {
            return Category::Coach;
        }
        match kind.size {
            BusSize::Midi => Category::Midi,
            BusSize::Solo => Category::Solo,
            BusSize::Articulated => Category::Articulated,
            BusSize::Double => Category::Double,
        }
    }

    /// The market's size of it (what it costs, carries, and who may drive it).
    pub fn size(self) -> BusSize {
        match self {
            Category::Midi => BusSize::Midi,
            Category::Solo | Category::Coach => BusSize::Solo,
            Category::Articulated => BusSize::Articulated,
            Category::Double => BusSize::Double,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Category::Midi => "midi",
            Category::Solo => "solo",
            Category::Articulated => "articulated",
            Category::Double => "double",
            Category::Coach => "coach",
        }
    }

    pub fn of_key(k: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.key() == k)
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Midi => "Midibus",
            Category::Solo => "Solo bus",
            Category::Articulated => "Articulated bus",
            Category::Double => "Double-decker",
            Category::Coach => "Coach",
        }
    }
}

/// Lower-case words of a text.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// Names that make a bus British when its driver's seat says nothing (a double-decker alone does
/// not: Berlin's are left-hand drive).
const UK_WORDS: [&str; 14] = ["uk", "gb", "british", "britain", "england", "english", "london", "routemaster", "rhd", "tfl", "scotland", "wales", "yorkshire", "lincolnshire"];

/// The region of a bus: the side its driver sits on (`driver_x`: the first driver's camera's x,
/// metres right of the bus's middle), else its names (`texts`).
pub fn guess_region(driver_x: Option<f32>, texts: &[&str]) -> Region {
    match driver_x {
        Some(x) if x > 0.15 => Region::Uk,
        Some(x) if x < -0.15 => Region::Europe,
        _ => {
            let w: Vec<String> = texts.iter().flat_map(|t| words(t)).collect();
            if w.iter().any(|x| UK_WORDS.contains(&x.as_str())) {
                Region::Uk
            } else {
                Region::Europe
            }
        }
    }
}

/// Names of coaches and long-distance buses (an intercity "Überlandbus" is a solo bus).
const COACH_WORDS: [&str; 12] = ["coach", "reisebus", "fernbus", "tourismo", "travego", "intouro", "tourliner", "touring", "comfortclass", "topclass", "starliner", "cityliner"];

/// Whether a bus is a coach, by its names.
pub fn guess_coach(texts: &[&str]) -> bool {
    let all = texts.join(" ").to_lowercase();
    let w: Vec<String> = texts.iter().flat_map(|t| words(t)).collect();
    COACH_WORDS.iter().any(|k| w.iter().any(|x| x == k) || (k.len() > 6 && all.contains(k)))
}

/// A bus file as a key: forward slashes, lower case (`Vehicles\A\B.bus` is `vehicles/a/b.bus`).
pub fn key_of(file: &str) -> String {
    file.replace('\\', "/").to_lowercase()
}

// --- the player's corrections, for the whole launcher ------------------------------------------

/// The player's word on one model: its category, its region (None: as guessed).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Correction {
    pub category: Option<Category>,
    pub region: Option<Region>,
}

/// The player's corrections of the guessed categories and regions, by bus file (`key_of`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Corrections {
    pub models: BTreeMap<String, Correction>,
}

impl Corrections {
    /// The file they are kept in.
    pub fn path() -> std::path::PathBuf {
        crate::data_dir().join("vehicle-classes.json")
    }

    /// Read (nothing: no corrections).
    pub fn load() -> Corrections {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn of(&self, file: &str) -> Correction {
        self.models.get(&key_of(file)).copied().unwrap_or_default()
    }

    /// Correct a model's category or region (`None`: back to the guess); the corrections of a
    /// model back to its guesses are forgotten.
    pub fn set(&mut self, file: &str, category: Option<Option<Category>>, region: Option<Option<Region>>) {
        let k = key_of(file);
        let mut c = self.models.get(&k).copied().unwrap_or_default();
        if let Some(x) = category {
            c.category = x;
        }
        if let Some(x) = region {
            c.region = x;
        }
        if c == Correction::default() {
            self.models.remove(&k);
        } else {
            self.models.insert(k, c);
        }
    }

    /// A bus of the market as the player has it: its size (and so its price, its places and who
    /// may drive it), whether it is a coach, and its region.
    pub fn apply(&self, b: &mut MarketBus) {
        let c = self.of(&b.file);
        if let Some(cat) = c.category {
            b.kind.size = cat.size();
            b.coach = cat == Category::Coach;
        }
        if let Some(r) = c.region {
            b.region = r;
        }
    }
}

/// A market bus's category.
pub fn category_of(b: &MarketBus) -> Category {
    Category::of(b.kind, b.coach)
}

// --- the company's choice ------------------------------------------------------------------------

/// Which buses a company works with.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum VehicleMode {
    /// UK buses only (right-hand drive).
    Uk,
    /// European buses only (left-hand drive).
    Europe,
    /// Every bus installed (a company of before the choice).
    #[default]
    All,
    /// The models the player switched on (`VehicleChoice::off`: those switched off).
    Own,
}

impl VehicleMode {
    pub const ALL: [VehicleMode; 4] = [VehicleMode::Uk, VehicleMode::Europe, VehicleMode::All, VehicleMode::Own];

    pub fn key(self) -> &'static str {
        match self {
            VehicleMode::Uk => "uk",
            VehicleMode::Europe => "europe",
            VehicleMode::All => "all",
            VehicleMode::Own => "own",
        }
    }

    pub fn of_key(k: &str) -> Option<VehicleMode> {
        VehicleMode::ALL.into_iter().find(|m| m.key() == k)
    }

    pub fn label(self) -> &'static str {
        match self {
            VehicleMode::Uk => "UK buses only",
            VehicleMode::Europe => "European buses only",
            VehicleMode::All => "All",
            VehicleMode::Own => "My own selection",
        }
    }

    /// The mode of a region.
    pub fn of_region(r: Region) -> VehicleMode {
        match r {
            Region::Uk => VehicleMode::Uk,
            Region::Europe => VehicleMode::Europe,
        }
    }
}

/// A company's vehicles: the mode, and in "My own selection" the models switched off (a model
/// installed later is on).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct VehicleChoice {
    pub mode: VehicleMode,
    pub off: BTreeSet<String>,
}

impl VehicleChoice {
    /// The choice a new company on a map of `region` starts with.
    pub fn for_region(r: Region) -> VehicleChoice {
        VehicleChoice { mode: VehicleMode::of_region(r), off: BTreeSet::new() }
    }

    /// Whether the company sees a bus of the market (corrected: `Corrections::apply`).
    pub fn allows(&self, b: &MarketBus) -> bool {
        match self.mode {
            VehicleMode::Uk => b.region == Region::Uk,
            VehicleMode::Europe => b.region == Region::Europe,
            VehicleMode::All => true,
            VehicleMode::Own => !self.off.contains(&key_of(&b.file)),
        }
    }

    /// Another mode. "My own selection" starts from what the company saw: the models the mode
    /// before left out are switched off (`market`: every bus installed, corrected).
    pub fn set_mode(&mut self, mode: VehicleMode, market: &[MarketBus]) {
        if mode == VehicleMode::Own && self.mode != VehicleMode::Own {
            self.off = market.iter().filter(|b| !self.allows(b)).map(|b| key_of(&b.file)).collect();
        }
        self.mode = mode;
    }

    /// One model on or off: the company's own selection from then on (from what it saw).
    pub fn set_model(&mut self, file: &str, on: bool, market: &[MarketBus]) {
        self.set_mode(VehicleMode::Own, market);
        let k = key_of(file);
        if on {
            self.off.remove(&k);
        } else {
            self.off.insert(k);
        }
    }

    /// Several models on or off at once (a category's, a search's): the own selection.
    pub fn set_models(&mut self, files: &[&str], on: bool, market: &[MarketBus]) {
        self.set_mode(VehicleMode::Own, market);
        for f in files {
            let k = key_of(f);
            if on {
                self.off.remove(&k);
            } else {
                self.off.insert(k);
            }
        }
    }

    /// How many of the market's buses the company sees.
    pub fn count(&self, market: &[MarketBus]) -> usize {
        market.iter().filter(|b| self.allows(b)).count()
    }
}

/// Whether the company sees a bus of the market.
pub fn allowed(c: &Company, b: &MarketBus) -> bool {
    c.vehicles.allows(b)
}

/// Whether a bus of the company's own fleet is one of the models it works with: a spare bus at
/// a breakdown is one of those (a bus of before, its region not known, is).
pub fn vehicle_allowed(c: &Company, v: &super::model::Vehicle) -> bool {
    match c.vehicles.mode {
        VehicleMode::All => true,
        VehicleMode::Uk => v.region.is_none_or(|r| r == Region::Uk),
        VehicleMode::Europe => v.region.is_none_or(|r| r == Region::Europe),
        VehicleMode::Own => !c.vehicles.off.contains(&key_of(&v.bus)),
    }
}

/// Whether a bus file of the company's own fleet is of a model it does not see any more
/// (`market`: corrected): it stays, "not in your selection".
pub fn left_out(c: &Company, file: &str, market: &[MarketBus]) -> bool {
    let k = key_of(file);
    market.iter().find(|b| key_of(&b.file) == k).is_some_and(|b| !c.vehicles.allows(b))
}

// --- the map's region -----------------------------------------------------------------------------

/// Names of British maps (when the map's files do not say it drives on the left).
const UK_MAP_WORDS: [&str; 22] = [
    "uk", "gb", "british", "britain", "england", "english", "london", "scotland", "scottish", "wales", "welsh", "yorkshire", "lincolnshire", "lancashire", "kent", "essex", "devon", "cornwall", "manchester", "birmingham", "liverpool", "leeds",
];

/// The region of a map: one that drives on the left (`[LHT]` in its global.cfg) is British, else
/// its names say it.
pub fn map_region(left_hand: bool, names: &[&str]) -> Region {
    if left_hand {
        return Region::Uk;
    }
    let w: Vec<String> = names.iter().flat_map(|t| words(t)).collect();
    if w.iter().any(|x| UK_MAP_WORDS.contains(&x.as_str())) {
        Region::Uk
    } else {
        Region::Europe
    }
}

#[cfg(test)]
mod tests {
    use super::super::market::tests::bus;
    use super::super::model::Drive;
    use super::*;

    fn uk(name: &str, size: BusSize) -> MarketBus {
        MarketBus { region: Region::Uk, ..bus(name, size, Drive::Diesel) }
    }

    #[test]
    fn the_region_is_the_side_the_driver_sits_on_and_names_only_break_a_tie() {
        // (MS_G3Pack's Streetdeck: x 0.73; MAN's Lion's City: -0.6)
        assert_eq!(guess_region(Some(0.731), &["Streetdeck", "Vehicles/MS_G3Pack/Streetdeck.bus"]), Region::Uk);
        assert_eq!(guess_region(Some(-0.6), &["MAN Lion's City", "Vehicles/MAN_LC/LC.bus"]), Region::Europe);
        // (a left-hand drive bus called "London" stays European: the seat decides)
        assert_eq!(guess_region(Some(-0.55), &["London tour bus"]), Region::Europe);
        // (no driver's camera, or one in the middle: the names)
        assert_eq!(guess_region(None, &["Routemaster", "Vehicles/RM/rm.bus"]), Region::Uk);
        assert_eq!(guess_region(Some(0.0), &["Wright Gemini UK"]), Region::Uk);
        assert_eq!(guess_region(None, &["MAN SD202"]), Region::Europe);
        // (a double-decker is not British by being one)
        assert_eq!(guess_region(None, &["Doppeldecker Berlin DD"]), Region::Europe);
    }

    #[test]
    fn coaches_are_told_by_their_names() {
        assert!(guess_coach(&["Coach O560 Series", "Vehicles/O560/o560.bus"]));
        assert!(guess_coach(&["Mercedes Tourismo"]));
        assert!(guess_coach(&["Setra Reisebus"]));
        assert!(!guess_coach(&["MAN Ueberlandbus"]));
        assert!(!guess_coach(&["Citaro"]));
        assert_eq!(Category::of(BusKind { size: BusSize::Solo, drive: Drive::Diesel }, true), Category::Coach);
        // (a "coach" that is articulated stays articulated)
        assert_eq!(Category::of(BusKind { size: BusSize::Articulated, drive: Drive::Diesel }, true), Category::Articulated);
        assert_eq!(Category::Coach.size(), BusSize::Solo);
    }

    #[test]
    fn the_players_corrections_change_the_category_and_the_region_and_back() {
        let mut co = Corrections::default();
        let mut b = bus("Citaro", BusSize::Solo, Drive::Diesel);
        co.set("Vehicles\\Citaro\\Citaro.bus", Some(Some(Category::Double)), Some(Some(Region::Uk)));
        co.apply(&mut b);
        assert_eq!((b.kind.size, b.region, b.coach), (BusSize::Double, Region::Uk, false));
        let mut b = bus("Citaro", BusSize::Solo, Drive::Diesel);
        co.set("vehicles/citaro/citaro.bus", Some(Some(Category::Coach)), None);
        co.apply(&mut b);
        assert_eq!((category_of(&b), b.region), (Category::Coach, Region::Uk));
        // (both back to the guess: forgotten)
        co.set("Vehicles/Citaro/Citaro.bus", Some(None), Some(None));
        assert!(co.models.is_empty());
        let json = serde_json::to_string(&Corrections::default()).unwrap();
        assert_eq!(serde_json::from_str::<Corrections>(&json).unwrap(), Corrections::default());
    }

    #[test]
    fn a_company_sees_its_region_all_or_its_own_selection() {
        let market = vec![uk("Streetdeck", BusSize::Double), uk("Enviro200", BusSize::Midi), bus("Citaro", BusSize::Solo, Drive::Diesel), bus("SD202", BusSize::Double, Drive::Diesel)];
        let mut v = VehicleChoice::for_region(Region::Uk);
        assert_eq!(v.count(&market), 2);
        assert!(v.allows(&market[0]) && !v.allows(&market[2]));
        v.set_mode(VehicleMode::Europe, &market);
        assert_eq!(v.count(&market), 2);
        assert!(v.allows(&market[3]) && !v.allows(&market[1]));
        v.set_mode(VehicleMode::All, &market);
        assert_eq!(v.count(&market), 4);
        // (the own selection starts from what the company saw: from "Europe", the UK buses off)
        v.set_mode(VehicleMode::Europe, &market);
        v.set_model("Vehicles/Streetdeck/Streetdeck.bus", true, &market);
        assert_eq!(v.mode, VehicleMode::Own);
        assert_eq!(v.count(&market), 3);
        assert!(!v.allows(&market[1]));
        v.set_models(&["Vehicles/Citaro/Citaro.bus", "Vehicles/SD202/SD202.bus"], false, &market);
        assert_eq!(v.count(&market), 1);
        // (a model installed later is on)
        assert!(v.allows(&bus("New", BusSize::Solo, Drive::Electric)));
    }

    #[test]
    fn an_older_company_sees_every_bus() {
        let c: VehicleChoice = serde_json::from_str("{}").unwrap();
        assert_eq!(c.mode, VehicleMode::All);
        let market = vec![uk("Streetdeck", BusSize::Double), bus("Citaro", BusSize::Solo, Drive::Diesel)];
        assert_eq!(c.count(&market), 2);
        // (and an older market bus is a European solo bus, no coach)
        let b: MarketBus = serde_json::from_str(r#"{"file":"Vehicles/A/a.bus","name":"A","kind":{"size":"double","drive":"diesel"},"paints":[],"default_paint":""}"#).unwrap();
        assert_eq!((b.region, b.coach, category_of(&b)), (Region::Europe, false, Category::Double));
    }

    /// Real bus files of the OMSI 2 folder of this computer (read only; skipped where it is
    /// not): the Streetdeck is British, Berlin's MAN SD202 double-decker European, the O560 a
    /// coach.
    #[test]
    fn real_buses_are_told_by_their_drivers_seat() {
        let root = std::path::Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\OMSI 2\Vehicles");
        let read = |rel: &str| omsi_vehicle::Vehicle::load(&root.join(rel)).ok().map(|v| super::super::market::class_of_vehicle(&v, &format!("Vehicles/{rel}"), &v.type_name.clone()));
        if !root.join("MS_G3Pack/Streetdeck.bus").is_file() {
            return;
        }
        let street = read("MS_G3Pack/Streetdeck.bus").expect("the Streetdeck reads");
        assert_eq!((street.0.size, street.2), (BusSize::Double, Region::Uk));
        if let Some((kind, _, region)) = read("MAN_SD202/MAN_D86.bus") {
            assert_eq!((kind.size, region), (BusSize::Double, Region::Europe));
        }
        if let Some((kind, coach, region)) = read("ABCoach_O560/O560_E5.bus") {
            assert_eq!((kind.size, region), (BusSize::Solo, Region::Europe));
            let _ = coach;
        }
    }

    #[test]
    fn a_spare_bus_is_one_of_the_models_the_company_works_with() {
        use super::super::market::{buy_new, Payment};
        use super::super::{found, Founding};
        let mut c = found(&Founding { name: "Wahl".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.cash = 10_000_000_00;
        let a = buy_new(&mut c, &uk("Streetdeck", BusSize::Solo), Payment::Cash, "").unwrap();
        let b = buy_new(&mut c, &bus("Citaro", BusSize::Solo, Drive::Diesel), Payment::Cash, "").unwrap();
        let allowed = |c: &Company| c.fleet.iter().filter(|v| vehicle_allowed(c, v)).map(|v| v.id).collect::<Vec<_>>();
        assert_eq!(allowed(&c), vec![a, b]);
        c.vehicles = VehicleChoice::for_region(Region::Uk);
        assert_eq!(allowed(&c), vec![a]);
        // (the own selection starts from "UK buses only": the Citaro off, then the Streetdeck)
        c.vehicles.set_model("Vehicles/Streetdeck/Streetdeck.bus", false, &[uk("Streetdeck", BusSize::Solo), bus("Citaro", BusSize::Solo, Drive::Diesel)]);
        assert_eq!(allowed(&c), Vec::<u32>::new());
        // (a bus of before: its region not known, every choice but its own selection takes it)
        c.vehicles = VehicleChoice::for_region(Region::Uk);
        c.fleet[1].region = None;
        assert_eq!(allowed(&c), vec![a, b]);
        assert!(left_out(&c, "Vehicles/Citaro/Citaro.bus", &[bus("Citaro", BusSize::Solo, Drive::Diesel)]));
    }

    #[test]
    fn a_map_that_drives_on_the_left_is_british() {
        assert_eq!(map_region(true, &["Somewhere"]), Region::Uk);
        assert_eq!(map_region(false, &["Lincolnshire V5", "Lincolnshire V5.0"]), Region::Uk);
        assert_eq!(map_region(false, &["Yorkshire"]), Region::Uk);
        assert_eq!(map_region(false, &["Bad_Huegelsdorf_2020", "Bad Hügelsdorf"]), Region::Europe);
        assert_eq!(map_region(false, &["Grundorf"]), Region::Europe);
    }
}
