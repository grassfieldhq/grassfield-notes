//! Careful file access for content the app does not control: synced repositories,
//! pictures referenced by Markdown files, attachments.
//!
//! Every open here refuses anything that is not a regular file (named pipes, devices,
//! sockets, folders), checks it *after* opening (no gap between check and use) and caps
//! how much is read. Opening never blocks on a named pipe.

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

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

/// A path written in a document (a picture or a link target), resolved inside the
/// document's folder `base` only: absolute paths and ".." are refused, and once links are
/// resolved the real path must still be inside `base`. Returns that real path.
pub fn confine(base: &Path, written: &str) -> Option<PathBuf> {
    let decoded = written.replace("%20", " ");
    let rel = Path::new(&decoded);
    if decoded.is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
        return None;
    }
    let root = base.canonicalize().ok()?;
    let real = root.join(rel).canonicalize().ok()?;
    real.starts_with(&root).then_some(real)
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

/// How `save_document` wrote a file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Saved {
    /// a new file renamed over the old one, with its mode, owner, group and attributes
    Replaced,
    /// written into the existing file, after a backup of its old content
    InPlace,
}

/// Copy the extended attributes of `from` to `to` (on Linux they include POSIX ACLs).
/// Returns false when one that matters (user attributes, ACLs) could not be copied;
/// security.* and trusted.* attributes need privileges and are left to the system.
#[cfg(target_os = "linux")]
fn copy_xattrs(from: &File, to: &File) -> bool {
    use std::os::unix::io::AsRawFd;
    let (src, dst) = (from.as_raw_fd(), to.as_raw_fd());
    // SAFETY: plain system calls on open descriptors, with buffers of the size they report
    unsafe {
        let len = libc::flistxattr(src, std::ptr::null_mut(), 0);
        if len <= 0 {
            return len == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOTSUP);
        }
        let mut names = vec![0u8; len as usize];
        let len = libc::flistxattr(src, names.as_mut_ptr() as *mut libc::c_char, names.len());
        if len < 0 {
            return false;
        }
        for name in names[..len as usize].split(|b| *b == 0).filter(|n| !n.is_empty()) {
            let Ok(cname) = std::ffi::CString::new(name) else { return false };
            let matters = !(name.starts_with(b"security.") || name.starts_with(b"trusted."));
            let size = libc::fgetxattr(src, cname.as_ptr(), std::ptr::null_mut(), 0);
            if size < 0 {
                if matters {
                    return false;
                }
                continue;
            }
            let mut value = vec![0u8; size as usize];
            let size = libc::fgetxattr(src, cname.as_ptr(), value.as_mut_ptr() as *mut libc::c_void, value.len());
            let ok = size >= 0 && libc::fsetxattr(dst, cname.as_ptr(), value.as_ptr() as *const libc::c_void, size as usize, 0) == 0;
            if !ok && matters {
                return false;
            }
        }
    }
    true
}

/// Save a document the user opened (a Markdown file anywhere on the computer).
///
/// Normally: a temporary file in the same folder gets the new text, the old file's mode,
/// owner and group, and (on Linux) its extended attributes and ACLs; it is flushed and
/// renamed over the old file, so a crash never leaves a half-written file.
///
/// When that cannot keep the file as it was (the folder is not writable so no temporary
/// file can be made there, or the owner, group or attributes cannot be given to a new
/// file), the text is written into the existing file instead, after its old content is
/// copied into `backups` (a private folder); the copy is removed once the new text is
/// safely on disk, and kept (and named in the error) if writing fails.
///
/// `path` must be a regular file or missing: the caller refuses links.
pub fn save_document(path: &Path, bytes: &[u8], backups: &Path) -> std::io::Result<Saved> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => m,
        Ok(_) => return Err(invalid("not a regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return write_atomic(path, bytes, 0o644).map(|_| Saved::Replaced);
        }
        Err(e) => return Err(e),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mode = meta.mode() & 0o7777;
        let replaced = (|| -> std::io::Result<bool> {
            let (tmp, mut f) = match create_temp_beside(path, 0o600) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(false),
                Err(e) => return Err(e),
            };
            let keep = (|| -> std::io::Result<bool> {
                std::io::Write::write_all(&mut f, bytes)?;
                use std::os::unix::io::AsRawFd;
                // SAFETY: fchown on our own open descriptor
                let owned = unsafe { libc::fchown(f.as_raw_fd(), meta.uid(), meta.gid()) } == 0;
                if !owned {
                    return Ok(false);
                }
                #[cfg(target_os = "linux")]
                {
                    let old = open_regular(path, Links::Refuse)?;
                    if !copy_xattrs(&old, &f) {
                        return Ok(false);
                    }
                }
                use std::os::unix::fs::PermissionsExt;
                f.set_permissions(std::fs::Permissions::from_mode(mode))?;
                f.sync_all()?;
                Ok(true)
            })();
            match keep {
                Ok(true) => {
                    drop(f);
                    if let Err(e) = std::fs::rename(&tmp, path) {
                        let _ = std::fs::remove_file(&tmp);
                        return Err(e);
                    }
                    if let Some(dir) = path.parent() {
                        if let Ok(d) = File::open(dir) {
                            let _ = d.sync_all();
                        }
                    }
                    Ok(true)
                }
                other => {
                    let _ = std::fs::remove_file(&tmp);
                    other
                }
            }
        })()?;
        if replaced {
            return Ok(Saved::Replaced);
        }
        write_in_place(path, &meta, bytes, backups)
    }
    #[cfg(not(unix))]
    {
        let _ = (meta, backups);
        write_atomic(path, bytes, 0o644).map(|_| Saved::Replaced)
    }
}

/// Write into the existing file (same inode, so owner, group, ACLs and attributes stay),
/// after a private backup of its old content.
#[cfg(unix)]
fn write_in_place(path: &Path, meta: &std::fs::Metadata, bytes: &[u8], backups: &Path) -> std::io::Result<Saved> {
    use std::io::{Seek, Write};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(backups)?;
    if !is_real_dir(backups) {
        return Err(invalid("backups is not a folder"));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let backup = backups.join(format!("{name}.{}-{seq}.bak", std::process::id()));
    let old = read_regular(path, Links::Refuse, 1 << 30)?;
    write_new(&backup, &old, 0o600)?;
    let done = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(path)?;
        let now = f.metadata()?;
        // still the same regular file that was checked
        if !now.is_file() || now.ino() != meta.ino() || now.dev() != meta.dev() {
            return Err(invalid("the file changed while saving"));
        }
        f.seek(std::io::SeekFrom::Start(0))?;
        f.write_all(bytes)?;
        f.set_len(bytes.len() as u64)?;
        f.sync_all()
    })();
    match done {
        Ok(()) => {
            let _ = std::fs::remove_file(&backup);
            Ok(Saved::InPlace)
        }
        Err(e) => Err(std::io::Error::new(e.kind(), format!("{e} (old content kept in {})", backup.display()))),
    }
}
