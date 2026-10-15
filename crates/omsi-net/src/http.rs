//! Plain HTTP for the launchers beside the game (Omsi-Hub's "Play together" of the bus company:
//! a friend's launcher talks to the boss's over the LAN, a VPN or the boss's Cloudflare tunnel):
//! a JSON body posted and the answer's text, and random bytes for a secret. The launcher's
//! crates take the HTTP client and the system's randomness from here, so that they need no
//! crates of their own for it.

use std::time::Duration;

/// Post `body` (JSON) to `url` (`http://…` or `https://…`) and give the answer's text, within
/// `timeout`. An answer that is not 200 is an error with its status.
pub fn post_json(url: &str, body: &str, timeout: Duration) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new().timeout(timeout).user_agent("openOMSI").build();
    match agent.post(url).set("Content-Type", "application/json").send_string(body) {
        Ok(r) => r.into_string().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code}")),
        Err(e) => Err(e.to_string()),
    }
}

/// `N` bytes from the system's secure random source (a session's secret).
pub fn random_bytes<const N: usize>() -> [u8; N] {
    use ring::rand::SecureRandom;
    let mut out = [0u8; N];
    if ring::rand::SystemRandom::new().fill(&mut out).is_err() {
        // (no secure source: the hasher's random keys and the clock, as the session ids)
        for chunk in out.chunks_mut(6) {
            let v = crate::random_session_id().to_be_bytes();
            chunk.copy_from_slice(&v[2..2 + chunk.len()]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn random_bytes_differ() {
        let a: [u8; 16] = super::random_bytes();
        let b: [u8; 16] = super::random_bytes();
        assert_ne!(a, b);
    }
}
