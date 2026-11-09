//! The GitHub access token lives in the system keyring (the Secret Service on Linux,
//! the Keychain on macOS, the Credential Manager on Windows), never in the notes folder.
//!
//! Every call here may wait for the keyring (for example while it asks to be unlocked),
//! so the window never calls them directly: `spawn` runs them on a background thread.

use std::sync::mpsc::{channel, Receiver};

const SERVICE: &str = "grassfield-notes";
const ACCOUNT: &str = "github-sync-token";

/// Make the platform keyring the default store, once. Tests always use an in-memory
/// store, so they never read or write the keyring of the person running them.
fn init_store() -> Result<(), String> {
    static INIT: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    INIT.get_or_init(|| {
        #[cfg(test)]
        let store = keyring_core::mock::Store::new();
        #[cfg(all(not(test), target_os = "linux"))]
        let store = zbus_secret_service_keyring_store::Store::new();
        #[cfg(all(not(test), target_os = "macos"))]
        let store = apple_native_keyring_store::keychain::Store::new();
        #[cfg(all(not(test), target_os = "windows"))]
        let store = windows_native_keyring_store::Store::new();
        #[cfg(any(test, target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            store.map(|s| keyring_core::set_default_store(s)).map_err(|e| e.to_string())
        }
        #[cfg(not(any(test, target_os = "linux", target_os = "macos", target_os = "windows")))]
        Err("no system keyring on this platform".to_string())
    })
    .clone()
}

fn entry() -> Result<keyring_core::Entry, String> {
    init_store()?;
    #[cfg(all(not(test), target_os = "linux"))]
    {
        // the name shown in the keyring app (Seahorse, KWalletManager…)
        let mods = std::collections::HashMap::from([("label", "Grassfield Notes: GitHub sync token")]);
        keyring_core::Entry::new_with_modifiers(SERVICE, ACCOUNT, &mods).map_err(|e| e.to_string())
    }
    #[cfg(any(test, not(target_os = "linux")))]
    keyring_core::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
}

/// The saved token, if there is one.
pub fn load() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn store(token: &str) -> Result<(), String> {
    entry()?.set_password(token).map_err(|e| e.to_string())
}

pub fn delete() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Run a keyring call on a background thread; the window polls the answer.
pub fn spawn<T: Send + 'static>(
    ctx: &eframe::egui::Context,
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Receiver<Result<T, String>> {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(job());
        ctx.request_repaint();
    });
    rx
}
