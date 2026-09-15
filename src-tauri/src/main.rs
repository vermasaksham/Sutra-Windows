// Without this, launching the release build on Windows pops a console window
// behind the app. It applies only on Windows release builds; every other
// target ignores it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod attachments;
/// The performance baseline. Test-only: see benchmarks.rs for what it measures
/// and why the ceilings are shaped the way they are.
#[cfg(test)]
mod benchmarks;
mod citations;
mod claims;
mod commands;
mod duplicates;
mod error;
mod evidence;
mod export;
mod frontmatter;
mod index;
mod links;
mod note;
mod pdfcache;
mod pdfread;
mod pdftext;
mod protocol;
mod references;
mod related;
mod secrets;
mod state;
mod tags;
mod typography;
mod updates;
mod vault;
mod views;
mod watcher;
mod zotero;

use state::AppState;
use tauri::Manager;

fn main() {
    // Before anything else, because this process may not be an application run
    // at all. Extraction re-invokes this same binary with a hidden flag so that
    // a panic in the PDF parser — and `panic = "abort"` makes any panic fatal —
    // kills a child process instead of the window someone is writing in. See
    // pdftext.rs. A normal launch has no such argument and falls straight
    // through.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(pdftext::CHILD_FLAG) {
        std::process::exit(pdftext::run_as_child(&args[2..]));
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Serves vault attachments without ever handing a path to the webview.
        // See protocol.rs for why this exists instead of Tauri's asset
        // protocol.
        .register_uri_scheme_protocol("sutra", protocol::serve)
        // `manage` hands a value to Tauri to own for the life of the app.
        // Commands then ask for it by type via `State<'_, AppState>` — there is
        // no global variable, and no way to get one of the wrong type.
        .manage(AppState::default())
        .setup(|app| {
            // Reopen last session's vault before the window appears, so the
            // frontend's first `current_vault` call already has the answer.
            let handle = app.handle().clone();
            let vault_state = app.state::<AppState>();
            state::restore_last_vault(&handle, &vault_state);
            Ok(())
        })
        // `generate_handler!` builds the lookup table from command name to
        // function. A command that is not listed here does not exist as far as
        // the frontend is concerned.
        .invoke_handler(tauri::generate_handler![
            commands::research_overview,
            commands::check_for_updates,
            commands::app_version,
            commands::open_release_page,
            commands::pick_vault,
            commands::current_vault,
            commands::list_notes,
            commands::search_notes,
            commands::backlinks,
            commands::reindex,
            commands::read_note,
            commands::create_note,
            commands::save_note,
            commands::set_note_meta,
            commands::delete_note,
            commands::export_docx,
            commands::zotero_search,
            commands::zotero_by_keys,
            commands::reference_status,
            commands::reference_config,
            commands::configure_references,
            commands::connect_zotero_account,
            commands::typography,
            commands::set_typography,
            commands::import_font,
            commands::remove_font,
            commands::restyle_sources,
            commands::zotero_detail,
            commands::zotero_open,
            commands::attach_file,
            commands::move_note,
            commands::list_folders,
            commands::create_folder,
            commands::migration_needed,
            commands::id_clashes,
            commands::migration_plan,
            commands::migrate_vault,
            commands::capture,
            commands::set_note_type,
            commands::list_tags,
            commands::similar_tags,
            commands::retag,
            commands::undo_retag,
            commands::create_source,
            commands::set_source_meta,
            commands::set_citations,
            commands::share_evidence,
            commands::note_evidence,
            commands::all_evidence,
            commands::list_sources,
            commands::citing_notes,
            commands::import_zotero_source,
            commands::create_literature_note,
            commands::extract_vault_pdf,
            commands::extract_zotero_pdf,
            commands::clear_pdf_text_cache,
            commands::zotero_annotations,
            commands::capture_annotations,
            commands::legacy_citations,
            commands::migrate_citations,
            commands::list_views,
            commands::read_view,
            commands::run_view,
            commands::create_view,
            commands::save_view,
            commands::list_chapters,
            commands::create_chapter,
            commands::read_chapter,
            commands::save_sequence,
            commands::chapter_sections,
            commands::chapters_using,
            commands::related_notes,
            commands::folder_neighbours,
            commands::duplicates_of,
            commands::duplicate_pairs,
            commands::not_duplicates,
            commands::merge_notes,
            commands::disagreements,
            commands::ai_status,
            commands::set_ai_settings,
            commands::ai_suggest,
        ])
        // `generate_context!` pulls in tauri.conf.json at compile time.
        .run(tauri::generate_context!())
        // `.run` returns a Result. `.expect` unwraps the Ok value or crashes
        // with this message on Err. That is the right call here: if the window
        // cannot be created there is no app left to run.
        .expect("failed to start Sutra");
}
