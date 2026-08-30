//! Watching the vault for changes made outside the app.

use crate::index::Index;
use crate::vault::Vault;
use notify_debouncer_full::notify::RecommendedWatcher;
use notify_debouncer_full::notify::{RecursiveMode, Result as NotifyResult};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// How long to wait for the filesystem to settle before reporting.
///
/// Editors rarely write once. Many write a temp file, rename, and touch
/// metadata — three events for one logical save. Our own atomic writes do the
/// same. Debouncing collapses that burst into a single notification, and also
/// means our own save does not immediately bounce back as an external change.
const QUIET_PERIOD: Duration = Duration::from_millis(400);

/// The event payload the frontend receives.
///
/// Ids only. The frontend decides what to do per note using the rule from the
/// design doc: reload silently when the buffer is clean, prompt when it is
/// dirty. That decision needs to know *which* notes changed and nothing more.
#[derive(Debug, Clone, Serialize)]
pub struct VaultChanged {
    pub changed: Vec<String>,
}

/// Keeping this alive keeps the watch running; dropping it stops it.
///
/// The type is a mouthful because the debouncer is generic over the backend
/// (inotify, ReadDirectoryChangesW, FSEvents) and its cache. `RecommendedWatcher`
/// is whichever one this platform uses.
pub type VaultWatcher = Debouncer<RecommendedWatcher, RecommendedCache>;

/// Start watching a vault directory, emitting `vault:changed` to the frontend.
///
/// Recursive, because notes live in nested folders now. The cost is that
/// writes into a hidden `.attachments` or `.sutra` also arrive, so those are
/// filtered out below rather than by the watch mode.
pub fn watch(
    app: AppHandle,
    root: &Path,
    vault: Arc<Vault>,
    index: Arc<Index>,
) -> NotifyResult<VaultWatcher> {
    let root_for_events = root.to_path_buf();
    let mut debouncer = new_debouncer(QUIET_PERIOD, None, move |result: DebounceEventResult| {
        let Ok(events) = result else { return };

        // A set, so a burst touching one note reports it once, and sorted
        // so the payload is deterministic.
        let mut changed = BTreeSet::new();
        for event in events {
            for path in &event.paths {
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                // Skip our own in-flight temp files; the rename that
                // follows is the event that matters.
                if !name.ends_with(".md") || name.ends_with(".tmp") {
                    continue;
                }
                // Anything inside a hidden folder is ours, not the user's:
                // `.sutra/trash` fills up on every delete and must not look
                // like a note appearing.
                if is_hidden(path, root_for_events.as_path()) {
                    continue;
                }
                // The id is inside the file now, so only the vault can say
                // which note a path belongs to.
                if let Some(id) = vault.id_at(path) {
                    changed.insert(id);
                }
            }
        }

        if changed.is_empty() {
            return;
        }

        // Reindex before notifying. The frontend reacts to this event by
        // reloading the note and refreshing the tree and backlinks, so if the
        // index were updated afterwards the UI would render a view of the
        // vault that is one edit out of date.
        for id in &changed {
            match vault.read_note(id) {
                Ok(doc) => {
                    if let Err(e) = index.upsert(&doc.summary, &doc.body) {
                        eprintln!("sutra: could not index {id}: {e}");
                    }
                }
                // Unreadable means deleted, moved to trash, or mid-write by
                // another program. Dropping it from the index is right in
                // every one of those cases; a later event re-adds it if it
                // comes back.
                Err(_) => {
                    if let Err(e) = index.remove(id) {
                        eprintln!("sutra: could not de-index {id}: {e}");
                    }
                }
            }
        }

        let payload = VaultChanged {
            changed: changed.into_iter().collect(),
        };
        // If the window has gone away there is nobody to tell, which is not
        // an error worth propagating out of a watcher thread.
        let _ = app.emit("vault:changed", payload);
    })?;

    debouncer.watch(root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

/// True when any folder between the vault root and this file starts with a dot.
///
/// The same one rule that keeps `.sutra` and every `.attachments` out of the
/// note listing, applied to events.
fn is_hidden(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        // Outside the vault entirely. Not ours either way.
        return true;
    };
    relative
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => n.to_str(),
            _ => None,
        })
        .any(|name| name.starts_with('.'))
}
