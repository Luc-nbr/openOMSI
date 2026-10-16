//! Omsi-Hub online, the client side (docs/omsihub/ONLINE.md): the optional account - email and
//! password only, on Supabase Auth - the friends, their online state, the invites to a company
//! session and the opt-in leaderboard (the database's `SECURITY DEFINER` functions, through
//! PostgREST), and the cloud saves (Supabase Storage).
//!
//! * `Client` - signing in and out, the email confirmed and the password reset by a code, the
//!   account deleted; the access token renewed with the refresh token a minute before it expires
//!   and once more on a 401 (a second 401 signs out). An account is made on the website
//!   (`sign_up_page`), not here;
//! * `saves` - the player's two cloud save slots in Storage, a save compressed with zstd and its
//!   sha256 checked both ways;
//! * `store` - the refresh token in the OS's credential store, nowhere else;
//! * `codes` - every answer as one of our codes (`too_many`, `not_found` ...), with its English
//!   text for the launcher to translate: nothing the server says is shown as it is;
//! * `check` - the lengths and shapes checked before anything is sent;
//! * `privacy` - the privacy statement and its version: the account's features wait until the
//!   player agreed to it.
//!
//! No network is no error of the player's: every call then answers `Error::Offline` at once
//! (the connection's own timeouts), and the launcher plays on locally. Nothing here logs.

pub mod check;
mod client;
pub mod codes;
mod http;
pub mod privacy;
pub mod saves;
pub mod store;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod saves_tests;
#[cfg(test)]
mod tests;

pub use client::{Client, Friend, Invite, Profile, Stats};
pub use codes::Error;
pub use saves::{Cloud, SaveError, Slot};

/// The Supabase project `omsi-hub-online` (public, as every client uses it; not a secret).
pub const URL: &str = "https://mlatjtjjdfifbgddcxzu.supabase.co";
/// Its publishable key, the `apikey` header of every request: public by design. What a request may
/// do is decided by the row-level security and the functions' grants (online/supabase/schema.sql).
pub const KEY: &str = "sb_publishable_LF17d5vnctmpyH1h6dRprw_s3CGcD-P";
/// The website (its live address, as in the website's `og:url`).
pub const SITE: &str = "https://omsihub.com";

/// Where an account is made: the website's sign-up (the app has no form of its own).
pub fn sign_up_page() -> String {
    format!("{SITE}/account.html#sign-up")
}

/// Where the client goes: the project's base (Auth `/auth/v1`, REST `/rest/v1`, Storage
/// `/storage/v1` under it) and its publishable key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub(crate) base: String,
    pub(crate) key: String,
    /// Plain HTTP allowed (a stand-in on this computer only).
    pub(crate) plain: bool,
}

impl Endpoints {
    /// The project's own.
    pub fn supabase() -> Self {
        Self { base: URL.into(), key: KEY.into(), plain: false }
    }

    /// Another project, HTTPS only.
    pub fn new(base: &str, key: &str) -> Result<Self, String> {
        if !base.starts_with("https://") {
            return Err(format!("not an HTTPS address: {base}"));
        }
        Ok(Self { base: base.trim_end_matches('/').into(), key: key.into(), plain: false })
    }

    /// A stand-in on this computer (127.0.0.1 or localhost) over plain HTTP: the tests' server,
    /// or a local Supabase to try the flows against.
    pub fn local(base: &str, key: &str) -> Result<Self, String> {
        let host = base.strip_prefix("http://").unwrap_or("").split(['/', ':']).next().unwrap_or("");
        if !matches!(host, "127.0.0.1" | "localhost") {
            return Err(format!("plain HTTP is for this computer only: {base}"));
        }
        Ok(Self { base: base.trim_end_matches('/').into(), key: key.into(), plain: true })
    }

    pub(crate) fn auth(&self, path: &str) -> String {
        format!("{}/auth/v1{path}", self.base)
    }

    pub(crate) fn rest(&self, path: &str) -> String {
        format!("{}/rest/v1{path}", self.base)
    }

    pub(crate) fn storage(&self, path: &str) -> String {
        format!("{}/storage/v1{path}", self.base)
    }
}
