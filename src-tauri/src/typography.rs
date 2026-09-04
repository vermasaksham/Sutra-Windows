//! How the app is set in type, and fonts brought in from outside.
//!
//! Two independent choices, kept apart because they fail differently. The
//! *interface* font is the rail, the list and the menus; the *reading* font is
//! the note itself. A person who wants a serif to write in rarely wants their
//! sidebar in one, and forcing the pair to move together is why so many
//! editors are set in a font nobody chose.
//!
//! A font named here need not exist. Every family falls back through a stack
//! ending in the system default, so a vault opened on a machine without
//! Cambria renders in something rather than nothing — which is also what makes
//! it safe to accept a family name typed by hand.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, SutraError};

/// Where imported fonts live, under the app's config directory.
///
/// Beside `sutra.json` rather than in the vault, deliberately: a font is a
/// property of this screen, not of the notes. Syncing a vault between a
/// desktop and a laptop should not drag one machine's typeface onto the other,
/// for the same reason it does not drag the theme.
pub const FONT_DIR: &str = "fonts";

/// The extensions a webview can actually render.
const ALLOWED: &[&str] = &["woff2", "woff", "ttf", "otf"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Typography {
    /// CSS family name for the note body. Empty means the app default.
    #[serde(default)]
    pub reading: String,
    /// CSS family name for the surrounding interface.
    #[serde(default)]
    pub interface: String,
    /// Body size in px. Clamped on the way in — a note set at 4px is a note
    /// nobody can read their way back out of to fix the setting.
    #[serde(default = "default_size")]
    pub size: f32,
    #[serde(default = "default_leading")]
    pub leading: f32,
    /// The measure, in px. Wide enough to matter, narrow enough to read.
    #[serde(default = "default_width")]
    pub width: f32,
    /// Fonts imported from a file, in the order they were added.
    #[serde(default)]
    pub fonts: Vec<CustomFont>,
}

/// One font file brought in from outside.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFont {
    /// What to call it in CSS and in the picker.
    pub family: String,
    /// The file's name under the fonts directory. Not a path: the webview is
    /// never handed one, here or anywhere else in this app.
    pub file: String,
}

fn default_size() -> f32 {
    16.0
}

fn default_leading() -> f32 {
    1.65
}

fn default_width() -> f32 {
    700.0
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            reading: String::new(),
            interface: String::new(),
            size: default_size(),
            leading: default_leading(),
            width: default_width(),
            fonts: Vec::new(),
        }
    }
}

impl Typography {
    /// Bring the numbers back into a range a person can read and undo.
    ///
    /// Applied on the way in rather than trusted from the config file, because
    /// that file is hand-editable and a typo in it should not produce a window
    /// whose settings panel is too small to find.
    pub fn clamped(mut self) -> Self {
        self.size = self.size.clamp(11.0, 28.0);
        self.leading = self.leading.clamp(1.2, 2.4);
        self.width = self.width.clamp(480.0, 1200.0);
        self.reading = clean_family(&self.reading);
        self.interface = clean_family(&self.interface);
        self
    }
}

/// Strip what would break out of a CSS `font-family` value.
///
/// The name reaches a stylesheet, so a quote or a semicolon in it would end the
/// declaration and begin something else. Everything outside the set a font name
/// actually uses is dropped rather than escaped — there is no legitimate family
/// called `Inter"; color: red`.
pub fn clean_family(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Whether this file is one the webview can load.
pub fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| ALLOWED.contains(&e.as_str()))
}

/// The content type for a font file, by extension.
pub fn font_content_type(name: &str) -> &'static str {
    match name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => "application/octet-stream",
    }
}

/// A file name that cannot escape the fonts directory.
///
/// Built from the family rather than taken from the source path: a file called
/// `../../sutra.json` is a real thing to be handed, and the answer is to not
/// use the given name at all.
pub fn stored_name(family: &str, extension: &str) -> String {
    let stem: String = clean_family(family)
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "font" } else { stem };
    format!("{stem}-{}.{extension}", ulid::Ulid::generate())
}

/// Copy a font file into the app's font directory.
pub fn import(dir: &Path, source: &Path, family: &str) -> Result<CustomFont> {
    if !is_font_file(source) {
        return Err(SutraError::NotADirectory(
            "that is not a font file — Sutra reads .woff2, .woff, .ttf and .otf".into(),
        ));
    }
    let family = clean_family(family);
    if family.is_empty() {
        return Err(SutraError::NotADirectory("give the font a name".into()));
    }

    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("ttf")
        .to_ascii_lowercase();
    let file = stored_name(&family, &extension);

    std::fs::create_dir_all(dir)?;
    std::fs::copy(source, dir.join(&file))?;
    Ok(CustomFont { family, file })
}

