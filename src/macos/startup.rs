//! Process startup helpers: canonical re-exec and the stderr log file.

use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

/// The log is rotated to `butterscroll.old.log` once it grows past this size.
pub const LOG_MAX_BYTES: u64 = 1_048_576;

/// Re-executes the canonical path of this binary when it was started through a
/// symlink, so macOS sees the real bundle/binary. Returns only when no re-exec happened.
pub fn reexec_if_symlinked() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("locating butterscroll binary: {e}"))?;
    let canonical = exe.canonicalize().map_err(|e| format!("resolving {}: {e}", exe.display()))?;
    if canonical == exe {
        return Ok(());
    }
    let err = Command::new(&canonical).args(std::env::args_os().skip(1)).exec();
    Err(format!("re-executing {}: {err}", canonical.display()))
}

/// Moves `log` to `old` (replacing it) when `log` is larger than `max` bytes.
/// Returns whether a rotation happened; a missing `log` is not an error.
pub fn rotate_if_larger(log: &Path, old: &Path, max: u64) -> io::Result<bool> {
    let len = match fs::metadata(log) {
        Ok(meta) => meta.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if len <= max {
        return Ok(false);
    }
    fs::rename(log, old)?;
    Ok(true)
}

/// Appends stderr (fd 2) to `log` for the rest of the process.
pub fn redirect_stderr(log: &Path) -> io::Result<()> {
    let file = OpenOptions::new().create(true).append(true).open(log)?;
    if unsafe { libc::dup2(file.as_raw_fd(), 2) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Unique per test name and process, so parallel tests never share files.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("butterscroll-startup-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rotates_only_above_limit() {
        let dir = scratch("rotates");
        let log = dir.join("butterscroll.log");
        let old = dir.join("butterscroll.old.log");

        // Above the limit: the log moves to `old`, replacing an existing old file.
        fs::write(&log, b"0123456789").unwrap();
        fs::write(&old, b"stale").unwrap();
        assert!(rotate_if_larger(&log, &old, 5).unwrap());
        assert!(!log.exists());
        assert_eq!(fs::read(&old).unwrap(), b"0123456789");

        // At or below the limit: untouched.
        fs::write(&log, b"abc").unwrap();
        assert!(!rotate_if_larger(&log, &old, 5).unwrap());
        assert_eq!(fs::read(&log).unwrap(), b"abc");

        // Missing log: nothing to rotate.
        let missing = dir.join("missing.log");
        assert!(!rotate_if_larger(&missing, &old, 5).unwrap());
    }
}
