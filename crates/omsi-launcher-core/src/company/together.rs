//! "Play together" in the bus company (Luc, 2026-10-15: "players work together for one company
//! through the bus company interface"): the boss shares his company with a session code; friends
//! join with it, the boss gives each a role, and each drives the duties of the company's day he
//! was given or asked for in his own game, at his own pace. Everything they drive is the boss's
//! company's: its fares, fines and figures.
//!
//! * The boss's launcher runs a small company server (`server`: HTTP on a port of its own, on
//!   the LAN's and a VPN's addresses, and through a Cloudflare tunnel where `cloudflared` is),
//!   found by the company's session code (`code`: `HUB-…`, the game code's sibling - the
//!   addresses, the session whose tunnel is posted on the game's relay, a secret, the company).
//!   The boss opens no world.
//! * A friend's launcher (`client`) posts its messages there (`wire`): the company's day as his
//!   role sees it, taking and giving back duties, starting one (answered with the duty's start:
//!   his game drives it alone, on the duty's day and time), his trip reports, and - as his role
//!   allows - giving duties to players, approving requests, managing players and the company's
//!   own pages.
//! * A player's role is a preset bundle of permissions the boss ticks on top of (`roles`); the
//!   boss's side answers on the company itself (`host`), checking every message against them,
//!   and the company keeps what came of it (`book`): the players with their roles and figures,
//!   who has which duty, and the trips driven, which the day's close books as the boss's own
//!   (measured, with that player's punctuality, passengers, fines and score).

pub mod book;
pub mod client;
pub mod code;
pub mod host;
pub mod roles;
pub mod server;
pub mod wire;
#[cfg(test)]
mod tests;

pub use book::{guest_name, guest_on, Together};
pub use roles::{Perm, Role};
