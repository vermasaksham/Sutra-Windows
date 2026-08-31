//! What the app holds onto between commands.

use crate::ai::{self, Assistant as _, Off};
use crate::error::{Result, SutraError};
use crate::index::Index;
use crate::vault::Vault;
use crate::watcher::VaultWatcher;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

/// Remembered between launches, in the OS app-config directory — *not* in the
/// vault. The vault holds notes; which vault was last open is this machine's
/// business, and putting it in the vault would sync it between machines that
/// have the folder at different paths.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    vault: Option<PathBuf>,
    #[serde(default)]
    ai: AiSettings,
}

/// Whether the optional assistant is switched on, and how to reach it.
///
/// Here rather than in the vault, deliberately. An API key is a credential for
/// this machine, and a vault is a folder people sync, back up, copy to a
/// second computer and sometimes share — writing a key into it would put a
/// secret somewhere none of those things expect one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiSettings {
    /// Off unless switched on. Not a default that can drift: with this false,
    /// the assistant constructed is [`crate::ai::Off`], which has no way to
    /// reach the network at all.
    #[serde(default)]
    pub enabled: bool,
    /// Stored in the app config directory as plain text.
    ///
    /// Said plainly rather than dressed up: this file is readable by anything
    /// running as the same user. Leaving it unset and exporting
    /// `ANTHROPIC_API_KEY` instead stores nothing at all, and the UI offers
    /// that as the alternative. A real secret store is worth having and is not
    /// in this step.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// What the frontend is allowed to know about the settings.
///
/// The key never comes back out. The UI needs to know whether one is stored,
/// not what it is, and a credential that is only ever written is a credential
/// that cannot leak through a screenshot or a devtools panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    /// What the checkbox says. The setting, not the outcome.
    pub enabled: bool,
    /// Whether asking would actually reach anything.
    ///
    /// Separate from `enabled` because "switched on with no key" is a real
    /// state someone can leave themselves in, and a panel of buttons that can
    /// only fail is worse than one that says what is missing.
    pub ready: bool,
    /// A key is stored in the config file.
    pub has_key: bool,
    /// `ANTHROPIC_API_KEY` is set in the environment, so no key need be stored.
    pub key_in_environment: bool,
    pub model: String,
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

/// An open vault, its index, and the watcher tied to both. They live and die
/// together: when this is replaced, the old watcher is dropped and its thread
/// stops.
///
/// `Arc` on the first two because the watcher thread needs them as well — it
/// reindexes a note that changed on disk before telling the frontend, so the
/// UI never queries an index that disagrees with the files.
struct Open {
    vault: Arc<Vault>,
    index: Arc<Index>,
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

    /// Run `f` against the open index.
    pub fn with_index<T>(&self, f: impl FnOnce(&Index) -> Result<T>) -> Result<T> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let open = guard.as_ref().ok_or(SutraError::NoVault)?;
        f(&open.index)
    }

    /// Run `f` against both, for the operations that must keep them in step.
    pub fn with_both<T>(&self, f: impl FnOnce(&Vault, &Index) -> Result<T>) -> Result<T> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let open = guard.as_ref().ok_or(SutraError::NoVault)?;
        f(&open.vault, &open.index)
    }

    /// Open a directory as the current vault and start watching it.
    pub fn open_vault(&self, app: &AppHandle, root: PathBuf) -> Result<String> {
        let vault = Arc::new(Vault::open(root.clone())?);
        let name = vault.display_name();

        // Build the index from the files every time a vault is opened. It is
        // derived data, so this is always safe, and it is the only way to pick
        // up edits made while the app was closed.
        let index = Arc::new(Index::open(&index_path(app, &root))?);
        index.rebuild(&vault)?;

        // A watcher that fails to start is not fatal — the app works, it just
        // will not notice external edits. Better degraded than dead.
        let watcher =
            match crate::watcher::watch(app.clone(), &root, Arc::clone(&vault), Arc::clone(&index))
            {
                Ok(w) => Some(w),
                Err(e) => {
                    eprintln!(
                        "sutra: could not watch the vault, external edits will be missed: {e}"
                    );
                    None
                }
            };

        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(Open {
            vault,
            index,
            _watcher: watcher,
        });
        drop(guard);

        let mut config = read_config(app).unwrap_or_default();
        config.vault = Some(root);
        save_config(app, &config);
        Ok(name)
    }

    /// The assistant to use, built fresh for each request.
    ///
    /// Returns [`ai::Off`] unless assistance is switched on *and* a key can be
    /// found. Both halves matter: switched off means the object that could
    /// reach the network is never constructed, and no key means there is
    /// nothing to construct it with.
    ///
    /// The environment is preferred over the stored key, so someone who would
    /// rather not have a secret in a config file simply does not put one there.
    pub fn assistant(&self, app: &AppHandle) -> Box<dyn ai::Assistant> {
        choose(&load_settings(app), std::env::var("ANTHROPIC_API_KEY").ok())
    }

    /// The open vault's display name, if any.
    pub fn vault_name(&self) -> Option<String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|o| o.vault.display_name())
    }
}

