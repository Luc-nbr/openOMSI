//! The company's session code (`HUB-XXXX-XXXX-…`), the game's session code's sibling: where the
//! boss's launcher is (its port and up to three addresses, a gaming VPN's first, as
//! `omsi_net::code_ipv4s` gives them), the session (the fingerprint of its key: the tunnel's
//! address is posted under it on the game's relay, `omsi_net::bridge::post_tunnel`), the secret
//! a friend's launcher shows with every message, and which company it is (a short tag of its
//! id). In the game codes' alphabet, without look-alike characters, in groups of four.
//!
//! Layout (bytes): secret 8, session 6, version 1, port 2, company tag 2, addresses 4 each (0 to
//! 3), CRC-16/CCITT-FALSE 2 over all of it.

use std::net::{Ipv4Addr, SocketAddr};

/// The code alphabet (the game's: no I, O, 0, 1).
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// What every company code begins with.
pub const PREFIX: &str = "HUB-";
/// The version of the company session (raised when its messages change shape).
pub const VERSION: u8 = 3;
/// At most this many addresses go into a code.
pub const MAX_ADDRS: usize = 3;
/// The bytes besides the addresses.
const FIXED: usize = 8 + 6 + 1 + 2 + 2 + 2;

/// What a company code carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanyCode {
    pub secret: [u8; 8],
    /// The session: the fingerprint of the boss's key (48 bits).
    pub session: u64,
    pub version: u8,
    pub port: u16,
    /// The company's tag (`tag_of` its id).
    pub company: u16,
    /// The boss's addresses, best first (none: the tunnel alone).
    pub ips: Vec<Ipv4Addr>,
}

/// Why a text is not a company code (English: keys of the translations).
pub const NOT_A_CODE: &str = "This is not a company code. Ask the boss for the code of his company (HUB-…).";
pub const GAME_CODE: &str = "This is the code of a game session. Ask the boss for the code of his company (HUB-…): it is in his \"Play together\" panel.";
pub const TYPO: &str = "The company code has a typo: copy the whole code again.";
pub const NEWER: &str = "This code was made by another version of Omsi-Hub: both players need the same version.";

/// The company's tag: 16 bits of its id (FNV-1a), so a code of another company of the boss is
/// told apart.
pub fn tag_of(company: &str) -> u16 {
    let mut h: u32 = 0x811C_9DC5;
    for b in company.trim().to_ascii_lowercase().bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    (h ^ (h >> 16)) as u16
}

/// CRC-16/CCITT-FALSE (the game codes' checksum).
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// Characters a code of `bytes` bytes has (5 bits a character).
fn chars_for(bytes: usize) -> usize {
    (bytes * 8).div_ceil(5)
}

/// The lengths (characters after the prefix) a code has: with 0 to `MAX_ADDRS` addresses,
/// unpadded and with its last group filled to four.
fn lengths() -> Vec<(usize, usize)> {
    (0..=MAX_ADDRS).map(|n| chars_for(FIXED + 4 * n)).map(|l| (l, l.div_ceil(4) * 4)).collect()
}

impl CompanyCode {
    /// The secret as the messages carry it (hex).
    pub fn secret_hex(&self) -> String {
        self.secret.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Where the boss's launcher may be reached, best first.
    pub fn addrs(&self) -> Vec<SocketAddr> {
        self.ips.iter().map(|ip| SocketAddr::from((*ip, self.port))).collect()
    }

    pub fn encode(&self) -> String {
        let ips: Vec<Ipv4Addr> = self.ips.iter().copied().take(MAX_ADDRS).collect();
        let mut b = Vec::with_capacity(FIXED + 4 * ips.len());
        b.extend_from_slice(&self.secret);
        b.extend_from_slice(&(self.session & 0xFFFF_FFFF_FFFF).to_be_bytes()[2..8]);
        b.push(self.version);
        b.extend_from_slice(&self.port.to_be_bytes());
        b.extend_from_slice(&self.company.to_be_bytes());
        for ip in &ips {
            b.extend_from_slice(&ip.octets());
        }
        let crc = crc16(&b);
        b.extend_from_slice(&crc.to_be_bytes());
        let n = chars_for(b.len());
        let mut chars = Vec::with_capacity(n.div_ceil(4) * 4);
        for i in 0..n {
            let mut v = 0usize;
            for k in 0..5 {
                let bit = i * 5 + k;
                let x = b.get(bit / 8).map(|x| (x >> (7 - bit % 8)) & 1).unwrap_or(0);
                v = (v << 1) | x as usize;
            }
            chars.push(ALPHABET[v] as char);
        }
        while chars.len() % 4 != 0 {
            chars.push(ALPHABET[0] as char);
        }
        let groups: Vec<String> = chars.chunks(4).map(|c| c.iter().collect()).collect();
        format!("{PREFIX}{}", groups.join("-"))
    }

    /// Read a code as a person may paste it: any case, with or without the prefix, dashes and
    /// spaces anywhere. Err says what is wrong (English: a key of the translations).
    pub fn decode(text: &str) -> Result<CompanyCode, &'static str> {
        let t = text.trim().to_ascii_uppercase();
        match Self::read(&t) {
            // (the game's code pasted instead: said so)
            Err(_) if !t.starts_with("HUB") && (t.starts_with("OMSI") || omsi_net::looks_like_code(&t)) => Err(GAME_CODE),
            r => r,
        }
    }

