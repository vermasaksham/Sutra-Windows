// Runs before the crate compiles. tauri-build reads tauri.conf.json, generates
// the permission/capability schemas, and (on Windows) embeds the app icon and
// manifest into the executable. If the config is malformed, this is what fails.
fn main() {
    tauri_build::build()
}
