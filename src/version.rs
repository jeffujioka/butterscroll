#![allow(dead_code)] // the parsed API is consumed by the update check in a later task
//! Semantic versions: parsing, SemVer 2.0 precedence and the crate's own version.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

/// The version this binary was built as (from `Cargo.toml`).
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl Version {
    /// Parses `MAJOR.MINOR.PATCH[-PRE]`. No leading `v`, no build metadata.
    pub fn parse(s: &str) -> Option<Version> {
        let (core, pre) = match s.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (s, None),
        };
        let mut parts = core.split('.');
        let major = number(parts.next()?)?;
        let minor = number(parts.next()?)?;
        let patch = number(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        let pre = match pre {
            Some(pre) if pre.split('.').all(valid_identifier) => Some(pre.to_owned()),
            Some(_) => return None,
            None => None,
        };
        Some(Version { major, minor, patch, pre })
    }

    /// Parses a release tag such as `v1.2.3`, stripping exactly one leading `v`.
    pub fn from_tag(tag: &str) -> Option<Version> {
        Version::parse(tag.strip_prefix('v')?)
    }

    pub fn current() -> Version {
        Version::parse(CURRENT).expect("CARGO_PKG_VERSION is a valid SemVer version")
    }

    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }
}

/// A numeric core component: digits only, no leading zeros, fits in `u64`.
fn number(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0')) {
        return None;
    }
    s.parse().ok()
}

fn valid_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn is_numeric(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit())
}

/// Compares one pre-release identifier pair per SemVer 2.0 §11.4.
fn compare_identifiers(a: &str, b: &str) -> Ordering {
    match (is_numeric(a), is_numeric(b)) {
        // Numeric identifiers compare as numbers; compare by length first to avoid overflow.
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                // A pre-release has lower precedence than the release it precedes.
                (None, None) => Ordering::Equal,
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(a), Some(b)) => {
                    let (mut a, mut b) = (a.split('.'), b.split('.'));
                    loop {
                        match (a.next(), b.next()) {
                            (None, None) => return Ordering::Equal,
                            // A shorter set of identifiers is lower when all shared ones are equal.
                            (None, Some(_)) => return Ordering::Less,
                            (Some(_), None) => return Ordering::Greater,
                            (Some(x), Some(y)) => match compare_identifiers(x, y) {
                                Ordering::Equal => continue,
                                other => return other,
                            },
                        }
                    }
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl FromStr for Version {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Version::parse(s).ok_or_else(|| format!("invalid version {s:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn parses_final_and_prerelease() {
        assert_eq!(Version::parse("0.2.0").unwrap().to_string(), "0.2.0");
        assert_eq!(Version::parse("0.2.0-rc.1").unwrap().pre.as_deref(), Some("rc.1"));
        assert!(Version::parse("0.2").is_none() && Version::parse("v0.2.0").is_none());
    }
    #[test] fn from_tag_strips_one_v() {
        assert_eq!(Version::from_tag("v1.2.3"), Version::parse("1.2.3"));
        assert!(Version::from_tag("vv1.2.3").is_none());
    }
    #[test] fn semver_precedence() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("0.2.0-rc.1") < v("0.2.0"));
        assert!(v("0.2.0-rc.2") < v("0.2.0-rc.10"));   // numeric identifiers compare as numbers
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0-alpha") < v("1.0.0-alpha.1"));
    }
    #[test] fn current_is_valid() { assert_eq!(Version::current().to_string(), CURRENT); }
}
