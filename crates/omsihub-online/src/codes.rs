//! Every answer the client gives as one of our codes, each with its English text (the launcher
//! translates it: `TEXTS` is in every locale, `omsi-launcher-api`'s test keeps it so). What the
//! server says in its own words - Better Auth's messages, PostgreSQL's errors - is read for its
//! code only and never shown, so no internals reach the player.

use serde_json::Value;

/// What went wrong, as the launcher says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// No network, the server not reached or too slow: Omsi-Hub plays on locally.
    Offline,
    /// The session is over (a 401 a fresh JWT did not cure): the token is gone from the store.
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
    ("password_too_long", "The password is too long: at most 128 characters."),
    ("invalid_code", "That code is not right."),
    ("code_expired", "That code has expired. Ask for a new one."),
    ("too_many_attempts", "Too many wrong codes. Ask for a new one."),
    ("reauth", "Sign in again first, then try once more."),
    // (the profile's and the friends')
    ("name_taken", "Another player has that display name already."),
    ("bad_name", "A display name has 3 to 24 characters."),
    ("bad_friend_code", "A friend code has 8 letters and digits."),
    ("bad_input", "Something in it is too long or not allowed."),
    ("not_allowed", "That is not allowed."),
    ("no_session", "Share your company first (Multiplayer): then there is a session to invite to."),
    // (the state)
    ("not_signed_in", "Sign in first."),
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

/// Better Auth's error (`{code, message}`, its `BASE_ERROR_CODES` and the email-OTP plugin's)
/// as ours.
pub(crate) fn of_auth(status: u16, body: &Value) -> Error {
    let code = body.get("code").and_then(Value::as_str).unwrap_or("");
    Error::Code(match code {
        "INVALID_EMAIL_OR_PASSWORD" | "CREDENTIAL_ACCOUNT_NOT_FOUND" | "USER_NOT_FOUND" => "invalid_credentials",
        "INVALID_PASSWORD" => "wrong_password",
        "EMAIL_NOT_VERIFIED" => "email_not_verified",
        "USER_ALREADY_EXISTS" | "USER_ALREADY_EXISTS_USE_ANOTHER_EMAIL" => "email_taken",
        "INVALID_EMAIL" => "invalid_email",
        "PASSWORD_TOO_SHORT" => "password_too_short",
        "PASSWORD_TOO_LONG" => "password_too_long",
        "INVALID_OTP" | "INVALID_TOKEN" => "invalid_code",
        "OTP_EXPIRED" | "TOKEN_EXPIRED" => "code_expired",
        "TOO_MANY_ATTEMPTS" => "too_many_attempts",
        "SESSION_EXPIRED" | "SESSION_NOT_FRESH" => "reauth",
        _ if status == 429 => "too_many",
        _ if status == 404 => "unavailable",
        _ if status == 422 => "bad_input",
        _ => "server",
    })
}

/// The Data API's error (PostgREST's `{code, message, details, hint}`) as ours: a function's
/// own code raised as its message (`RAISE EXCEPTION 'too_many'`), else by the SQLSTATE.
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
