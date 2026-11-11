//! Letting go of a video's page cache when nobody is watching it any more.
//!
//! Reading a file leaves its data in the kernel's page cache. That memory is
//! free to reclaim, but in a container it is charged to the container, so
//! after a 2 GB episode the Docker memory number (Unraid's Docker tab, `docker
//! stats`) stays hundreds of MiB above idle until the kernel gets round to
//! reclaiming it, which can take hours on a machine with plenty of RAM.
//! When a playback ends we tell the kernel we're done with the file, and the
//! number goes back to idle straight away.
//!
//! Only Linux has a way to ask for this (`posix_fadvise`); elsewhere it does
//! nothing. Data another program is using stays where it is: the kernel only
//! drops clean pages, and anything that reads them again just reads the disk.

use std::path::PathBuf;

/// Drop `path`'s cached pages, in the background. Errors are ignored: this is a hint.
pub fn release(path: impl Into<PathBuf>) {
    let path = path.into();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = drop_cache(&path) {
            tracing::debug!(path = %path.display(), error = %e, "couldn't release the page cache");
        }
    });
}

#[cfg(target_os = "linux")]
fn drop_cache(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let file = std::fs::File::open(path)?;
    // SAFETY: the descriptor is open for the whole call; offset 0 and length 0 mean the whole file.
    let rc = unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(rc))
    }
}

#[cfg(not(target_os = "linux"))]
fn drop_cache(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
