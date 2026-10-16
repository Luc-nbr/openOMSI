//! Every answer the client gives as one of our codes, each with its English text (the launcher
//! translates it: `TEXTS` is in every locale, `omsi-launcher-api`'s test keeps it so). What the
//! server says in its own words - Supabase Auth's messages, PostgreSQL's and Storage's errors - is
//! read for its code only and never shown, so no internals reach the player.

use serde_json::Value;

/// What went wrong, as the launcher says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// No network, the server not reached or too slow: Omsi-Hub plays on locally.
    Offline,
    /// The session is over (a 401 a fresh access token did not cure, or the refresh token refused):
    /// the token is gone from the store.
    SignedOut,
    /// One of `TEXTS`' codes.
    Code(&'static str),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::Offline => "offline",
            Error::SignedOut => "signed_out",
            Error::Code(c) => c,
        }
    }

    /// Its English text.
    pub fn text(&self) -> &'static str {
        text(self.code())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.text())
    }
}

/// The answers of `send_friend_request` that are a success.
pub const ADDED: &[&str] = &["sent", "accepted"];

/// Every code and its text. The functions' own answers (docs/omsihub/ONLINE.md) come first.
pub const TEXTS: &[(&str, &str)] = &[
    ("sent", "Friend request sent."),
    ("accepted", "You are friends now: they had asked you already."),
    ("already", "You are friends already, or a request is open."),
    ("self", "That is your own friend code."),
    ("not_found", "No player has that friend code."),
    ("no_profile", "Choose your display name first."),
    ("too_many", "Too many tries in a short time. Try again later."),
    ("full", "The friends list is full."),
    // (the account's)
    ("invalid_credentials", "The email address or the password is not right."),
    ("wrong_password", "The password is not right."),
    ("email_not_verified", "Confirm your email address first: enter the code we sent you."),
    ("email_taken", "There is an account with this email address already. Sign in, or reset its password."),
    ("invalid_email", "That is not an email address."),
    ("password_too_short", "The password is too short: at least 8 characters."),
    ("password_too_long", "The password is too long: at most 72 characters."),
    ("weak_password", "The password is too weak: choose a longer one, with letters, digits and symbols."),
    ("same_password", "The new password is the one you have now: choose another."),
    ("invalid_code", "That code is not right."),
    ("code_expired", "That code has expired. Ask for a new one."),
    ("too_many_attempts", "Too many wrong codes. Ask for a new one."),
    ("reauth", "Sign in again first, then try once more."),
    // (the profile's and the friends')
    ("name_taken", "Another player has that display name already."),
    ("bad_name", "A display name has 3 to 24 characters: letters A to Z, digits, spaces, _ . and -."),
    ("bad_friend_code", "A friend code has 8 letters and digits."),
    ("bad_input", "Something in it is too long or not allowed."),
    ("not_allowed", "That is not allowed."),
    ("no_session", "Share your company first (Multiplayer): then there is a session to invite to."),
    // (the cloud saves': their texts are in cloud-saves.yml)
    ("limit", "Both cloud slots are taken: keep this company local, or replace one of your 2 cloud companies."),
    ("conflict", "This company changed on this PC and in the cloud since it was last synced."),
    ("too_large", "This company is too large for the cloud (at most 25 MB compressed). It stays local."),
    ("no_cloud_save", "This cloud save is not there any more."),
    ("upload_failed", "The cloud storage refused the upload. Try again."),
    ("bad_save", "The cloud save is damaged: its checksum is wrong. Nothing was changed."),
    // (the state)
    ("not_signed_in", "Sign in first."),
    ("privacy", "Please accept the Privacy Statement before using your account."),
    ("offline", "Offline: the online features are back as soon as the network is."),
    ("signed_out", "You were signed out. Sign in again."),
    ("unavailable", "The server does not offer this yet."),
    ("server", "The server could not do that just now. Try again later."),
];

/// A code's English text (an unknown code's is the server's).
pub fn text(code: &str) -> &'static str {
    TEXTS.iter().find(|(c, _)| *c == code).or_else(|| TEXTS.iter().find(|(c, _)| *c == "server")).map(|(_, t)| *t).unwrap_or("")
}

/// A word the server answered, as our code, when it is one.
pub fn known(word: &str) -> Option<&'static str> {
    let w = word.trim().to_ascii_lowercase();
    TEXTS.iter().map(|(c, _)| *c).find(|c| *c == w)
}

