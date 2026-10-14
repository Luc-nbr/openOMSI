//! The player's bus as the game puts it down (the game's `spawn` asks here, and so do the
//! livery studio and the display previews): its file, its coupled parts, its livery and the
//! bus options of `--setvar`.

use crate::host::host;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;

/// The vehicle file the game spawns for `bus` (an articulated bus's rear section: the whole
/// bus).
pub(crate) fn player_bus_path(root: &Path, bus: &str) -> anyhow::Result<PathBuf> {
    host().player_bus_path(root, bus)
}

/// The `[couple_back]` chain behind `vehicle`, read as the game reads it (the studio's tests).
#[cfg(test)]
pub(crate) fn load_coupled_parts(root: &Path, vehicle: &mut omsi_sim::VehicleInstance) -> Vec<Arc<omsi_sim::VehicleType>> {
    host().load_coupled_parts(root, vehicle)
}

/// The paint scheme named `paint` (or numbered so) of `vt`, as the game picks it for
/// `--paint` (its `spawn::paint_scheme`).
pub(crate) fn paint_scheme(vt: &omsi_sim::VehicleType, paint: Option<&str>) -> Option<usize> {
    let asked = paint.map(str::trim).filter(|p| !p.is_empty())?;
    vt.paint_schemes.iter().position(|s| s.name.eq_ignore_ascii_case(asked)).or_else(|| asked.parse::<usize>().ok().filter(|i| *i < vt.paint_schemes.len()))
}

/// The paint scheme of a coupled part (an articulated bus's rear section) for the lead's
/// `scheme`: the part's scheme of the same name, as OMSI pairs the repaints of both parts by
/// their name; a part without one keeps the same place in its list (as before, when the parts'
/// folders list the same repaints in the same order).
pub fn part_scheme(lead: &omsi_sim::VehicleType, scheme: Option<usize>, part: &omsi_sim::VehicleType) -> Option<usize> {
    let i = scheme?;
    let named = lead.paint_schemes.get(i).and_then(|s| part.paint_schemes.iter().position(|p| p.name.eq_ignore_ascii_case(&s.name)));
    named.or_else(|| (i < part.paint_schemes.len()).then_some(i))
}

/// The variables of `--setvar` (`name=value,…`, the bus step's bus options): names trimmed,
/// pieces without a name or an `=` left out.
pub fn setvars(arg: Option<&str>) -> Vec<(String, f32)> {
    arg.into_iter()
        .flat_map(|sv| sv.split(','))
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| !k.trim().is_empty())
        .map(|(k, v)| (k.trim().to_string(), omsi_cfg::parse_f32(v)))
        .collect()
}
