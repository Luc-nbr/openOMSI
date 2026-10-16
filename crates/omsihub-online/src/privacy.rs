//! The privacy statement (docs/omsihub/PRIVACY.md), built into the program: its text, and its
//! version read from the file's own marker (`<!-- privacy-version: N -->`), so the number lives in
//! one place. A signed-in player agrees to `VERSION` (`Client::accept_privacy`) before the account's
//! features do anything (`Client::privacy_needed`); a new version asks again.

/// The statement as the file has it (English, the binding text).
const RAW: &str = include_str!("../../../docs/omsihub/PRIVACY.md");

/// The statement's version: the file's marker (a file without one does not build).
pub const VERSION: u32 = version_of(RAW);

/// The number after `privacy-version:` in `text`, read while the program is built.
const fn version_of(text: &str) -> u32 {
    const KEY: &[u8] = b"privacy-version:";
    let b = text.as_bytes();
    let mut i = 0;
    while i + KEY.len() <= b.len() {
        let mut k = 0;
        while k < KEY.len() && b[i + k] == KEY[k] {
            k += 1;
        }
        if k == KEY.len() {
            let mut j = i + k;
            while j < b.len() && b[j] == b' ' {
                j += 1;
            }
            let mut n = 0u32;
            let mut digits = 0;
            while j < b.len() && b[j].is_ascii_digit() {
                n = n * 10 + (b[j] - b'0') as u32;
                j += 1;
                digits += 1;
            }
            assert!(digits > 0 && n > 0, "PRIVACY.md: the privacy-version marker has no version");
            return n;
        }
        i += 1;
    }
    panic!("PRIVACY.md has no privacy-version marker")
}

/// The statement's Markdown, its line ends as "\n" and without the version marker.
pub fn text() -> String {
    RAW.replace("\r\n", "\n").lines().filter(|l| !l.trim_start().starts_with("<!--")).collect::<Vec<_>>().join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version built in is the marker's in the file as it is now (read again here, apart from
    /// `version_of`): a stale build or a changed marker fails.
    #[test]
    fn the_version_is_the_files_marker() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/omsihub/PRIVACY.md");
        let file = std::fs::read_to_string(path).unwrap();
        let marker = file.lines().find_map(|l| l.trim().strip_prefix("<!-- privacy-version:")).expect("the marker");
        let n: u32 = marker.trim_end_matches("-->").trim().parse().expect("a number");
        assert_eq!(n, VERSION);
        assert!(VERSION >= 1);
        assert_eq!(version_of("x\r\n<!-- privacy-version: 12 -->\r\n# T"), 12);
    }

    #[test]
    fn the_text_has_plain_line_ends_and_no_marker() {
        let t = text();
        assert!(!t.contains('\r'));
        assert!(!t.contains("privacy-version"));
        assert!(t.starts_with("# "));
    }
}
