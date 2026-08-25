// Without this, launching the release build on Windows pops a console window
// behind the app. It applies only on Windows release builds; every other
// target ignores it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod frontmatter;
mod index;
mod links;
mod note;
mod state;
mod vault;
mod watcher;

use state::AppState;
use tauri::Manager;

/// A Tauri command: a Rust function the frontend can call by name.
///
/// The `#[tauri::command]` macro generates the glue that receives the IPC
/// message, runs this function, and serialises what it returns back to
/// JavaScript. On the frontend it is reached with
/// `invoke<string>("app_version")`.
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            app_version,
            commands::pick_vault,
            commands::current_vault,
            commands::list_notes,
            commands::search_notes,
            commands::backlinks,
            commands::note_titles,
            commands::reindex,
            commands::read_note,
            commands::create_note,
            commands::save_note,
            commands::delete_note,
            commands::attach_file,
        ])
        // `generate_context!` pulls in tauri.conf.json at compile time.
        .run(tauri::generate_context!())
        // `.run` returns a Result. `.expect` unwraps the Ok value or crashes
        // with this message on Err. That is the right call here: if the window
        // cannot be created there is no app left to run.
        .expect("failed to start Sutra");
}
