//! "Play together" in the bus company (Luc, 2026-10-15: "the plan with the session code for the
//! company mode"): the boss runs his company and hosts its world; friends join with a session
//! code and drive duties of the company's day for it.
//!
//! * The boss's launcher runs a small company server (`server`: HTTP on a port of its own, on
//!   the LAN's and a VPN's addresses, and through a Cloudflare tunnel where `cloudflared` is),
//!   found by the company's session code (`code`: `HUB-…`, the game code's sibling - the
//!   addresses, the session whose tunnel is posted on the game's relay, a secret, the company).
//! * A friend's launcher (`client`) posts its messages there (`wire`): the company's day as a
//!   driver needs it, taking and giving back duties, starting one (answered with the duty's
//!   start and the boss's game session code: the friend's game joins the boss's world with
//!   `--lan-join` and drives the duty there - the host's AI bus of that tour leaves), and its
//!   trip reports.
//! * The boss's side answers on the company itself (`host`), and the company keeps what came of
//!   it (`book`): who took which duty, and the trips driven, which the day's close books as the
//!   boss's own (measured, with that player's punctuality, passengers and score). Money and
//!   decisions stay the boss's; every claim and report is checked against the plan.

pub mod book;
pub mod client;
pub mod code;
pub mod host;
pub mod server;
pub mod wire;
#[cfg(test)]
mod tests;

pub use book::{guest_name, guest_on, Together};