    fn read(t: &str) -> Result<CompanyCode, &'static str> {
        let mut s: String = t.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let fits = |n: usize| lengths().iter().any(|&(a, b)| n == a || n == b);
        if s.starts_with("HUB") && fits(s.len() - 3) {
            s = s[3..].to_string();
        }
        if !fits(s.len()) {
            return Err(NOT_A_CODE);
        }
        // (the filling of the last group taken off)
        if let Some(&(a, _)) = lengths().iter().find(|&&(a, b)| s.len() == b && a < b) {
            if s.as_bytes()[a..].iter().all(|c| *c == ALPHABET[0]) {
                s.truncate(a);
            }
        }
        let mut bits: Vec<u8> = Vec::with_capacity(s.len() * 5);
        for c in s.bytes() {
            let v = ALPHABET.iter().position(|a| *a == c).ok_or(TYPO)?;
            for k in (0..5).rev() {
                bits.push(((v >> k) & 1) as u8);
            }
        }
        let n = bits.len() / 8;
        let b: Vec<u8> = (0..n).map(|i| bits[i * 8..i * 8 + 8].iter().fold(0u8, |a, x| (a << 1) | x)).collect();
        if n < FIXED || (n - FIXED) % 4 != 0 {
            return Err(NOT_A_CODE);
        }
        let body = &b[..n - 2];
        if crc16(body) != u16::from_be_bytes([b[n - 2], b[n - 1]]) {
            return Err(TYPO);
        }
        let mut secret = [0u8; 8];
        secret.copy_from_slice(&body[0..8]);
        let mut sid = [0u8; 8];
        sid[2..8].copy_from_slice(&body[8..14]);
        let version = body[14];
        if version != VERSION {
            return Err(NEWER);
        }
        let port = u16::from_be_bytes([body[15], body[16]]);
        let company = u16::from_be_bytes([body[17], body[18]]);
        let ips = body[19..].chunks_exact(4).map(|o| Ipv4Addr::new(o[0], o[1], o[2], o[3])).collect();
        Ok(CompanyCode { secret, session: u64::from_be_bytes(sid), version, port, company, ips })
    }
}

/// Whether `text` looks like a company code (the join field says so as it is typed).
pub fn looks_like(text: &str) -> bool {
    CompanyCode::decode(text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(n: usize) -> CompanyCode {
        let ips = [Ipv4Addr::new(25, 34, 223, 28), Ipv4Addr::new(192, 168, 1, 20), Ipv4Addr::new(100, 64, 3, 4)];
        CompanyCode { secret: [1, 2, 3, 4, 250, 6, 7, 8], session: 0xABCD_EF01_2345, version: VERSION, port: 27045, company: tag_of("connexion"), ips: ips[..n].to_vec() }
    }

    #[test]
    fn a_code_reads_back_whatever_way_it_is_pasted() {
        for n in 0..=MAX_ADDRS {
            let c = code(n);
            let text = c.encode();
            assert!(text.starts_with(PREFIX), "{text}");
            assert_eq!(CompanyCode::decode(&text), Ok(c.clone()));
            assert_eq!(CompanyCode::decode(&text.to_lowercase()), Ok(c.clone()));
            assert_eq!(CompanyCode::decode(&text.replace('-', " ")), Ok(c.clone()));
            assert_eq!(CompanyCode::decode(&text[PREFIX.len()..]), Ok(c.clone()));
            assert_eq!(CompanyCode::decode(&format!("  {text}\n")), Ok(c.clone()));
        }
    }

    #[test]
    fn a_bad_code_says_why() {
        let text = code(2).encode();
        // one character changed
        let mut b = text.clone().into_bytes();
        let at = PREFIX.len() + 5;
        b[at] = if b[at] == b'A' { b'B' } else { b'A' };
        assert_eq!(CompanyCode::decode(&String::from_utf8(b).unwrap()), Err(TYPO));
        // cut short, or something else entirely
        assert_eq!(CompanyCode::decode(&text[..text.len() - 6]), Err(NOT_A_CODE));
        assert_eq!(CompanyCode::decode("hello"), Err(NOT_A_CODE));
        // the game's code instead
        let game = omsi_net::SessionCode::single(7, Ipv4Addr::new(192, 168, 1, 20), 27015, 0x1234_5678_9ABC).encode();
        assert_eq!(CompanyCode::decode(&game), Err(GAME_CODE));
        // another version
        let mut other = code(1);
        other.version = VERSION + 1;
        assert_eq!(CompanyCode::decode(&other.encode()), Err(NEWER));
    }

    #[test]
    fn companies_have_tags_of_their_own() {
        assert_eq!(tag_of("connexion"), tag_of(" Connexion "));
        assert_ne!(tag_of("connexion"), tag_of("stadtbus"));
    }
}
