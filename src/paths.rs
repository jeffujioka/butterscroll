use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const BUNDLE_ID: &str = "com.jeffujioka.mfx";

/// `$HOME/Library/Application Support/mfx`.
pub fn app_support_dir() -> Option<PathBuf> {
    under_home(std::env::var_os("HOME").as_deref(), "Library/Application Support/mfx")
}

/// `$HOME/Library/Logs/mfx.log`: stderr of the running app, also the LaunchAgent's StandardErrorPath.
pub fn log_file() -> Option<PathBuf> {
    under_home(std::env::var_os("HOME").as_deref(), "Library/Logs/mfx.log")
}

/// `$HOME/Library/Logs/mfx.old.log`: the previous log, kept once the current one is rotated away.
pub fn old_log_file() -> Option<PathBuf> {
    under_home(std::env::var_os("HOME").as_deref(), "Library/Logs/mfx.old.log")
}

/// `$HOME/Library/Application Support/mfx/mfx.lock`: held by the running menu-bar app.
pub fn main_lock() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("mfx.lock"))
}

/// `$HOME/Library/Application Support/mfx/settings.lock`: held by an open settings window.
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
            bundle_of(Path::new("/Applications/mfx.app/Contents/MacOS/mfx")),
            Some(PathBuf::from("/Applications/mfx.app"))
        );
        assert_eq!(bundle_of(Path::new("/x/target/release/mfx")), None);
        assert_eq!(bundle_of(Path::new("/x/mfx.app/Contents/Resources/mfx")), None);
    }
}
