//! An openOMSI version as its releases name it: `MAJOR.MINOR.COMMIT` (docs/VERSIONING.md), read
//! from whatever says it - `openomsi --version` ("openomsi 0.2.1234"), a release archive's name
//! ("openOMSI-0.2.1234-windows-x64"), the Windows executable's ProductVersion, the macOS bundle's
//! CFBundleShortVersionString - with a pull request's test build's tail ("-pr1192") kept apart.

use std::cmp::Ordering;
use std::fmt;

/// A release version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GameVersion {
    pub major: u32,
    pub minor: u32,
    /// The commits since `VERSION` last changed (0 when a text gives only `MAJOR.MINOR`).
    pub commit: u32,
    /// A test build's pull request (`0.1.1313-pr1192`): None for a release.
    pub pr: Option<u32>,
}

impl GameVersion {
    pub const fn new(major: u32, minor: u32, commit: u32) -> GameVersion {
        GameVersion { major, minor, commit, pr: None }
    }

    /// The first version in `text`: `0.2.1234`, `v0.3.5`, `0.2` - a word of digits and dots,
    /// at least two numbers, not part of a longer word ("x64" or a date are none).
    pub fn find(text: &str) -> Option<GameVersion> {
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let starts = b[i].is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'.') || (b[i - 1] == b'v' && (i == 1 || !b[i - 2].is_ascii_alphanumeric())));
            if !starts {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            let word = text[i..j].trim_end_matches('.');
            let parts: Vec<&str> = word.split('.').collect();
            if (2..=3).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty() && p.len() <= 9) {
                let n = |k: usize| parts.get(k).map_or(Some(0), |p| p.parse::<u32>().ok());
                if let (Some(major), Some(minor), Some(commit)) = (n(0), n(1), n(2)) {
                    // (a test build's "-pr1192" right after it)
                    let rest = &text[j..];
                    let pr = rest.strip_prefix("-pr").map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>()).and_then(|d| d.parse().ok());
                    return Some(GameVersion { major, minor, commit, pr });
                }
            }
            i = j.max(i + 1);
        }
        None
    }
}

impl Ord for GameVersion {
    fn cmp(&self, o: &GameVersion) -> Ordering {
        (self.major, self.minor, self.commit).cmp(&(o.major, o.minor, o.commit))
    }
}

impl PartialOrd for GameVersion {
    fn partial_cmp(&self, o: &GameVersion) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl fmt::Display for GameVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.commit)?;
        if let Some(pr) = self.pr {
            write!(f, "-pr{pr}")?;
        }
        Ok(())
    }
}

/// As its text ("0.2.1234"), for the launcher's page.
impl serde::Serialize for GameVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_found_in_what_says_them() {
        assert_eq!(GameVersion::find("openomsi 0.2.1234\n"), Some(GameVersion::new(0, 2, 1234)));
        assert_eq!(GameVersion::find("openOMSI-0.3.17-windows-x64"), Some(GameVersion::new(0, 3, 17)));
        assert_eq!(GameVersion::find("v0.3.5"), Some(GameVersion::new(0, 3, 5)));
        assert_eq!(GameVersion::find("0.2"), Some(GameVersion::new(0, 2, 0)));
        let pr = GameVersion::find("0.1.1313-pr1192").unwrap();
        assert_eq!((pr.commit, pr.pr), (1313, Some(1192)));
        assert_eq!(pr.to_string(), "0.1.1313-pr1192");
        // (an architecture, a lone number or a four-part number are no version)
        assert_eq!(GameVersion::find("openOMSI-windows-x64"), None);
        assert_eq!(GameVersion::find("build 17"), None);
        assert_eq!(GameVersion::find("1.2.3.4"), None);
        assert_eq!(GameVersion::find(""), None);
    }

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(GameVersion::new(0, 2, 999) < GameVersion::new(0, 3, 0));
        assert!(GameVersion::new(0, 10, 0) > GameVersion::new(0, 9, 50));
        let mut pr = GameVersion::new(0, 2, 5);
        pr.pr = Some(7);
        assert_eq!(pr.cmp(&GameVersion::new(0, 2, 5)), Ordering::Equal);
    }
}
