// Without this, launching the release build on Windows pops a console window
// behind the app. It applies only on Windows release builds; every other
// target ignores it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// A Tauri command: a Rust function the frontend can call by name.
///
/// The `#[tauri::command]` macro generates the glue that receives the IPC
/// message, runs this function, and serialises what it returns back to
/// JavaScript. On the frontend it is reached with
/// `invoke<string>("app_version")`.
///
/// A note on the return type, since this is the first Rust here: `String` is an
/// owned, heap-allocated, growable string. `env!("CARGO_PKG_VERSION")` is a
/// `&'static str` — a borrowed view into text baked into the binary at compile
/// time — so `.to_string()` copies it into a `String` we own and can hand off.
/// We must return an owned value because the borrowed one cannot outlive this
/// function call.
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn main() {
    tauri::Builder::default()
        // `generate_handler!` builds the lookup table from command name to
        // function. A command that is not listed here does not exist as far as
        // the frontend is concerned.
        .invoke_handler(tauri::generate_handler![app_version])
        // `generate_context!` pulls in tauri.conf.json at compile time.
        .run(tauri::generate_context!())
        // `.run` returns a Result. `.expect` unwraps the Ok value or crashes
        // with this message on Err. That is the right call here: if the window
        // cannot be created there is no app left to run.
        .expect("failed to start Sutra");
}
