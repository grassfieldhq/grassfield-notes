//! The few things that differ between Linux, macOS and Windows.

use std::path::Path;
use std::process::{Command, Stdio};

/// Open a web address with the default browser, or a file or folder with its default app.
/// Runs in the background and never blocks the window.
pub fn open(target: &str) -> Result<(), String> {
    let target = target.strip_prefix("file://").unwrap_or(target).to_string();
    let mut cmd = opener(&target);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn open_path(path: &Path) -> Result<(), String> {
    open(&path.to_string_lossy())
}

/// Show a file in the file manager (its folder is opened) instead of opening the file
/// itself: used for files that should never be launched directly.
pub fn reveal(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg("-R").arg(path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("explorer.exe");
        c.arg(format!("/select,{}", path.display()));
        c
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut cmd = {
        let folder = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
        opener(&folder.to_string_lossy())
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// A web or mail address that may be opened (after the user saw it).
pub fn is_web_link(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t.starts_with("http://") || t.starts_with("https://") || t.starts_with("mailto:")
}

/// A link with a scheme ("ftp:", "smb:", "javascript:"…), not a path. Single letters are
/// Windows drive names ("C:"), which are paths.
pub fn has_scheme(target: &str) -> bool {
    match target.split_once(':') {
        Some((s, _)) => s.len() > 1 && s.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)),
        None => false,
    }
}

#[cfg(target_os = "windows")]
fn opener(target: &str) -> Command {
    // explorer.exe handles web addresses, files and folders, and never opens a console window
    let mut c = Command::new("explorer.exe");
    let is_url = target.contains("://") || target.starts_with("mailto:");
    c.arg(if is_url { target.to_string() } else { target.replace('/', "\\") });
    c
}

#[cfg(target_os = "macos")]
fn opener(target: &str) -> Command {
    let mut c = Command::new("open");
    c.arg(target);
    c
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn opener(target: &str) -> Command {
    let mut c = Command::new("xdg-open");
    c.arg(target);
    c
}

/// How the shortcut modifier is shown: ⌘ on macOS, Ctrl elsewhere.
pub fn shortcut_label(text: &str) -> std::borrow::Cow<'_, str> {
    if cfg!(target_os = "macos") && (text.contains("Ctrl") || text.contains("Strg")) {
        std::borrow::Cow::Owned(text.replace("Ctrl+", "⌘").replace("Strg+", "⌘"))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

/// A command for an external program (git, tesseract, pdftotext…).
/// On Windows it runs without flashing a console window over the app.
pub fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}