/// Where a vault's index database lives.
///
/// In the app data directory, never in the vault. The index is derived and
/// machine-local: putting it beside the notes would sync it between machines,
/// and a half-synced SQLite file is worse than no index at all.
///
/// The filename is a hash of the vault path so two vaults do not share one
/// database. The hash need not be stable across releases — if it changes, the
/// worst case is that a fresh index gets built, which costs one scan.
fn index_path(app: &AppHandle, root: &Path) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    let name = format!("{:016x}.sqlite", hasher.finish());

    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("index")
        .join(name)
}

/// Which assistant these settings call for.
///
/// Pure, and separate from reading the config file, because this is the switch
/// the whole feature turns on: "off" has to mean the object that could reach
/// the network is never built, and that is worth a test rather than a reading
/// of the code. Taking the environment as an argument is what makes it one.
fn choose(settings: &AiSettings, env_key: Option<String>) -> Box<dyn ai::Assistant> {
    if !settings.enabled {
        return Box::new(ai::Off);
    }
    // The environment first, so someone who would rather not have a secret in
    // a config file simply does not put one there.
    let key = env_key
        .filter(|k| !k.trim().is_empty())
        .or_else(|| settings.api_key.clone())
        .filter(|k| !k.trim().is_empty());
    match key {
        Some(key) => Box::new(ai::Claude::new(
            key,
            settings
                .model
                .clone()
                .unwrap_or_else(|| ai::DEFAULT_MODEL.into()),
        )),
        None => Box::new(ai::Off),
    }
}

/// The stored AI settings, or the defaults when there are none.
fn load_settings(app: &AppHandle) -> AiSettings {
    read_config(app).unwrap_or_default().ai
}

/// What the frontend may see.
pub fn ai_status(app: &AppHandle) -> AiStatus {
    let settings = load_settings(app);
    let env_key = std::env::var("ANTHROPIC_API_KEY").ok();
    AiStatus {
        enabled: settings.enabled,
        // Asked of the thing that would answer, rather than re-derived here:
        // one rule for which assistant you get, and the UI reads the same one.
        ready: choose(&settings, env_key.clone()).label() != Off.label(),
        has_key: settings
            .api_key
            .clone()
            .is_some_and(|k| !k.trim().is_empty()),
        key_in_environment: std::env::var("ANTHROPIC_API_KEY").is_ok_and(|k| !k.trim().is_empty()),
        model: settings.model.unwrap_or_else(|| ai::DEFAULT_MODEL.into()),
    }
}

/// Change the AI settings, leaving the remembered vault alone.
pub fn set_ai_settings(app: &AppHandle, ai: AiSettings) {
    let mut config = read_config(app).unwrap_or_default();
    config.ai = ai;
    save_config(app, &config);
}

fn read_config(app: &AppHandle) -> Option<Config> {
    let path = config_path(app)?;
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> AiSettings {
        AiSettings {
            enabled: true,
            api_key: Some("stored-key".into()),
            model: None,
        }
    }

    #[test]
    fn assistance_is_off_until_it_is_switched_on() {
        // The default, and the thing the whole feature rests on. `Off` has no
        // way to reach the network, so "switched off" is a fact about which
        // object exists rather than a flag something has to check.
        assert_eq!(choose(&AiSettings::default(), None).label(), "off");
        // Even with a key sitting in the config, and even with one in the
        // environment: a key is not consent.
        assert_eq!(
            choose(
                &AiSettings {
                    enabled: false,
                    ..on()
                },
                Some("env-key".into())
            )
            .label(),
            "off"
        );
    }

    #[test]
    fn switched_on_with_no_key_is_still_off() {
        assert_eq!(
            choose(
                &AiSettings {
                    api_key: None,
                    ..on()
                },
                None
            )
            .label(),
            "off"
        );
        // An empty string is not a key.
        assert_eq!(
            choose(
                &AiSettings {
                    api_key: Some("   ".into()),
                    ..on()
                },
                Some("  ".into())
            )
            .label(),
            "off"
        );
    }

    #[test]
    fn switched_on_with_no_key_is_not_ready() {
        // The state someone leaves themselves in by ticking the box and not
        // pasting a key. The panel reads `ready`, so it says what is missing
        // rather than offering buttons that can only fail.
        for (settings, env) in [
            (AiSettings::default(), None),
            (
                AiSettings {
                    api_key: None,
                    ..on()
                },
                None,
            ),
        ] {
            assert_eq!(choose(&settings, env).label(), Off.label());
        }
        assert_ne!(choose(&on(), None).label(), Off.label());
    }

    #[test]
    fn switched_on_with_a_key_uses_the_default_model() {
        assert_eq!(choose(&on(), None).label(), ai::DEFAULT_MODEL);
        assert_eq!(
            choose(
                &AiSettings {
                    model: Some("claude-haiku-4-5".into()),
                    ..on()
                },
                None
            )
            .label(),
            "claude-haiku-4-5"
        );
    }

    #[test]
    fn the_environment_is_enough_on_its_own() {
        // So a key need never be written to disk at all.
        assert_eq!(
            choose(
                &AiSettings {
                    api_key: None,
                    ..on()
                },
                Some("env-key".into())
            )
            .label(),
            ai::DEFAULT_MODEL
        );
    }
}
