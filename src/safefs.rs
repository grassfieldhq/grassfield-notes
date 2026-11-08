//! Careful file access for content the app does not control: synced repositories,
//! pictures referenced by Markdown files, attachments.
//!
//! Every open here refuses anything that is not a regular file (named pipes, devices,
//! sockets, folders), checks it *after* opening (no gap between check and use) and caps
//! how much is read. Opening never blocks on a named pipe.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// How a path's last component is treated when it is a symbolic link.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Links {
    /// Follow it (the user picked this file themselves).
    Follow,
    /// Refuse it (content from elsewhere must never point outside its folder).
    Refuse,
}

fn invalid(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, msg.to_string())
}

/// Open a regular file for reading.
pub fn open_regular(path: &Path, links: Links) -> std::io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NONBLOCK: opening a named pipe would otherwise wait for a writer forever
        let mut flags = libc::O_NONBLOCK | libc::O_CLOEXEC;
        if links == Links::Refuse {
            flags |= libc::O_NOFOLLOW;
        }
        opts.custom_flags(flags);
    }
    #[cfg(not(unix))]
    if links == Links::Refuse && std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(invalid("symbolic links are not accepted here"));
    }
    let f = opts.open(path)?;
    if !f.metadata()?.is_file() {
        return Err(invalid("not a regular file"));
    }
    Ok(f)
}

/// Read a whole regular file, refusing files larger than `max` bytes.
pub fn read_regular(path: &Path, links: Links, max: u64) -> std::io::Result<Vec<u8>> {
    let f = open_regular(path, links)?;
    if f.metadata()?.len() > max {
        return Err(invalid("file too large"));
    }
    let mut out = Vec::new();
    // the file may grow after the check: never read more than the cap
    f.take(max.saturating_add(1)).read_to_end(&mut out)?;
    if out.len() as u64 > max {
        return Err(invalid("file too large"));
    }
    Ok(out)
}
