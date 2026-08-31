//! The Tauri command surface — the entire boundary the frontend can see.
//!
//! Two rules hold throughout, both from the spec:
//!
//! 1. No filesystem path crosses this line. Notes are addressed by id, and the
//!    vault is identified by a display name only.
//! 2. The body is markdown text, passed through untouched. Rust is responsible
//!    for the file and its frontmatter, not for interpreting the prose.

use crate::error::{Result, SutraError};
use crate::export::ExportDocument;
use crate::frontmatter::NoteType;
use crate::index::{Backlink, SearchHit};
use crate::state::AppState;
use crate::tags::Suggestion;
use crate::vault::{MigrationPlan, NoteDoc, NoteSummary, Retag, TagChange};
use crate::zotero::{Reference, Zotero};
use serde::Serialize;
use std::collections::HashMap;
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
    folder: Option<String>,
) -> Result<NoteDoc> {
    state.with_both(|vault, index| {
        let doc = vault.create_note(&title, folder.clone())?;
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

/// Write a note out as a .docx.
///
/// Opens a save dialog on the Rust side, so no path crosses the boundary here
/// either. Returns the chosen file's name for the confirmation message, or
/// None if the user cancelled.
#[tauri::command]
pub fn export_docx(app: AppHandle, document: ExportDocument) -> Result<Option<String>> {
    let suggested = format!("{}.docx", crate::note::slugify(&document.title));
    let Some(target) = app
        .dialog()
        .file()
        .set_file_name(&suggested)
        .add_filter("Word document", &["docx"])
        .blocking_save_file()
    else {
        return Ok(None);
    };

    let path = target
        .into_path()
        .map_err(|e| SutraError::Export(e.to_string()))?;
    crate::export::write_docx(&document, &path)?;
    Ok(path.file_name().map(|n| n.to_string_lossy().to_string()))
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
pub fn attach_file(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: Option<String>,
) -> Result<Option<String>> {
    let Some(file) = app.dialog().file().blocking_pick_file() else {
        return Ok(None);
    };
    let path = file
        .into_path()
        .map_err(|e| SutraError::NotADirectory(e.to_string()))?;
    let reference = state.with_vault(|vault| vault.import_attachment(&path, folder))?;
    Ok(Some(reference))
}

/// Move a note into another folder.
///
/// The whole operation is a rename. Nothing that links to this note is
/// rewritten, because links name its id and the id is not in the path.
#[tauri::command]
pub fn move_note(state: State<'_, AppState>, id: String, folder: String) -> Result<NoteSummary> {
    state.with_both(|vault, index| {
        let summary = vault.move_note(&id, &folder)?;
        // The body has not changed, but the indexed row carries the folder, so
        // it has to be rewritten for folder filters to stay correct.
        let doc = vault.read_note(&id)?;
        index.upsert(&summary, &doc.body)?;
        Ok(summary)
    })
}

/// Every tag in the vault, as written, with how many notes carry it.
#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> Result<HashMap<String, usize>> {
    state.with_vault(|vault| vault.list_tags())
}

/// Tags that look like they were meant to be the same. Offered, never applied.
#[tauri::command]
pub fn similar_tags(state: State<'_, AppState>) -> Result<Vec<Suggestion>> {
    state.with_vault(|vault| vault.similar_tags())
}

/// Rename a tag across the vault, or merge it into another.
///
/// Returns what every touched note's tags used to be, which is what the
/// frontend hands back to undo it.
#[tauri::command]
pub fn retag(state: State<'_, AppState>, from: String, to: String) -> Result<Retag> {
    state.with_both(|vault, index| {
        let result = vault.retag(&from, &to)?;
        // Only the notes that changed are re-indexed. A full rebuild would also
        // be correct, and on a large vault it would be a visible pause for an
        // operation that is supposed to feel like an edit.
        for change in &result.changed {
            let doc = vault.read_note(&change.id)?;
            index.upsert(&doc.summary, &doc.body)?;
        }
        Ok(result)
    })
}

/// Put tags back exactly as they were before a retag.
#[tauri::command]
pub fn undo_retag(state: State<'_, AppState>, changed: Vec<TagChange>) -> Result<usize> {
    state.with_both(|vault, index| {
        let restored = vault.undo_retag(&changed)?;
        for change in &changed {
            if let Ok(doc) = vault.read_note(&change.id) {
                index.upsert(&doc.summary, &doc.body)?;
            }
        }
        Ok(restored)
    })
}

/// Capture a note without deciding where it belongs.
///
/// Its own command rather than `create_note` with a folder argument, because
/// the Inbox's name is the vault's business, not the frontend's — and because
/// this is the operation section 13 asks to be faster than organising.
#[tauri::command]
pub fn capture(state: State<'_, AppState>) -> Result<NoteDoc> {
    state.with_both(|vault, index| {
        let doc = vault.create_note("", Some(crate::vault::INBOX.to_string()))?;
        index.upsert(&doc.summary, &doc.body)?;
        Ok(doc)
    })
}

/// Change what kind of note this is.
#[tauri::command]
pub fn set_note_type(
    state: State<'_, AppState>,
    id: String,
    note_type: NoteType,
) -> Result<NoteSummary> {
    state.with_both(|vault, index| {
        let summary = vault.set_type(&id, note_type)?;
        let doc = vault.read_note(&id)?;
        index.upsert(&summary, &doc.body)?;
        Ok(summary)
    })
}

/// Whether this vault still records its hierarchy in frontmatter.
#[tauri::command]
pub fn migration_needed(state: State<'_, AppState>) -> Result<bool> {
    state.with_vault(|vault| vault.needs_migration())
}

/// What migrating would do, without doing any of it.
#[tauri::command]
pub fn migration_plan(state: State<'_, AppState>) -> Result<MigrationPlan> {
    state.with_vault(|vault| vault.migration_plan())
}

/// Reorganise a flat vault into folders. Copies every note first.
///
/// The index is rebuilt rather than patched: a migration moves most of the
/// vault at once, and rebuilding from the markdown afterwards is both simpler
/// and the thing that proves the index really is derived.
#[tauri::command]
pub fn migrate_vault(state: State<'_, AppState>) -> Result<usize> {
    state.with_both(|vault, index| {
        let moved = vault.migrate()?;
        index.rebuild(vault)?;
        Ok(moved)
    })
}

/// Every folder in the vault, shallowest first.
#[tauri::command]
pub fn list_folders(state: State<'_, AppState>) -> Result<Vec<String>> {
    state.with_vault(|vault| vault.list_folders())
}

/// Make a folder. Parents are created as needed; depth is capped.
#[tauri::command]
pub fn create_folder(state: State<'_, AppState>, folder: String) -> Result<String> {
    state.with_vault(|vault| vault.create_folder(&folder))
}
