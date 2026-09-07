//! Where API keys live.
//!
//! Two credentials exist: a Zotero API key, which is read-write over someone's
//! whole library, and an Anthropic key, which spends money. Until v0.2.1 both
//! were written as plain text into `sutra.json` in the app config directory —
//! said out loud in the code rather than hidden, but still a secret readable by
//! anything running as that user.
//!
//! # What this does and does not promise
//!
//! On Windows and macOS a key goes into the platform's own credential store
//! (Credential Manager, Keychain) through the `keyring` crate. On every other
//! platform there is no store here at all and [`KeyStorage::ConfigFile`] is
//! reported, so the UI can say plainly where the key is rather than implying a
//! protection that does not exist. Sutra ships for Windows; Linux support is a
//! development convenience, and inventing a half-working secret service for it
//! would be worse than saying so.
//!
//! The environment always wins. `ZOTERO_API_KEY` and `ANTHROPIC_API_KEY` are
//! read first and stored nowhere, which stays the option for anyone who would
//! rather Sutra never persisted a credential at all.
//!
//! # Why a trait
//!
//! So the migration and fallback logic can be tested. A real keyring needs a
//! logged-in desktop session with a running credential service; CI has
//! neither. [`MemoryStore`] stands in, and every rule below is proved against
//! it rather than asserted in a comment.

use crate::error::{Result, SutraError};
use serde::Serialize;

/// The service name every credential is filed under.
///
/// Only the platform store reads it, so on a platform without one it is
/// genuinely unused rather than accidentally so.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
const SERVICE: &str = "dev.sutra.app";

/// The account name for the Zotero key.
pub const ZOTERO: &str = "zotero-api-key";
/// The account name for the assistant's key.
pub const ANTHROPIC: &str = "anthropic-api-key";

/// Where a key actually is, so the UI can say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyStorage {
    /// Not set anywhere.
    None,
    /// In the environment, and stored by this app nowhere.
    Environment,
    /// In the platform credential store.
    Keychain,
    /// In `sutra.json`, as plain text. Either this platform has no store, or
    /// writing to it failed and the user chose to carry on.
    ConfigFile,
}

/// Somewhere a secret can be kept.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;
    /// Whether this is a real credential store rather than a stand-in.
    ///
    /// Drives what the UI is told, and whether a failed write may fall back to
    /// the config file: on a platform that has a store, a write that fails is
    /// an error worth showing, not a reason to quietly write plain text.
    fn is_secure(&self) -> bool;
}

/// The platform credential store, where there is one.
#[cfg(any(windows, target_os = "macos"))]
pub struct PlatformStore;

#[cfg(any(windows, target_os = "macos"))]
impl SecretStore for PlatformStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        let entry =
            keyring::Entry::new(SERVICE, account).map_err(|e| SutraError::Secret(e.to_string()))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SutraError::Secret(e.to_string())),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<()> {
        let entry =
            keyring::Entry::new(SERVICE, account).map_err(|e| SutraError::Secret(e.to_string()))?;
        entry
            .set_password(secret)
            .map_err(|e| SutraError::Secret(e.to_string()))
    }

    fn delete(&self, account: &str) -> Result<()> {
        let entry =
            keyring::Entry::new(SERVICE, account).map_err(|e| SutraError::Secret(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SutraError::Secret(e.to_string())),
        }
    }

    fn is_secure(&self) -> bool {
        true
    }
}

/// The stand-in for a platform with no credential store wired up.
///
/// Holds nothing and reports itself insecure, which is what makes the config
/// file the honest place for a key there.
///
/// Genuinely unused on Windows and macOS, where `platform_store` returns the
/// real thing — the same shape as `SERVICE` above, which only those platforms
/// read. Windows CI caught this as `struct NoStore is never constructed`,
/// which a Linux `cargo clippy` cannot see: neither half of a `cfg` is
/// compiled on the platform it is not for, so each one is only ever checked
/// where it applies.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
pub struct NoStore;

impl SecretStore for NoStore {
    fn get(&self, _account: &str) -> Result<Option<String>> {
        Ok(None)
    }
    fn set(&self, _account: &str, _secret: &str) -> Result<()> {
        Err(SutraError::Secret(
            "this platform has no credential store".into(),
        ))
    }
    fn delete(&self, _account: &str) -> Result<()> {
        Ok(())
    }
    fn is_secure(&self) -> bool {
        false
    }
}

