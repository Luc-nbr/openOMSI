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

/// The statement's Markdown, its line ends as "\n", without its HTML comments (the version
/// marker, the maintainer's notes): what a player reads.
pub fn text() -> String {
    without_comments(&RAW.replace("\r\n", "\n")).trim().to_string()
}

/// `md` without its `<!-- … -->` comments, over several lines too (one never closed runs to the
/// end). The blank line a comment leaves only parts blocks, as any blank line does.
pub fn without_comments(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(k) = rest.find("<!--") {
        out.push_str(&rest[..k]);
        rest = rest[k..].find("-->").map_or("", |e| &rest[k + e + 3..]);
    }
    out.push_str(rest);
    out
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

    /// What a player reads: no "\r", no comment - neither the marker nor the maintainer's note.
    #[test]
    fn the_text_has_plain_line_ends_and_no_comments() {
        let t = text();
        assert!(!t.contains('\r'));
        assert!(!t.contains("privacy-version") && !t.contains("<!--") && !t.contains("-->"));
        assert!(!t.to_lowercase().contains("maintainer"), "a note for the maintainer is shown");
        assert!(t.starts_with("# "));
        assert_eq!(without_comments("a <!-- x -->b\n<!-- one\ntwo -->\nc<!-- open"), "a b\n\nc");
    }
}
