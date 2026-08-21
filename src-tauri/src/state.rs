//! What the app holds onto between commands.

use crate::error::{Result, SutraError};
use crate::vault::Vault;
use crate::watcher::VaultWatcher;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Remembered between launches, in the OS app-config directory — *not* in the
/// vault. The vault holds notes; which vault was last open is this machine's
/// business, and putting it in the vault would sync it between machines that
/// have the folder at different paths.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    vault: Option<PathBuf>,
}

/// The one piece of shared mutable state.
///
/// `Mutex` because Tauri runs commands on a thread pool, so two commands can
/// land at once. Rust will not let us share a `&mut` across threads, and that
/// is not pedantry here: two saves racing on the same note is a real thing.
/// The lock makes the race impossible rather than unlikely.
#[derive(Default)]
pub struct AppState {
    inner: Mutex<Option<Open>>,
}

/// An open vault and the watcher tied to it. They live and die together: when
/// this is replaced, the old watcher is dropped and its thread stops.
struct Open {
    vault: Vault,
    _watcher: Option<VaultWatcher>,
}

impl AppState {
    /// Run `f` against the open vault.
    ///
    /// Commands take this shape rather than handing out a `&Vault` because the
    /// borrow must not outlive the lock. Passing a closure lets the guard drop
    /// at the end of this function, which the borrow checker enforces for us.
    pub fn with_vault<T>(&self, f: impl FnOnce(&Vault) -> Result<T>) -> Result<T> {
        // A poisoned mutex means another thread panicked while holding it.
        // Recover rather than propagate: the vault is plain data, and refusing
        // to open a notes app because an unrelated command panicked is worse
        // than carrying on.
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let open = guard.as_ref().ok_or(SutraError::NoVault)?;
        f(&open.vault)
    }

    /// Open a directory as the current vault and start watching it.
    pub fn open_vault(&self, app: &AppHandle, root: PathBuf) -> Result<String> {
        let vault = Vault::open(root.clone())?;
        let name = vault.display_name();

        // A watcher that fails to start is not fatal — the app works, it just
        // will not notice external edits. Better degraded than dead.
        let watcher = match crate::watcher::watch(app.clone(), &root) {
            Ok(w) => Some(w),
            Err(e) => {
                eprintln!("sutra: could not watch the vault, external edits will be missed: {e}");
                None
            }
        };

        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(Open {
            vault,
            _watcher: watcher,
        });
        drop(guard);

        save_config(app, &Config { vault: Some(root) });
        Ok(name)
    }

    /// The open vault's display name, if any.
    pub fn vault_name(&self) -> Option<String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|o| o.vault.display_name())
    }
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("sutra.json"))
}

/// Best-effort persistence. A machine where the config directory is not
/// writable should still run; it just forgets the vault between launches.
fn save_config(app: &AppHandle, config: &Config) {
    let Some(path) = config_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, json);
    }
}

/// Reopen the vault from the last session, if it is still there.
///
/// A vault that has been moved or deleted is simply not reopened — the user
/// gets the "choose a vault" screen rather than an error about a path they may
/// not remember choosing.
pub fn restore_last_vault(app: &AppHandle, state: &AppState) {
    let Some(path) = config_path(app) else { return };
    let Ok(json) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(config) = serde_json::from_str::<Config>(&json) else {
        return;
    };
    let Some(root) = config.vault else { return };
    if !Path::new(&root).is_dir() {
        return;
    }
    let _ = state.open_vault(app, root);
}
