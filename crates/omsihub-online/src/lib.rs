//! Omsi-Hub online, the client side (docs/omsihub/ONLINE.md): the optional account - email and
//! password only, on Neon Auth (Better Auth) - and the friends, their online state and the
//! invites to a company session, on Neon's Data API (its `SECURITY DEFINER` functions).
//!
//! * `Client` - signing up, in and out, the email confirmed and the password reset by a code,
//!   the account deleted; the Data API's calls with the JWT, fetched again before it expires
//!   and once more on a 401 (a second 401 signs out);
//! * `saves` - the cloud saves: the function `saves`' routes with the same JWT, a save
//!   compressed with zstd and moved by the presigned URLs, its sha256 checked both ways;
//! * `store` - the session token in the OS's credential store, nowhere else;
//! * `codes` - every answer as one of our codes (`too_many`, `not_found` ...), with its English
//!   text for the launcher to translate: nothing the server says is shown as it is;
//! * `check` - the lengths and shapes checked before anything is sent.
//!
//! No network is no error of the player's: every call then answers `Error::Offline` at once
//! (the connection's own timeouts), and the launcher plays on locally. Nothing here logs.

pub mod check;
mod client;
pub mod codes;
mod http;
pub mod saves;
pub mod store;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod saves_tests;
#[cfg(test)]
mod tests;

pub use client::{Client, Friend, Invite, Profile, SignUp};
pub use codes::Error;
pub use saves::{Cloud, SaveError, Slot};

/// Neon Auth of the project `omsi-hub-online` (public, as every client uses it; not a secret).
pub const AUTH: &str = "https://ep-polished-silence-b9tp8jw4.neonauth.c-7.eu-central-1.aws.neon.tech/neondb/auth";
/// Its Data API.
pub const DATA: &str = "https://ep-polished-silence-b9tp8jw4.apirest.c-7.eu-central-1.aws.neon.tech/neondb/rest/v1";
/// Its cloud saves (the Neon Function `saves`).
pub const SAVES: &str = "https://br-shy-water-b9vx3gyb-saves.compute.c-7.eu-central-1.aws.neon.tech/";

/// Where the client goes: the auth's base, the Data API's and the cloud saves' function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub(crate) auth: String,
    pub(crate) data: String,
    pub(crate) saves: String,
    /// Plain HTTP allowed (the tests' server on this computer only).
    pub(crate) plain: bool,
}

impl Endpoints {
    /// The project's own.
    pub fn neon() -> Self {
        Self { auth: AUTH.into(), data: DATA.into(), saves: SAVES.trim_end_matches('/').into(), plain: false }
    }

    /// Others, HTTPS only.
    pub fn new(auth: &str, data: &str, saves: &str) -> Result<Self, String> {
        for u in [auth, data, saves] {
            if !u.starts_with("https://") {
                return Err(format!("not an HTTPS address: {u}"));
            }
        }
        Ok(Self { auth: auth.trim_end_matches('/').into(), data: data.trim_end_matches('/').into(), saves: saves.trim_end_matches('/').into(), plain: false })
    }

    /// The tests' server on 127.0.0.1, over plain HTTP.
    #[cfg(test)]
    pub(crate) fn local(base: &str) -> Self {
        assert!(base.starts_with("http://127.0.0.1:"), "plain HTTP is for the loopback only");
        Self { auth: format!("{base}/auth"), data: format!("{base}/rest/v1"), saves: format!("{base}/saves"), plain: true }
    }
}
