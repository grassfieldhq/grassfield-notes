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