/// An in-memory store, for tests.
#[cfg(test)]
pub struct MemoryStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
    /// Set to make every write fail, standing in for a locked keychain.
    pub failing: bool,
    pub secure: bool,
}

#[cfg(test)]
impl MemoryStore {
    pub fn new() -> Self {
        Self {
            entries: Default::default(),
            failing: false,
            secure: true,
        }
    }
    pub fn failing() -> Self {
        Self {
            failing: true,
            ..Self::new()
        }
    }
    /// A platform with no credential store: it refuses writes *and* reports
    /// itself insecure, exactly as [`NoStore`] does.
    pub fn insecure() -> Self {
        Self {
            secure: false,
            failing: true,
            ..Self::new()
        }
    }
}

#[cfg(test)]
impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.entries.lock().unwrap().get(account).cloned())
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        if self.failing {
            return Err(SutraError::Secret("the keychain refused".into()));
        }
        self.entries
            .lock()
            .unwrap()
            .insert(account.to_string(), secret.to_string());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        self.entries.lock().unwrap().remove(account);
        Ok(())
    }
    fn is_secure(&self) -> bool {
        self.secure
    }
}

/// The store this build uses.
pub fn platform_store() -> Box<dyn SecretStore> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        Box::new(PlatformStore)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Box::new(NoStore)
    }
}

/// What reading a key produced, and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub key: Option<String>,
    pub storage: KeyStorage,
    /// True when a plaintext key has been copied into the store and the
    /// config's copy should now be cleared.
    ///
    /// Only ever set after the store confirmed the write. A migration that
    /// deletes the plaintext copy and then fails to save the new one has lost
    /// the user's key, which is far worse than leaving it where it was.
    pub clear_plaintext: bool,
}

/// Find a key, migrating a plaintext one into the store on the way.
///
/// The order is the one the app has always used: environment, then stored,
/// then whatever is in the config file.
///
/// Migration happens here rather than in a separate startup pass because this
/// is the only place holding all three answers at once. It is safe to run on
/// every read: once the store has the key, the plaintext branch is never
/// reached again.
///
/// A store that fails to *read* is treated as empty rather than fatal. A
/// locked or unavailable keychain must not make the app unusable, and falling
/// through to the config file is exactly the recovery path — the key is then
/// reported as living in the config file, which is true.
pub fn resolve(
    store: &dyn SecretStore,
    account: &str,
    environment: Option<String>,
    plaintext: Option<&str>,
) -> Resolved {
    if let Some(key) = environment.filter(|k| !k.trim().is_empty()) {
        return Resolved {
            key: Some(key),
            storage: KeyStorage::Environment,
            clear_plaintext: false,
        };
    }

    if let Ok(Some(key)) = store.get(account)
        && !key.trim().is_empty()
    {
        return Resolved {
            key: Some(key),
            storage: KeyStorage::Keychain,
            // The config's copy is stale the moment the store has one. Saying
            // so here is what finally removes a plaintext key left behind by a
            // migration that was interrupted before it could clear it.
            clear_plaintext: plaintext.is_some(),
        };
    }

    let Some(key) = plaintext.filter(|k| !k.trim().is_empty()) else {
        return Resolved {
            key: None,
            storage: KeyStorage::None,
            clear_plaintext: false,
        };
    };

    // A plaintext key, and nothing in the store. Try to move it in.
    match store.set(account, key) {
        Ok(()) => Resolved {
            key: Some(key.to_string()),
            storage: KeyStorage::Keychain,
            clear_plaintext: true,
        },
        // The store would not take it. Keep the key working where it is and
        // report honestly that it is in the config file.
        Err(_) => Resolved {
            key: Some(key.to_string()),
            storage: KeyStorage::ConfigFile,
            clear_plaintext: false,
        },
    }
}

/// What a save should do with a key the user just typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    /// What to write into the config file: `None` clears the field, `Some`
    /// keeps a plaintext copy because there was nowhere better.
    pub plaintext: Option<String>,
    pub storage: KeyStorage,
    /// Why the key could not be stored securely, when it could not be. Carried
    /// to the UI so a failure is visible rather than swallowed.
    pub warning: Option<String>,
}

