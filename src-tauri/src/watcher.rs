//! Watching the vault for changes made outside the app.

use crate::note;
use notify_debouncer_full::notify::RecommendedWatcher;
use notify_debouncer_full::notify::{RecursiveMode, Result as NotifyResult};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
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
/// Non-recursive: notes are flat in the root, and we do not want every write
/// into `attachments/` or `trash/` waking the UI.
pub fn watch(app: AppHandle, root: &Path) -> NotifyResult<VaultWatcher> {
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
                if name.ends_with(".tmp") {
                    continue;
                }
                if let Some(id) = note::id_from_file_name(name) {
                    changed.insert(id.to_string());
                }
            }
        }

        if changed.is_empty() {
            return;
        }
        let payload = VaultChanged {
            changed: changed.into_iter().collect(),
        };
        // If the window has gone away there is nobody to tell, which is not
        // an error worth propagating out of a watcher thread.
        let _ = app.emit("vault:changed", payload);
    })?;

    debouncer.watch(root, RecursiveMode::NonRecursive)?;
    Ok(debouncer)
}
