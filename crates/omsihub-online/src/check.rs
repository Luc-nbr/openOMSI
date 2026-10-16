//! What the player types, checked before it is sent (the server checks it again): the same
//! limits as Neon Auth's (a password of 8 to 128 characters) and the tables' (a display name of
//! 3 to 24, a friend code of 8). Each answers the text as it is sent, or the code saying why not.

use crate::codes::Error;

/// An email address: trimmed, at most 254 characters, one `@` with something on both sides and
/// a dot after it, no spaces.
pub fn email(s: &str) -> Result<String, Error> {
    let s = s.trim();
    let ok = s.len() <= 254
        && !s.chars().any(|c| c.is_whitespace() || c.is_control())
        && s.split_once('@').is_some_and(|(user, host)| !user.is_empty() && user.len() <= 64 && !host.contains('@') && host.contains('.') && !host.starts_with('.') && !host.ends_with('.'));
    ok.then(|| s.to_string()).ok_or(Error::Code("invalid_email"))
}

/// A password: 8 to 128 characters, as it was typed.
pub fn password(s: &str) -> Result<String, Error> {
    match s.chars().count() {
        0..=7 => Err(Error::Code("password_too_short")),
        129.. => Err(Error::Code("password_too_long")),
        _ => Ok(s.to_string()),
    }
}

/// A display name: trimmed, 3 to 24 characters, no control characters.
pub fn display_name(s: &str) -> Result<String, Error> {
    let s = s.trim();
    let n = s.chars().count();
    ((3..=24).contains(&n) && !s.chars().any(char::is_control)).then(|| s.to_string()).ok_or(Error::Code("bad_name"))
}

/// A friend code: 8 letters and digits (the spaces and dashes of one read out dropped).
pub fn friend_code(s: &str) -> Result<String, Error> {
    let s: String = s.chars().filter(|c| !matches!(c, ' ' | '-' | '\t')).collect();
    (s.len() == 8 && s.chars().all(|c| c.is_ascii_alphanumeric())).then_some(s).ok_or(Error::Code("bad_friend_code"))
}

/// A code from an email: 4 to 10 digits (Neon Auth sends 6), the spaces dropped.
pub fn email_code(s: &str) -> Result<String, Error> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    ((4..=10).contains(&s.len()) && s.chars().all(|c| c.is_ascii_digit())).then_some(s).ok_or(Error::Code("invalid_code"))
}

/// A company session's code to pass on (`together`'s, at most a few hundred characters).
pub fn session_code(s: &str) -> Result<String, Error> {
    let s = s.trim();
    (!s.is_empty() && s.len() <= 1024 && s.chars().all(|c| c.is_ascii_graphic())).then(|| s.to_string()).ok_or(Error::Code("bad_input"))
}

/// One of the online states the server knows.
pub fn status(s: &str) -> Result<&'static str, Error> {
    ["online", "away", "busy", "offline"].into_iter().find(|x| *x == s).ok_or(Error::Code("bad_input"))
}

/// An id the server gave (a user's, an invite's): short, no spaces.
pub fn id(s: &str) -> Result<String, Error> {
    let s = s.trim();
    (!s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))).then(|| s.to_string()).ok_or(Error::Code("bad_input"))
}

/// How strong a password looks (0 weak, 1 fair, 2 strong), for the hint beside the field.
pub fn strength(s: &str) -> u8 {
    let n = s.chars().count();
    let kinds = [s.chars().any(|c| c.is_lowercase()), s.chars().any(|c| c.is_uppercase()), s.chars().any(|c| c.is_ascii_digit()), s.chars().any(|c| !c.is_alphanumeric())].iter().filter(|x| **x).count();
    match (n, kinds) {
        (0..=7, _) => 0,
        (8..=11, 0..=2) => 0,
        (8..=11, _) | (12..=15, 0..=1) => 1,
        (12..=15, _) => 2,
        (_, 0..=1) => 1,
        _ => 2,
    }
}
