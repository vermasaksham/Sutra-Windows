//! The Tauri command surface — the entire boundary the frontend can see.
//!
//! Two rules hold throughout, both from the spec:
//!
//! 1. No filesystem path crosses this line. Notes are addressed by id, and the
//!    vault is identified by a display name only.
//! 2. The body is markdown text, passed through untouched. Rust is responsible
//!    for the file and its frontmatter, not for interpreting the prose.

use crate::error::{Result, SutraError};
use crate::index::{Backlink, SearchHit};
use crate::state::AppState;
use crate::vault::{NoteDoc, NoteSummary};
use crate::zotero::{Reference, Zotero};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// What the frontend knows about the open vault: its name, and nothing else.
#[derive(Debug, Serialize)]
pub struct VaultInfo {
    pub name: String,
}

/// Open the native folder picker and adopt the chosen directory.
///
/// The dialog is opened from Rust rather than from JavaScript so the path never
/// exists on the frontend side at all. `blocking_pick_folder` is fine here
/// because Tauri runs commands on a worker thread, not the UI thread.
///
/// `Ok(None)` means the user cancelled, which is not an error.
#[tauri::command]
pub fn pick_vault(app: AppHandle, state: State<'_, AppState>) -> Result<Option<VaultInfo>> {
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = folder
        .into_path()
        .map_err(|e| SutraError::NotADirectory(e.to_string()))?;
    let name = state.open_vault(&app, path)?;
    Ok(Some(VaultInfo { name }))
}

/// The vault restored from the last session, if there is one.
#[tauri::command]
pub fn current_vault(state: State<'_, AppState>) -> Option<VaultInfo> {
    state.vault_name().map(|name| VaultInfo { name })
}

/// Every note, for building the sidebar tree.
///
/// Served from the index rather than by scanning the directory: the sidebar is
/// redrawn on every save, and re-reading every file each time would make the
/// app slower the more notes it holds. `parent` and `position` come back
/// unchanged, so the frontend assembles the tree without any SQL crossing the
/// boundary.
#[tauri::command]
pub fn list_notes(state: State<'_, AppState>) -> Result<Vec<NoteSummary>> {
    state.with_index(|index| index.all_notes())
}

/// Full-text search across the vault.
#[tauri::command]
pub fn search_notes(state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>> {
    state.with_index(|index| index.search(&query, 30))
}

/// Notes that link to `id`, for the backlinks panel.
#[tauri::command]
pub fn backlinks(state: State<'_, AppState>, id: String) -> Result<Vec<Backlink>> {
    state.with_index(|index| index.backlinks(&id))
}

/// Throw the index away and rebuild it from the markdown files.
///
/// Exposed because it should always be safe to do. If search or the tree ever
/// look wrong, this is the fix, and the fact that it cannot lose anything is
/// the point of the whole storage design.
#[tauri::command]
pub fn reindex(state: State<'_, AppState>) -> Result<usize> {
    state.with_both(|vault, index| index.rebuild(vault))
}

#[tauri::command]
pub fn read_note(state: State<'_, AppState>, id: String) -> Result<NoteDoc> {
    state.with_vault(|vault| vault.read_note(&id))
}

#[tauri::command]
pub fn create_note(
    state: State<'_, AppState>,
    title: String,
    parent: Option<String>,
) -> Result<NoteDoc> {
    state.with_both(|vault, index| {
        let doc = vault.create_note(&title, parent.clone())?;
        index.upsert(&doc.summary, &doc.body)?;
        Ok(doc)
    })
}

/// Save a note. Returns the updated metadata so the sidebar can refresh a
/// changed title or timestamp without re-listing the whole vault.
#[tauri::command]
pub fn save_note(
    state: State<'_, AppState>,
    id: String,
    title: String,
    body: String,
) -> Result<NoteSummary> {
    state.with_both(|vault, index| {
        // File first, index second. If the write fails there is nothing to
        // index, and if the index fails the note is still safely on disk and a
        // rebuild will pick it up.
        let summary = vault.save_note(&id, &title, &body)?;
        index.upsert(&summary, &body)?;
        Ok(summary)
    })
}

/// Replace a note's icon, cover and tags. The body is untouched.
#[tauri::command]
pub fn set_note_meta(
    state: State<'_, AppState>,
    id: String,
    icon: Option<String>,
    cover: Option<String>,
    tags: Vec<String>,
) -> Result<NoteSummary> {
    state.with_both(|vault, index| {
        let summary = vault.set_meta(&id, icon.clone(), cover.clone(), tags.clone())?;
        // The body has not changed, but the index row carries the title, tags
        // and icon, so it has to be rewritten. Re-read rather than assume: the
        // note on disk is the truth.
        let doc = vault.read_note(&id)?;
        index.upsert(&summary, &doc.body)?;
        Ok(summary)
    })
}

/// Move a note to the trash folder. Nothing is unlinked.
#[tauri::command]
pub fn delete_note(state: State<'_, AppState>, id: String) -> Result<()> {
    state.with_both(|vault, index| {
        vault.delete_note(&id)?;
        index.remove(&id)?;
        Ok(())
    })
}

/// Search the running Zotero for references.
///
/// Not cached: a library changes while the app is open, and a stale hit that
/// no longer exists is worse than asking again. Zotero is on the loopback
/// interface, so the round trip is cheap.
#[tauri::command]
pub fn zotero_search(query: String) -> Result<Vec<Reference>> {
    Zotero::default().search(&query, 20)
}

/// Resolve citation keys to references, so `[@KEY]` can render a label.
#[tauri::command]
pub fn zotero_by_keys(keys: Vec<String>) -> Result<Vec<Reference>> {
    Zotero::default().by_keys(&keys)
}

/// Copy a file into the vault's attachments folder.
///
/// Returns the vault-relative reference to put in the markdown. The frontend
/// picks the file through the same Rust-side dialog, so again no path crosses
/// the boundary in either direction.
#[tauri::command]
pub fn attach_file(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>> {
    let Some(file) = app.dialog().file().blocking_pick_file() else {
        return Ok(None);
    };
    let path = file
        .into_path()
        .map_err(|e| SutraError::NotADirectory(e.to_string()))?;
    let reference = state.with_vault(|vault| vault.import_attachment(&path))?;
    Ok(Some(reference))
}
