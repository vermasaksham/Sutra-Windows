//! The Tauri command surface — the entire boundary the frontend can see.
//!
//! Two rules hold throughout, both from the spec:
//!
//! 1. No filesystem path crosses this line. Notes are addressed by id, and the
//!    vault is identified by a display name only.
//! 2. The body is markdown text, passed through untouched. Rust is responsible
//!    for the file and its frontmatter, not for interpreting the prose.

use crate::ai::{Ask, Draft, Task};
use crate::claims::Disagreement;
use crate::duplicates::Duplicate;
use crate::error::{Result, SutraError};
use crate::export::ExportDocument;
use crate::frontmatter::NoteType;
use crate::frontmatter::{Citation, SourceMeta};
use crate::index::CitingNote;
use crate::index::{Backlink, DuplicatePair, SearchHit, ViewResult};
use crate::references::{Availability, ItemDetail, Reference, ReferenceProvider};
use crate::related::Related;
use crate::state::{self, AiSettings, AiStatus, AppState};
use crate::tags::Suggestion;
use crate::vault::{MigrationPlan, NoteDoc, NoteSummary, Retag, TagChange};
use crate::views::Query;
use crate::zotero::{WEB_BASE as ZOTERO_WEB_BASE, Zotero};
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
pub fn zotero_search(app: AppHandle, query: String) -> Result<Vec<Reference>> {
    crate::state::provider(&app).search(&query, 20)
}

/// Resolve citation keys to references, so `[@KEY]` can render a label.
#[tauri::command]
pub fn zotero_by_keys(app: AppHandle, keys: Vec<String>) -> Result<Vec<Reference>> {
    crate::state::provider(&app).items(&keys)
}

/// Whether the reference manager is reachable, and why not when it is not.
///
/// Deliberately never an error: "Zotero is closed" is an ordinary state of the
/// world, not a failure of the app, and the caller needs to render a status
/// line rather than an error toast. Notes, sources, evidence and citations all
/// keep working from cached metadata regardless of what this returns.
#[tauri::command]
pub fn reference_status(app: AppHandle) -> Availability {
    crate::state::provider(&app).availability()
}

/// The stored connection and style, with the key withheld.
#[tauri::command]
pub fn reference_config(app: AppHandle) -> crate::state::ReferenceConfig {
    crate::state::reference_config(&app)
}

/// Save the connection and the citation style.
#[tauri::command]
pub fn configure_references(
    app: AppHandle,
    account: bool,
    user_id: Option<String>,
    api_key: Option<String>,
    style: String,
    locale: String,
) -> crate::state::ReferenceConfig {
    crate::state::set_reference_settings(&app, account, user_id, api_key, style, locale)
}

/// Re-render every linked source in the current style.
///
/// Needed because the styled forms are a cache: switching style leaves every
/// existing source holding the old one. Returns how many were restyled, so the
/// UI can say something true rather than "done".
///
/// Sources the library cannot render — it does not have that style, the item
/// is gone — are left exactly as they were rather than blanked. A citation
/// that still reads correctly in the previous style beats one that reads
/// "Untitled".
#[tauri::command]
pub fn restyle_sources(app: AppHandle, state: State<'_, AppState>) -> Result<usize> {
    let settings = crate::state::reference_settings(&app);
    let style = settings.style.trim().to_string();
    if style.is_empty() {
        return Ok(0);
    }

    let linked = state.with_vault(|vault| vault.linked_sources())?;
    if linked.is_empty() {
        return Ok(0);
    }

    let provider = crate::state::provider(&app);
    let keys: Vec<String> = linked.iter().map(|(_, key)| key.clone()).collect();

    let mut done = 0;
    // In batches: a library of four hundred papers is one URL Zotero will
    // refuse, and asking one item at a time is four hundred round trips.
    for chunk in keys.chunks(STYLE_BATCH) {
        let rendered = provider.styled(chunk, &style, &settings.locale)?;
        state.with_vault(|vault| {
            for (key, styled) in &rendered {
                if let Some((id, _)) = linked.iter().find(|(_, k)| k == key) {
                    vault.cache_style(id, &style, styled.clone())?;
                    done += 1;
                }
            }
            Ok(())
        })?;
    }
    Ok(done)
}