/// Store a key the user has just entered.
///
/// `None` means "leave whatever is stored alone"; `Some("")` means "remove it".
/// That distinction is the existing behaviour of the settings form — an
/// untouched password box must not wipe a working key — and it is kept.
/// `held` is the plaintext copy currently in the config file, which exists
/// only on a platform with no store, or after a save that had to fall back. It
/// has to be passed in so that an untouched box does not wipe it — on such a
/// platform the config file *is* where the key lives.
pub fn save(
    store: &dyn SecretStore,
    account: &str,
    key: Option<&str>,
    held: Option<&str>,
) -> Saved {
    let Some(key) = key else {
        // Untouched. Leave both copies exactly as they are.
        let stored = matches!(store.get(account), Ok(Some(_)));
        return Saved {
            plaintext: held.map(str::to_string),
            storage: match (stored, held) {
                (true, _) => KeyStorage::Keychain,
                (false, Some(_)) => KeyStorage::ConfigFile,
                (false, None) => KeyStorage::None,
            },
            warning: None,
        };
    };

    if key.trim().is_empty() {
        let warning = store
            .delete(account)
            .err()
            .map(|e| format!("The stored key could not be removed: {e}"));
        return Saved {
            plaintext: None,
            storage: KeyStorage::None,
            warning,
        };
    }

    match store.set(account, key) {
        Ok(()) => Saved {
            plaintext: None,
            storage: KeyStorage::Keychain,
            warning: None,
        },
        Err(e) => {
            // Two different failures, and they deserve different words. A
            // platform with no store was never going to have one; a platform
            // that has one and refused is something the user should look at.
            let warning = if store.is_secure() {
                format!(
                    "The key could not be saved to this computer's credential store ({e}). \
                     It has been kept in Sutra's settings file as plain text instead. \
                     Set ZOTERO_API_KEY or ANTHROPIC_API_KEY in the environment to store nothing at all."
                )
            } else {
                "This platform has no credential store, so the key is kept in Sutra's \
                 settings file as plain text. Set ZOTERO_API_KEY or ANTHROPIC_API_KEY in \
                 the environment to store nothing at all."
                    .to_string()
            };
            Saved {
                plaintext: Some(key.to_string()),
                storage: KeyStorage::ConfigFile,
                warning: Some(warning),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_wins_and_stores_nothing() {
        let store = MemoryStore::new();
        let got = resolve(&store, ZOTERO, Some("from-env".into()), Some("from-file"));
        assert_eq!(got.key.as_deref(), Some("from-env"));
        assert_eq!(got.storage, KeyStorage::Environment);
        assert!(!got.clear_plaintext);
        assert_eq!(store.get(ZOTERO).unwrap(), None, "nothing may be stored");
    }

    #[test]
    fn a_blank_environment_variable_is_not_a_key() {
        let store = MemoryStore::new();
        store.set(ZOTERO, "stored").unwrap();
        let got = resolve(&store, ZOTERO, Some("   ".into()), None);
        assert_eq!(got.key.as_deref(), Some("stored"));
        assert_eq!(got.storage, KeyStorage::Keychain);
    }

    #[test]
    fn a_plaintext_key_migrates_into_the_store() {
        let store = MemoryStore::new();
        let got = resolve(&store, ZOTERO, None, Some("plaintext"));

        assert_eq!(got.key.as_deref(), Some("plaintext"));
        assert_eq!(got.storage, KeyStorage::Keychain);
        assert!(got.clear_plaintext, "the config copy must now be removed");
        assert_eq!(store.get(ZOTERO).unwrap().as_deref(), Some("plaintext"));
    }

    #[test]
    fn migration_only_clears_the_plaintext_after_the_store_took_it() {
        // The failure that must never happen: the config field is emptied and
        // the key is nowhere. A refusing store leaves everything as it was.
        let store = MemoryStore::failing();
        let got = resolve(&store, ZOTERO, None, Some("plaintext"));

        assert_eq!(
            got.key.as_deref(),
            Some("plaintext"),
            "the key must keep working"
        );
        assert!(
            !got.clear_plaintext,
            "the only copy of the key was about to be deleted"
        );
        assert_eq!(got.storage, KeyStorage::ConfigFile, "and say so honestly");
    }

    #[test]
    fn migration_is_idempotent() {
        let store = MemoryStore::new();
        resolve(&store, ZOTERO, None, Some("plaintext"));
        // Second launch: the store has it, the config no longer does.
        let again = resolve(&store, ZOTERO, None, None);
        assert_eq!(again.key.as_deref(), Some("plaintext"));
        assert_eq!(again.storage, KeyStorage::Keychain);
        assert!(!again.clear_plaintext);
    }

    #[test]
    fn a_stored_key_wins_over_a_leftover_plaintext_one_and_clears_it() {
        // A migration interrupted between storing and saving the config. The
        // store is authoritative, and the stale copy is asked to go.
        let store = MemoryStore::new();
        store.set(ZOTERO, "the real one").unwrap();
        let got = resolve(&store, ZOTERO, None, Some("stale"));
        assert_eq!(got.key.as_deref(), Some("the real one"));
        assert!(got.clear_plaintext);
    }

    #[test]
    fn nothing_anywhere_is_not_an_error() {
        let store = MemoryStore::new();
        let got = resolve(&store, ANTHROPIC, None, None);
        assert_eq!(
            got,
            Resolved {
                key: None,
                storage: KeyStorage::None,
                clear_plaintext: false
            }
        );
    }

    #[test]
    fn saving_a_key_puts_it_in_the_store_and_not_in_the_file() {
        let store = MemoryStore::new();
        let saved = save(&store, ANTHROPIC, Some("sk-test"), None);
        assert_eq!(saved.plaintext, None, "no plaintext copy may be written");
        assert_eq!(saved.storage, KeyStorage::Keychain);
        assert!(saved.warning.is_none());
        assert_eq!(store.get(ANTHROPIC).unwrap().as_deref(), Some("sk-test"));
    }

    #[test]
    fn an_empty_key_removes_the_stored_one() {
        let store = MemoryStore::new();
        store.set(ANTHROPIC, "sk-test").unwrap();
        let saved = save(&store, ANTHROPIC, Some(""), None);
        assert_eq!(saved.storage, KeyStorage::None);
        assert_eq!(store.get(ANTHROPIC).unwrap(), None);
    }

    #[test]
    fn an_untouched_box_leaves_the_stored_key_alone() {
        let store = MemoryStore::new();
        store.set(ANTHROPIC, "sk-test").unwrap();
        let saved = save(&store, ANTHROPIC, None, None);
        assert_eq!(saved.storage, KeyStorage::Keychain);
        assert_eq!(store.get(ANTHROPIC).unwrap().as_deref(), Some("sk-test"));
    }

    #[test]
    fn an_untouched_box_does_not_wipe_a_config_file_key() {
        // On a platform with no credential store the config file *is* where
        // the key lives. Saving the style dropdown must not delete it.
        let store = MemoryStore::insecure();
        let saved = save(&store, ZOTERO, None, Some("lives-in-the-file"));
        assert_eq!(saved.plaintext.as_deref(), Some("lives-in-the-file"));
        assert_eq!(saved.storage, KeyStorage::ConfigFile);
        assert!(saved.warning.is_none(), "nothing happened, so say nothing");
    }

    #[test]
    fn a_refused_save_is_reported_and_never_silently_dropped() {
        let store = MemoryStore::failing();
        let saved = save(&store, ZOTERO, Some("zot-key"), None);

        // The key still works...
        assert_eq!(saved.plaintext.as_deref(), Some("zot-key"));
        // ...the UI is told where it really is...
        assert_eq!(saved.storage, KeyStorage::ConfigFile);
        // ...and the user is told why.
        let warning = saved.warning.expect("a failed save must be surfaced");
        assert!(warning.contains("credential store"), "{warning}");
        assert!(warning.contains("plain text"), "{warning}");
    }

    #[test]
    fn a_platform_without_a_store_says_so_in_different_words() {
        let store = MemoryStore::insecure();
        let saved = save(&store, ZOTERO, Some("zot-key"), None);
        let warning = saved.warning.expect("this must still be surfaced");
        assert!(
            warning.contains("no credential store"),
            "a platform that never had a store must not read as a malfunction: {warning}"
        );
        assert_eq!(saved.storage, KeyStorage::ConfigFile);
    }

    #[test]
    fn the_real_store_is_never_a_no_store_on_windows_or_macos() {
        // Pins the cfg: a build for a shipping platform that quietly compiled
        // the stand-in would report ConfigFile for ever and nobody would
        // notice.
        let secure = platform_store().is_secure();
        #[cfg(any(windows, target_os = "macos"))]
        assert!(secure, "the platform credential store was not compiled in");
        #[cfg(not(any(windows, target_os = "macos")))]
        assert!(!secure, "a store appeared on a platform that has none");
    }
}
