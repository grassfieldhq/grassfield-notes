//! The system's own Open and Save panels, through the XDG desktop portal (FileChooser).
//! In Grassfield these are Grassfield's panels; elsewhere, those of the desktop in use.
//!
//! A panel runs on a background thread and the window polls its answer. When no portal
//! answers, the caller falls back to the built-in chooser (or the default export folder).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

pub enum Answer {
    Files(Vec<PathBuf>),
    Cancelled,
    /// no portal (or no FileChooser) on this system
    Unavailable,
}

/// What to ask for.
pub enum Ask {
    /// pick existing files, filtered by extension
    Open { multiple: bool, filter: (String, Vec<&'static str>) },
    /// pick a folder
    Folder,
    /// pick where to save, with a suggested name
    Save { name: String },
}

static UNAVAILABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Worth trying: a Linux desktop session, and no portal failure seen so far.
pub fn available() -> bool {
    cfg!(target_os = "linux")
        && !UNAVAILABLE.load(std::sync::atomic::Ordering::Relaxed)
        && std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
}

/// Show a panel; the answer arrives on the returned channel.
pub fn ask(ctx: &eframe::egui::Context, title: String, ask: Ask, folder: Option<PathBuf>) -> Receiver<Answer> {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let answer = run(&title, ask, folder.as_deref());
        if matches!(answer, Answer::Unavailable) {
            UNAVAILABLE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let _ = tx.send(answer);
        ctx.request_repaint();
    });
    rx
}

#[cfg(target_os = "linux")]
fn run(title: &str, ask: Ask, folder: Option<&Path>) -> Answer {
    use ashpd::desktop::file_chooser::{FileFilter, SelectedFiles};
    use ashpd::desktop::ResponseError;
    let folder = folder.filter(|f| f.is_dir());
    let result = async_io::block_on(async {
        let request = match ask {
            Ask::Open { multiple, filter } => {
                let mut f = FileFilter::new(&filter.0);
                for ext in &filter.1 {
                    f = f.glob(&format!("*.{ext}"));
                }
                SelectedFiles::open_file().title(title).modal(true).multiple(multiple).filter(f).current_folder::<&Path>(folder)?.send().await?
            }
            Ask::Folder => SelectedFiles::open_file().title(title).modal(true).directory(true).current_folder::<&Path>(folder)?.send().await?,
            Ask::Save { name } => SelectedFiles::save_file().title(title).modal(true).current_name(name.as_str()).current_folder::<&Path>(folder)?.send().await?,
        };
        request.response()
    });
    match result {
        Ok(files) => Answer::Files(files.uris().iter().filter_map(|u| file_uri_to_path(u.as_str())).collect()),
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => Answer::Cancelled,
        Err(_) => Answer::Unavailable,
    }
}

#[cfg(not(target_os = "linux"))]
fn run(_: &str, _: Ask, _: Option<&Path>) -> Answer {
    Answer::Unavailable
}

/// A local path from a `file://` URI (percent-decoded). Other URIs are refused.
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // "file:///path" (empty host) or "file://localhost/path"
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok())?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
    }
    #[cfg(not(unix))]
    String::from_utf8(out).ok().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_become_paths() {
        assert_eq!(file_uri_to_path("file:///home/a/Mes%20notes.md"), Some(PathBuf::from("/home/a/Mes notes.md")));
        assert_eq!(file_uri_to_path("file://localhost/tmp/x"), Some(PathBuf::from("/tmp/x")));
        assert_eq!(file_uri_to_path("file:///caf%C3%A9.md"), Some(PathBuf::from("/café.md")));
        assert_eq!(file_uri_to_path("https://example.org/x"), None);
        assert_eq!(file_uri_to_path("file://server/share/x"), None);
        assert_eq!(file_uri_to_path("file:///bad%2"), None);
    }
}