/// How many items to ask Zotero to render at once.
const STYLE_BATCH: usize = 25;

/// Cache the current style for one freshly imported source.
///
/// Best-effort on purpose. A source that imported cleanly but could not be
/// rendered — no such style, the library went away between the two requests —
/// is still a perfectly good source note, and failing the import over its
/// formatting would be losing the paper to save the punctuation.
fn cache_current_style(app: &AppHandle, vault: &crate::vault::Vault, id: &str, key: &str) {
    let settings = crate::state::reference_settings(app);
    let style = settings.style.trim();
    if style.is_empty() {
        return;
    }
    let provider = crate::state::provider(app);
    if let Ok(rendered) = provider.styled(
        std::slice::from_ref(&key.to_string()),
        style,
        &settings.locale,
    ) && let Some((_, styled)) = rendered.into_iter().next()
    {
        let _ = vault.cache_style(id, style, styled);
    }
}

/// Connect a Zotero account from the key alone.
///
/// Takes the key, asks Zotero whose it is, stores both, and reports back. This
/// replaces the step that was failing: the API needs a numeric user ID, Zotero
/// documents that it is "different from usernames", and a username produces a
/// 404 that reads like an empty library. Nobody should have to know that.
///
/// A key with no read access to the personal library is refused here rather
/// than saved, because it would connect successfully and then find nothing —
/// the worst kind of failure to debug.
#[tauri::command]
pub fn connect_zotero_account(
    app: AppHandle,
    api_key: String,
) -> Result<crate::state::ReferenceConfig> {
    let identity = Zotero::identify(ZOTERO_WEB_BASE, &api_key)?;
    if !identity.can_read {
        return Err(SutraError::Zotero(
            "That key has no read access to your library. Create one at \
             zotero.org/settings/keys with \"Allow library access\" ticked."
                .into(),
        ));
    }

    let settings = crate::state::reference_settings(&app);
    Ok(crate::state::set_reference_settings(
        &app,
        true,
        Some(identity.user_id),
        Some(api_key),
        settings.style,
        settings.locale,
    ))
}

/// Everything about one item, collections and attachments included.
#[tauri::command]
pub fn zotero_detail(app: AppHandle, key: String) -> Result<ItemDetail> {
    crate::state::provider(&app).detail(&key)
}

/// Show the item in Zotero's own window.
#[tauri::command]
pub fn zotero_open(app: AppHandle, key: String) -> Result<()> {
    crate::state::provider(&app).open(&key)
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

/// What migrating the legacy citations would involve.
#[tauri::command]
pub fn legacy_citations(state: State<'_, AppState>) -> Result<HashMap<String, usize>> {
    state.with_vault(|vault| vault.legacy_citations())
}

/// What a citation migration did.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationMigration {
    /// Zotero key, and the source note it now points at.
    pub migrated: Vec<(String, String)>,
    /// Keys Zotero could not answer for. Left exactly as they were.
    pub unresolved: Vec<String>,
    pub notes_changed: usize,
}