/// Read one imported font back, refusing anything that is not a plain name.
///
/// The webview asks for `sutra://localhost/fonts/<file>`, and `<file>` arrives
/// from a stylesheet this app wrote — but it is still input, and a request for
/// `../sutra.json` would otherwise hand over the API key.
pub fn read(dir: &Path, file: &str) -> Result<Vec<u8>> {
    let name = Path::new(file);
    let mut parts = name.components();
    let Some(std::path::Component::Normal(only)) = parts.next() else {
        return Err(SutraError::NotADirectory("not a font".into()));
    };
    if parts.next().is_some() {
        return Err(SutraError::NotADirectory("not a font".into()));
    }
    if !is_font_file(Path::new(only)) {
        return Err(SutraError::NotADirectory("not a font".into()));
    }
    Ok(std::fs::read(dir.join(only))?)
}

/// The fonts directory under a config directory.
pub fn dir_in(config: &Path) -> PathBuf {
    config.join(FONT_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_family_name_cannot_escape_its_css_declaration() {
        // The name is written into a stylesheet, so a quote or a semicolon
        // would end the declaration and start another one.
        assert_eq!(clean_family("Inter"), "Inter");
        assert_eq!(clean_family("  Source Sans 3  "), "Source Sans 3");
        assert_eq!(clean_family("Inter\"; color: red"), "Inter color red");
        assert_eq!(clean_family("a{}<script>"), "ascript");
        assert_eq!(clean_family(""), "");
    }

    #[test]
    fn the_stored_name_is_built_rather_than_taken() {
        // A file offered as `../../sutra.json` is a real thing to be handed.
        // The source name is never used, so there is nothing to sanitise.
        let name = stored_name("Source Serif 4", "woff2");
        assert!(name.starts_with("source-serif-4-"), "got {name}");
        assert!(name.ends_with(".woff2"), "got {name}");
        assert!(!name.contains('/') && !name.contains('\\'));

        // Two imports of the same family do not collide.
        assert_ne!(stored_name("X", "ttf"), stored_name("X", "ttf"));
        // A family with nothing usable in it still yields a legal name.
        assert!(stored_name("///", "otf").starts_with("font-"));
    }

    #[test]
    fn only_fonts_are_accepted() {
        assert!(is_font_file(Path::new("a.woff2")));
        assert!(is_font_file(Path::new("a.TTF")));
        assert!(!is_font_file(Path::new("a.exe")));
        assert!(!is_font_file(Path::new("a")));
    }

    #[test]
    fn reading_refuses_anything_that_is_not_a_bare_font_name() {
        let dir = std::env::temp_dir();
        // The API key lives in the directory above this one.
        assert!(read(&dir, "../sutra.json").is_err());
        assert!(read(&dir, "a/b.ttf").is_err());
        assert!(read(&dir, "sutra.json").is_err());
        assert!(read(&dir, "/etc/passwd").is_err());
    }

    #[test]
    fn unreadable_settings_are_brought_back_into_range() {
        // This file is hand-editable, and a typo must not produce a window
        // whose settings panel is too small to read your way back out of.
        let mad = Typography {
            size: 2.0,
            leading: 12.0,
            width: 40.0,
            ..Default::default()
        }
        .clamped();
        assert_eq!(mad.size, 11.0);
        assert_eq!(mad.leading, 2.4);
        assert_eq!(mad.width, 480.0);
    }

    #[test]
    fn the_defaults_match_what_the_app_shipped_with() {
        // Changing these silently restyles every existing vault.
        let d = Typography::default();
        assert_eq!(d.size, 16.0);
        assert_eq!(d.leading, 1.65);
        assert_eq!(d.width, 700.0);
        assert!(d.reading.is_empty(), "empty means the app's own font");
    }

    #[test]
    fn font_content_types_come_from_the_extension() {
        assert_eq!(font_content_type("a.woff2"), "font/woff2");
        assert_eq!(font_content_type("a.OTF"), "font/otf");
        assert_eq!(font_content_type("a.exe"), "application/octet-stream");
    }
}
