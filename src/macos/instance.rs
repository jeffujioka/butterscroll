//! Single running instance: an exclusive `flock` on a lock file. The kernel
//! releases the lock when the holder exits, even after a crash.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// Holds the exclusive lock; dropping it (or exiting) releases the lock.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

/// Takes the exclusive lock on `path` without blocking. `Ok(None)` means another
/// process holds it. Parent directories are created as needed.
pub fn try_acquire(path: &Path) -> io::Result<Option<InstanceLock>> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new().create(true).write(true).truncate(false).open(path)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(InstanceLock { _file: file }));
    }
    let err = io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(None)
    } else {
        Err(err)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Unique per call and process, so parallel tests never share a directory.
    fn unique_temp_dir() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("butterscroll-instance-{}-{n}", std::process::id()))
    }

    #[test]
    fn second_acquire_fails_until_first_dropped() {
        let p = unique_temp_dir().join("x.lock");
        let a = try_acquire(&p).unwrap();
        assert!(a.is_some());
        assert!(try_acquire(&p).unwrap().is_none());
        drop(a);
        assert!(try_acquire(&p).unwrap().is_some());
    }
}