/// Turn every `[@KEY]` into a citation of a source note in the vault.
///
/// Needs Zotero, once: the keys came from there and only it knows what they
/// stand for. After this the vault does not need Zotero again — which is the
/// entire point of doing it.
#[tauri::command]
pub fn migrate_citations(app: AppHandle, state: State<'_, AppState>) -> Result<CitationMigration> {
    let counts = state.with_vault(|vault| vault.legacy_citations())?;
    let mut keys: Vec<String> = counts.into_keys().collect();
    keys.sort();
    if keys.is_empty() {
        return Ok(CitationMigration {
            migrated: Vec::new(),
            unresolved: Vec::new(),
            notes_changed: 0,
        });
    }

    let found = crate::state::provider(&app).items(&keys)?;
    let mut mapping = HashMap::new();
    let mut migrated = Vec::new();

    state.with_both(|vault, index| {
        for reference in &found {
            let summary = vault.import_source(&reference.title, reference.to_source())?;
            let doc = vault.read_note(&summary.id)?;
            index.upsert(&summary, &doc.body)?;
            mapping.insert(reference.key.clone(), summary.id.clone());
            migrated.push((reference.key.clone(), summary.id));
        }
        Ok(())
    })?;

    let notes_changed = state.with_both(|vault, index| {
        let changed = vault.migrate_citations(&mapping)?;
        // Bodies changed, so the links and full-text rows for those notes are
        // stale. A rebuild is the simple, provably complete answer, and this
        // runs once.
        index.rebuild(vault)?;
        Ok(changed)
    })?;

    let unresolved = keys
        .into_iter()
        .filter(|k| !mapping.contains_key(k))
        .collect();

    Ok(CitationMigration {
        migrated,
        unresolved,
        notes_changed,
    })
}

/// Create a source note in the library.
#[tauri::command]
pub fn create_source(
    state: State<'_, AppState>,
    title: String,
    meta: SourceMeta,
) -> Result<NoteDoc> {
    state.with_both(|vault, index| {
        let doc = vault.create_source(&title, meta.clone())?;
        index.upsert(&doc.summary, &doc.body)?;
        Ok(doc)
    })
}

/// Replace what a source note records about its paper.
#[tauri::command]
pub fn set_source_meta(
    state: State<'_, AppState>,
    id: String,
    meta: SourceMeta,
) -> Result<NoteSummary> {
    state.with_both(|vault, index| {
        let summary = vault.set_source_meta(&id, meta.clone())?;
        let doc = vault.read_note(&id)?;
        index.upsert(&summary, &doc.body)?;
        Ok(summary)
    })
}

/// Replace a note's citations. Send the complete desired list.
#[tauri::command]
pub fn set_citations(
    state: State<'_, AppState>,
    id: String,
    citations: Vec<Citation>,
) -> Result<NoteSummary> {
    state.with_both(|vault, index| {
        let summary = vault.set_citations(&id, citations.clone())?;
        let doc = vault.read_note(&id)?;
        index.upsert(&summary, &doc.body)?;
        Ok(summary)
    })
}

/// Every source note in the vault.
#[tauri::command]
pub fn list_sources(state: State<'_, AppState>) -> Result<Vec<NoteSummary>> {
    state.with_vault(|vault| vault.list_sources())
}

/// Which notes cite a source, and where in it.
#[tauri::command]
pub fn citing_notes(state: State<'_, AppState>, id: String) -> Result<Vec<CitingNote>> {
    state.with_index(|index| index.citing(&id))
}

/// Bring a Zotero item into the vault as a source note.
///
/// The details are copied in once rather than looked up every time they are
/// shown. That is the difference between a citation that still means something
/// in ten years and one that stops working when Zotero is uninstalled.
#[tauri::command]
pub fn import_zotero_source(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
) -> Result<NoteSummary> {
    let found = crate::state::provider(&app).items(std::slice::from_ref(&key))?;
    let reference = found
        .into_iter()
        .next()
        .ok_or_else(|| SutraError::Zotero(format!("Zotero has no item {key}")))?;

    state.with_both(|vault, index| {
        let summary = vault.import_source(&reference.title, reference.to_source())?;
        cache_current_style(&app, vault, &summary.id, &key);
        let doc = vault.read_note(&summary.id)?;
        index.upsert(&summary, &doc.body)?;
        Ok(summary)
    })
}

