//! Omsi-Hub as an add-on of openOMSI (docs/omsihub/ADDON_PLAN.md, phase 1): what Omsi-Hub puts
//! into an openOMSI it does not ship itself, and how the two talk.
//!
//! * `lua/` - the plugin: a Lua folder plugin on openOMSI's own plugin API (`omsi-plugin`), so it
//!   runs in an unchanged openOMSI on every system. It reads the duty the company assigned,
//!   offers or takes it, reports each trip with its ratings back, and shows the company's
//!   notices on the screen;
//! * [`bridge`] - the files between Omsi-Hub and the plugin (the assignment, the handshake, the
//!   reports), with a fallback through the game's log for an openOMSI without file functions;
//! * [`compat`] - which openOMSI the add-on works with and how well: the version read from the
//!   folder first, then the plugin's handshake (the API's ABI, function by function);
//! * [`detect`] - an openOMSI folder the player chose: its program, its version, its plugins
//!   folder;
//! * [`install`] - the plugin put there, switched on and off;
//! * [`version`] - openOMSI's version numbers.
//!
//! Nothing here depends on the game's crates: the launcher (and later Omsi-Hub on its own) uses
//! it beside any openOMSI.

pub mod bridge;
pub mod compat;
pub mod detect;
pub mod install;
pub mod version;

pub use version::GameVersion;
