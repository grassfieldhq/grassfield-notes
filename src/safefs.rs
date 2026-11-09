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

/// True when `path` is a real folder (not a link to one).
pub fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

/// True when `path` resolves (links included) to somewhere inside the folder `root`,
/// which must already be canonical.
pub fn is_inside(root: &Path, path: &Path) -> bool {
    path.canonicalize().map(|p| p.starts_with(root)).unwrap_or(false)
}

/// Remove a file, a link or a whole folder. A link is removed itself, never followed.
pub fn remove_any(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Make sure `path` is a real folder: a link or a file in its place is removed first.
pub fn ensure_real_dir(path: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(path).is_ok() && !is_real_dir(path) {
        std::fs::remove_file(path)?;
    }
    match std::fs::create_dir(path) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(e),
        _ => {}
    }
    if is_real_dir(path) {
        Ok(())
    } else {
        Err(invalid("not a folder"))
    }
}

static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Create a new, empty temporary file next to `dst` (same folder, so a rename is atomic).
/// The file is created exclusively, so a link planted at that name is never followed.
fn create_temp_beside(dst: &Path, mode: u32) -> std::io::Result<(std::path::PathBuf, File)> {
    let dir = dst.parent().ok_or_else(|| invalid("no folder"))?;
    let name = dst.file_name().ok_or_else(|| invalid("no file name"))?.to_string_lossy();
    for _ in 0..100 {
        let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = dir.join(format!(".{name}.gf-{}-{seq}.tmp", std::process::id()));
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(mode).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        #[cfg(not(unix))]
        let _ = mode;
        match opts.open(&tmp) {
            Ok(f) => return Ok((tmp, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(invalid("no free temporary name"))
}

/// Replace `dst` with what `fill` writes, atomically: a temporary file in the same
/// folder is written, flushed to disk, then renamed over `dst`. A rename replaces a link
/// at `dst` instead of writing through it, and readers never see a half-written file.
pub fn write_atomic_with(
    dst: &Path,
    mode: u32,
    fill: impl FnOnce(&mut File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let (tmp, mut f) = create_temp_beside(dst, mode)?;
    let done = (|| {
        fill(&mut f)?;
        #[cfg(unix)]
        {
            // the umask may have narrowed the mode given at creation
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(mode))?;
        }
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, dst)
    })();
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    } else if let Some(dir) = dst.parent() {
        // make the rename itself durable (best effort; not possible on every system)
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
    done
}

/// Write a new file; fails if anything (a file or a link) already has that name.
pub fn write_new(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut f = opts.open(path)?;
    std::io::Write::write_all(&mut f, bytes)?;
    f.sync_all()
}

/// `write_atomic_with` for bytes already in memory.
pub fn write_atomic(dst: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    write_atomic_with(dst, mode, |f| std::io::Write::write_all(f, bytes))
}

/// Copy a regular file (at most `max` bytes) to `dst`, atomically.
pub fn copy_regular(src: &Path, links: Links, dst: &Path, max: u64, mode: u32) -> std::io::Result<()> {
    let from = open_regular(src, links)?;
    if from.metadata()?.len() > max {
        return Err(invalid("file too large"));
    }
    write_atomic_with(dst, mode, |f| {
        let copied = std::io::copy(&mut from.take(max.saturating_add(1)), f)?;
        if copied > max {
            return Err(invalid("file too large"));
        }
        Ok(())
    })
}
