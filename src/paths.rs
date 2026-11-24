use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const BUNDLE_ID: &str = "com.jeffujioka.butterscroll";

/// `$HOME/Library/Application Support/butterscroll`.
pub fn app_support_dir() -> Option<PathBuf> {
    under_home(std::env::var_os("HOME").as_deref(), "Library/Application Support/butterscroll")
}

/// `$HOME/Library/Logs/butterscroll.log`: stderr of the running app, also the LaunchAgent's StandardErrorPath.
pub fn log_file() -> Option<PathBuf> {
    under_home(std::env::var_os("HOME").as_deref(), "Library/Logs/butterscroll.log")
}

/// `$HOME/Library/Logs/butterscroll.old.log`: the previous log, kept once the current one is rotated away.
pub fn old_log_file() -> Option<PathBuf> {
    under_home(std::env::var_os("HOME").as_deref(), "Library/Logs/butterscroll.old.log")
}

/// `$HOME/Library/Application Support/butterscroll/butterscroll.lock`: held by the running menu-bar app.
pub fn main_lock() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("butterscroll.lock"))
}

/// `$HOME/Library/Application Support/butterscroll/settings.lock`: held by an open settings window.
pub fn settings_lock() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("settings.lock"))
}

fn under_home(home: Option<&OsStr>, rel: &str) -> Option<PathBuf> {
    home.filter(|h| !h.is_empty()).map(|h| Path::new(h).join(rel))
}

/// `X.app` for an executable at `X.app/Contents/MacOS/<name>`, otherwise `None`.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    if macos.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
        return None;
    }
    if bundle.extension()? != "app" {
        return None;
    }
    Some(bundle.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn bundle_of_finds_app_root() {
        assert_eq!(
            bundle_of(Path::new("/Applications/butterscroll.app/Contents/MacOS/butterscroll")),
            Some(PathBuf::from("/Applications/butterscroll.app"))
        );
        assert_eq!(bundle_of(Path::new("/x/target/release/butterscroll")), None);
        assert_eq!(bundle_of(Path::new("/x/butterscroll.app/Contents/Resources/butterscroll")), None);
    }
}
