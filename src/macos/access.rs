//! Access decisions: which Settings link grants access, and when to open Settings.

// Unused until the menu app and Settings window wire this in (Task 5).
#![allow(dead_code)]

use std::ffi::CStr;

/// Settings pane for the privacy override (macOS 27.3 and later).
pub const OVERRIDE_URL: &str =
    "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension.privacy-autonomous-control";
/// Settings pane for Accessibility (before macOS 27.3).
pub const ACCESSIBILITY_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/// Consecutive ticks with access before a failure counts as new.
pub const RECOVERY_TICKS: u32 = 10;

/// Major and minor from a `kern.osproductversion` string; a missing minor is 0.
pub fn parse_os_version(s: &str) -> Option<(u32, u32)> {
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let mut parts = s[..end].split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|m| m.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

/// The link for `os`; an unknown version gets the override link.
pub fn grant_url_for(os: Option<(u32, u32)>) -> &'static str {
    match os {
        Some(v) if v < (27, 3) => ACCESSIBILITY_URL,
        _ => OVERRIDE_URL,
    }
}

/// The link for the running macOS.
pub fn grant_url() -> &'static str {
    grant_url_for(os_product_version().as_deref().and_then(parse_os_version))
}

fn os_product_version() -> Option<String> {
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    // SAFETY: name is NUL-terminated; buf and len describe a valid writable buffer.
    let rc = unsafe {
        libc::sysctlbyname(
            c"kern.osproductversion".as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    let s = CStr::from_bytes_until_nul(&buf[..len.min(buf.len())]).ok()?;
    s.to_str().ok().map(str::to_owned)
}

/// Decides, once per tick, whether Settings should open on the Access pane.
pub struct AccessWatch {
    reported: bool,
    ticks_with_access: u32,
}

impl AccessWatch {
    pub fn new() -> Self {
        Self { reported: false, ticks_with_access: 0 }
    }

    /// True exactly when Settings should open.
    pub fn update(&mut self, has_access: bool) -> bool {
        if has_access {
            self.ticks_with_access = self.ticks_with_access.saturating_add(1);
            if self.ticks_with_access >= RECOVERY_TICKS {
                self.reported = false;
            }
            return false;
        }
        self.ticks_with_access = 0;
        let open = !self.reported;
        self.reported = true;
        open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_product_versions() {
        assert_eq!(parse_os_version("27.3"), Some((27, 3)));
        assert_eq!(parse_os_version("27.3.1"), Some((27, 3)));
        assert_eq!(parse_os_version("28"), Some((28, 0)));
        assert_eq!(parse_os_version("27.3 beta"), Some((27, 3)));
        assert_eq!(parse_os_version(""), None);
        assert_eq!(parse_os_version("abc"), None);
    }

    #[test]
    fn grant_url_by_version() {
        assert_eq!(grant_url_for(Some((26, 5))), ACCESSIBILITY_URL);
        assert_eq!(grant_url_for(Some((27, 2))), ACCESSIBILITY_URL);
        assert_eq!(grant_url_for(Some((27, 3))), OVERRIDE_URL);
        assert_eq!(grant_url_for(Some((27, 4))), OVERRIDE_URL);
        assert_eq!(grant_url_for(Some((28, 0))), OVERRIDE_URL);
    }

    #[test]
    fn unknown_version_uses_override_link() {
        assert_eq!(grant_url_for(None), OVERRIDE_URL);
    }

    #[test]
    fn missing_access_at_launch_opens_once() {
        let mut w = AccessWatch::new();
        assert!(w.update(false));
        assert!(!w.update(false));
        assert!(!w.update(false));
    }

    #[test]
    fn access_at_launch_never_opens() {
        let mut w = AccessWatch::new();
        for _ in 0..20 {
            assert!(!w.update(true));
        }
    }

    #[test]
    fn missing_access_opens_once_until_recovered() {
        let mut w = AccessWatch::new();
        for _ in 0..RECOVERY_TICKS {
            w.update(true);
        }
        assert!(w.update(false)); // the tap died
        assert!(!w.update(false)); // retries, same failure
        for _ in 0..RECOVERY_TICKS {
            assert!(!w.update(true));
        }
        assert!(w.update(false)); // a new failure after a real recovery
    }

    #[test]
    fn flapping_tap_opens_once() {
        let mut w = AccessWatch::new();
        assert!(w.update(false));
        for _ in 0..5 {
            for _ in 0..RECOVERY_TICKS - 1 {
                w.update(true);
            }
            assert!(!w.update(false)); // never stayed up long enough to count as recovered
        }
    }
}