/// A function's answer as a word: a text, the first row's, or an object's `result`/`status`/
/// `code` (as PostgREST answers a scalar, a table or a composite).
pub(crate) fn word_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.first().and_then(word_of),
        Value::Object(o) => ["result", "status", "code"].iter().find_map(|k| o.get(*k)).and_then(word_of).or_else(|| (o.len() == 1).then(|| o.values().next().and_then(word_of)).flatten()),
        _ => None,
    }
}

/// Supabase Auth's error as ours: its `error_code` (`{code, error_code, msg}`), or an older
/// GoTrue's `error` (`{error, error_description}`). A session it no longer knows is `SignedOut`.
pub(crate) fn of_auth(status: u16, body: &Value) -> Error {
    let code = body.get("error_code").and_then(Value::as_str).or_else(|| body.get("error").and_then(Value::as_str)).unwrap_or("");
    Error::Code(match code {
        "invalid_credentials" | "invalid_grant" => "invalid_credentials",
        "email_not_confirmed" => "email_not_verified",
        "user_already_exists" | "email_exists" => "email_taken",
        "weak_password" => "weak_password",
        "same_password" => "same_password",
        // (Supabase says the same of a wrong code and an expired one)
        "otp_expired" => "invalid_code",
        "validation_failed" | "email_address_invalid" => "invalid_email",
        "over_email_send_rate_limit" | "over_request_rate_limit" => "too_many",
        "otp_disabled" | "signup_disabled" | "email_provider_disabled" => "unavailable",
        "reauthentication_needed" | "reauthentication_not_valid" => "reauth",
        "session_not_found" | "session_expired" | "refresh_token_not_found" | "refresh_token_already_used" | "bad_jwt" | "user_not_found" | "user_banned" => return Error::SignedOut,
        _ if status == 429 => "too_many",
        _ if status == 404 => "unavailable",
        _ if status == 422 => "bad_input",
        _ => "server",
    })
}

/// PostgREST's error codes that mean the access token is not taken (expired, invalid), the
/// SQLSTATE `hub_me()` raises without a player, and Storage's `InvalidJWT` (a 400 with a 403 in it).
pub(crate) fn is_unauthorized(status: u16, body: &Value) -> bool {
    status == 401 || matches!(body.get("code").and_then(Value::as_str), Some("PGRST301" | "PGRST302" | "PGRST303" | "28000")) || body.get("error").and_then(Value::as_str) == Some("InvalidJWT")
}

/// PostgREST's error (`{code, message, details, hint}`) as ours: a function's own code raised as
/// its message (`RAISE EXCEPTION 'too_many'`), else by the SQLSTATE.
pub(crate) fn of_data(status: u16, body: &Value) -> Error {
    let message = body.get("message").and_then(Value::as_str).unwrap_or("");
    if let Some(c) = known(message).or_else(|| message.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).find_map(known)) {
        if !matches!(c, "offline" | "signed_out" | "server") {
            return Error::Code(c);
        }
    }
    let state = body.get("code").and_then(Value::as_str).unwrap_or("");
    Error::Code(match state {
        "23505" => "name_taken",
        "23514" | "22001" | "22023" | "22P02" | "23502" => "bad_input",
        "42501" => "not_allowed",
        "PGRST202" | "42883" => "unavailable",
        _ if status == 429 => "too_many",
        _ if status == 404 => "unavailable",
        _ => "server",
    })
}

/// The status Storage means: its body's `statusCode` (it answers many errors as a 400 with the
/// real one inside), else the HTTP status.
pub(crate) fn storage_status(status: u16, body: &Value) -> u16 {
    match body.get("statusCode") {
        Some(Value::String(s)) => s.parse().unwrap_or(status),
        Some(Value::Number(n)) => n.as_u64().and_then(|n| u16::try_from(n).ok()).unwrap_or(status),
        _ => status,
    }
}

/// Storage's error (`{statusCode, error, message}`) as ours.
pub(crate) fn of_storage(status: u16, body: &Value) -> Error {
    Error::Code(match storage_status(status, body) {
        404 => "not_found",
        413 => "too_large",
        403 => "not_allowed",
        409 => "conflict",
        429 => "too_many",
        400 | 422 => "bad_input",
        _ => "server",
    })
}