/// Create a literature note from a reference-manager item.
///
/// Two notes come out of this, deliberately. The *source* note holds the
/// paper's details, cached so they survive Zotero being closed or uninstalled.
/// The *literature* note holds the reading of it, cites the source, and starts
/// as empty headings. Keeping them apart is what lets the question "where did
/// this come from?" always have an answer: every claim in the literature note
/// sits under a heading the researcher wrote, and every bibliographic fact
/// sits on a source note that names the item it was copied from.
///
/// The abstract is asked of the provider and passed through untouched. If the
/// provider has none, the note simply has none: nothing here composes one.
#[tauri::command]
pub fn create_literature_note(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    folder: Option<String>,
) -> Result<NoteSummary> {
    let zotero = crate::state::provider(&app);

    // `detail` also brings collections and attachments; if that half fails —
    // an older Zotero, a permissions oddity — fall back to the plain item
    // rather than refusing to make the note.
    let (reference, collections, pdf) = match zotero.detail(&key) {
        Ok(detail) => {
            let pdf = detail
                .attachments
                .iter()
                .find(|a| a.is_pdf)
                .map(|a| a.title.clone());
            (detail.reference, detail.collections, pdf)
        }
        Err(_) => {
            let found = zotero.items(std::slice::from_ref(&key))?;
            let reference = found
                .into_iter()
                .next()
                .ok_or_else(|| SutraError::Zotero(format!("Zotero has no item {key}")))?;
            (reference, Vec::new(), None)
        }
    };

    let mut meta = reference.to_source();
    meta.collections = collections;
    meta.pdf = pdf;
    let abstract_text = reference.abstract_text.clone();
    let title = reference.title.clone();

    state.with_both(|vault, index| {
        let source = vault.import_source(&title, meta.clone())?;
        cache_current_style(&app, vault, &source.id, &key);
        let source_doc = vault.read_note(&source.id)?;
        index.upsert(&source, &source_doc.body)?;

        let doc = vault.create_literature_note(
            &title,
            folder.clone(),
            &source.id,
            abstract_text.as_deref(),
        )?;
        index.upsert(&doc.summary, &doc.body)?;
        Ok(doc.summary)
    })
}

/// How the app is set in type.
#[tauri::command]
pub fn typography(app: AppHandle) -> crate::typography::Typography {
    crate::state::typography(&app)
}

/// Save the typography. Sizes are clamped to a readable range on the way in.
#[tauri::command]
pub fn set_typography(
    app: AppHandle,
    typography: crate::typography::Typography,
) -> crate::typography::Typography {
    crate::state::set_typography(&app, typography)
}

/// Bring a font file in from outside.
///
/// Copied rather than referenced, for the same reason attachments are: a font
/// that lives somewhere else on the disk stops working the moment that folder
/// moves, and there would be no way to say why. The picker opens on the Rust
/// side, so no filesystem path crosses the boundary here either.
#[tauri::command]
pub fn import_font(
    app: AppHandle,
    family: String,
) -> Result<Option<crate::typography::Typography>> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter("Fonts", &["woff2", "woff", "ttf", "otf"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = file
        .into_path()
        .map_err(|e| SutraError::NotADirectory(e.to_string()))?;
    let dir = crate::state::font_dir(&app)
        .ok_or_else(|| SutraError::NotADirectory("no config directory".into()))?;

    let added = crate::typography::import(&dir, &path, &family)?;
    let mut settings = crate::state::typography(&app);
    // Replacing a family rather than stacking a second copy of it: importing
    // the same font twice is a correction, not a collection.
    settings.fonts.retain(|f| f.family != added.family);
    settings.fonts.push(added);
    Ok(Some(crate::state::set_typography(&app, settings)))
}

/// Forget an imported font. The file is deleted; nothing else refers to it.
#[tauri::command]
pub fn remove_font(app: AppHandle, family: String) -> crate::typography::Typography {
    let mut settings = crate::state::typography(&app);
    if let Some(dir) = crate::state::font_dir(&app) {
        for font in settings.fonts.iter().filter(|f| f.family == family) {
            let _ = std::fs::remove_file(dir.join(&font.file));
        }
    }
    settings.fonts.retain(|f| f.family != family);
    // A family that was in use and is now gone falls back to the app's own
    // font rather than leaving a name nothing can resolve.
    if settings.reading == family {
        settings.reading.clear();
    }
    if settings.interface == family {
        settings.interface.clear();
    }
    crate::state::set_typography(&app, settings)
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

// ---- views -------------------------------------------------------------------

/// Every view note in the vault.
#[tauri::command]
pub fn list_views(state: State<'_, AppState>) -> Result<Vec<NoteSummary>> {
    state.with_vault(|vault| vault.list_views())
}

/// The query a view note holds, or `None` if it holds none.
#[tauri::command]
pub fn read_view(state: State<'_, AppState>, id: String) -> Result<Option<Query>> {
    state.with_vault(|vault| vault.view_query(&id))
}

/// Run a query and return the notes it matches.
///
/// Takes the query rather than a view's id on purpose: evaluating a view then
/// touches nothing on disk at all, not even the view's own file, and the same
/// command gives the editor a live preview of a query that has not been saved
/// yet — which is what makes building one a matter of seeing the results
/// change rather than guessing.
#[tauri::command]
pub fn run_view(state: State<'_, AppState>, query: Query) -> Result<ViewResult> {
    state.with_index(|index| index.run_view(&query))
}

/// Create a view note holding this query.
#[tauri::command]
pub fn create_view(state: State<'_, AppState>, title: String, query: Query) -> Result<NoteDoc> {
    state.with_both(|vault, index| {
        let doc = vault.create_view(&title, query.clone())?;
        index.upsert(&doc.summary, &doc.body)?;
        Ok(doc)
    })
}

/// Replace a view note's query.
#[tauri::command]
pub fn save_view(state: State<'_, AppState>, id: String, query: Query) -> Result<NoteSummary> {
    state.with_both(|vault, index| {
        let summary = vault.set_view_query(&id, query.clone())?;
        let doc = vault.read_note(&id)?;
        index.upsert(&summary, &doc.body)?;
        Ok(summary)
    })
}

// ---- context -----------------------------------------------------------------

/// Notes near this one, each with a line saying why.
///
/// The body comes from the frontend rather than being re-read here, so the
/// panel reflects what is on screen — including edits not yet saved. Asking a
/// question about the open note must not depend on whether autosave has run.
#[tauri::command]
pub fn related_notes(
    state: State<'_, AppState>,
    id: String,
    body: String,
    limit: usize,
) -> Result<Vec<Related>> {
    state.with_index(|index| index.related(&id, &body, limit))
}

/// The other notes in a note's folder.
///
/// Its own list rather than a relatedness signal: sitting in the same folder
/// is a fact about filing, not about subject, and mixing it into a ranking
/// would let a folder of forty notes crowd out everything computed.
#[tauri::command]
pub fn folder_neighbours(
    state: State<'_, AppState>,
    id: String,
    limit: usize,
) -> Result<Vec<NoteSummary>> {
    state.with_index(|index| index.folder_neighbours(&id, limit))
}

// ---- duplicates and disagreements --------------------------------------------

/// Notes that may be this one written twice.
///
/// Never acted on here: this returns candidates and a sentence about each, and
/// every consequence is a button somebody presses.
#[tauri::command]
pub fn duplicates_of(
    state: State<'_, AppState>,
    id: String,
    title: String,
    body: String,
    limit: usize,
) -> Result<Vec<Duplicate>> {
    state.with_both(|vault, index| {
        let dismissed = vault.dismissed_duplicates(&id).unwrap_or_default();
        index.duplicates(&id, &title, &body, &dismissed, limit)
    })
}

/// Every pair in the vault that may be duplicates. The tidying pass.
#[tauri::command]
pub fn duplicate_pairs(state: State<'_, AppState>, limit: usize) -> Result<Vec<DuplicatePair>> {
    state.with_index(|index| index.duplicate_pairs(limit))
}

/// Record that two notes are not duplicates, so neither is offered again.
#[tauri::command]
pub fn not_duplicates(state: State<'_, AppState>, a: String, b: String) -> Result<()> {
    state.with_vault(|vault| vault.not_duplicates(&a, &b))
}

/// Fold one note into another and send the absorbed one to the trash.
///
/// The index is rebuilt rather than patched: a merge rewrites links across the
/// vault, and reconstructing from the markdown afterwards is both simpler than
/// tracking what changed and the thing that proves the index is derived.
#[tauri::command]
pub fn merge_notes(
    state: State<'_, AppState>,
    keep: String,
    absorb: String,
) -> Result<NoteSummary> {
    state.with_both(|vault, index| {
        let summary = vault.merge_notes(&keep, &absorb)?;
        index.rebuild(vault)?;
        Ok(summary)
    })
}

/// Numeric claims in this note that differ from one in a connected note.
///
/// "Differ", not "contradict". Which is right, or whether they are even about
/// the same measurement, is not knowable from the text and is not claimed.
#[tauri::command]
pub fn disagreements(
    state: State<'_, AppState>,
    id: String,
    body: String,
    limit: usize,
) -> Result<Vec<Disagreement>> {
    state.with_index(|index| index.disagreements(&id, &body, limit))
}

// ---- optional AI -------------------------------------------------------------

/// Whether assistance is switched on, and what it would use.
///
/// Never returns the key. The UI needs to know that one is stored, not what it
/// is: a credential that is only ever written cannot leak through a screenshot
/// or a devtools panel.
#[tauri::command]
pub fn ai_status(app: AppHandle) -> AiStatus {
    state::ai_status(&app)
}

/// Switch assistance on or off, and set the key and model.
///
/// An empty key clears the stored one rather than storing an empty string, so
/// "remove my key" is expressible.
#[tauri::command]
pub fn set_ai_settings(
    app: AppHandle,
    enabled: bool,
    api_key: Option<String>,
    model: Option<String>,
) -> AiStatus {
    state::set_ai_settings(
        &app,
        AiSettings {
            enabled,
            api_key: api_key.filter(|k| !k.trim().is_empty()),
            model: model.filter(|m| !m.trim().is_empty()),
        },
    );
    state::ai_status(&app)
}

/// Ask for a suggestion about the open note.
///
/// Returns a value and changes nothing. There is no id in the reply, no path,
/// and no write anywhere on this path — accepting a suggestion goes through
/// `save_note` or `set_note_meta` like anything a person types, which is what
/// makes "the AI may not write to a file" a fact about the code rather than a
/// promise.
///
/// The body comes from the frontend so the suggestion is about what is on
/// screen, including edits autosave has not written yet.
#[tauri::command]
pub fn ai_suggest(
    app: AppHandle,
    state: State<'_, AppState>,
    task: Task,
    title: String,
    body: String,
) -> Result<Draft> {
    // Gathered before the assistant is even built, and it is all that leaves
    // the machine: one note, the vault's tag list when tags are being asked
    // for, and the source ids the citation rule is checked against.
    let ask = state.with_vault(|vault| {
        let mut vault_tags: Vec<(String, usize)> = if task == Task::Tags {
            vault.list_tags()?.into_iter().collect()
        } else {
            Vec::new()
        };
        vault_tags.sort_by(|(a_tag, a), (b_tag, b)| b.cmp(a).then_with(|| a_tag.cmp(b_tag)));
        Ok(Ask {
            task,
            title,
            body,
            vault_tags: vault_tags.into_iter().map(|(tag, _)| tag).collect(),
            known_sources: vault.list_sources()?.into_iter().map(|s| s.id).collect(),
        })
    })?;

    state.assistant(&app).respond(&ask)
}
