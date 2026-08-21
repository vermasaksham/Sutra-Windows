//! Note filenames, and getting bytes onto disk safely.

use crate::error::Result;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Longest slug we will put in a filename. Windows still has a 260-character
/// path limit unless long paths are enabled, and a vault sitting under a deep
/// `Documents\...` tree eats a lot of that before we add anything.
const MAX_SLUG: usize = 60;

/// Characters Windows refuses in a filename, plus the separators.
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Turn a note title into the human-readable half of its filename.
///
/// This is lossy on purpose and never round-trips — the title of record lives
/// in frontmatter. All this has to do is produce something legal, stable, and
/// recognisable in a file browser.
pub fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut pending_dash = false;

    for ch in title.chars() {
        if ch.is_whitespace() || ch == '-' || ch == '_' {
            // Collapse any run of spacing into a single dash, but only emit it
            // once we know real text follows — that trims trailing dashes for
            // free instead of needing a second pass.
            pending_dash = !slug.is_empty();
        } else if ch.is_control() || FORBIDDEN.contains(&ch) {
            // Dropped entirely rather than replaced, so "a/b" reads "ab" and
            // not "a-b" — a slash is not a word break.
            continue;
        } else {
            if pending_dash {
                slug.push('-');
                pending_dash = false;
            }
            slug.push(ch);
        }
    }

    // Truncate on a character boundary; slicing bytes would panic on any
    // multi-byte character, and titles here will contain them.
    if slug.chars().count() > MAX_SLUG {
        slug = slug.chars().take(MAX_SLUG).collect();
    }
    // Windows silently strips trailing dots and spaces from filenames, which
    // would make our on-disk name disagree with the one we think we wrote.
    let slug = slug.trim_end_matches(['.', '-']).to_string();

    if slug.is_empty() {
        "untitled".to_string()
    } else {
        slug
    }
}

/// `<slug>_<ULID>.md`
pub fn file_name(title: &str, id: &str) -> String {
    format!("{}_{}.md", slugify(title), id)
}

/// Recover a note's id from its filename: the part after the last `_`.
///
/// Returns `None` for anything that is not a note we wrote, which is how
/// directory scans skip `README.md` and similar.
pub fn id_from_file_name(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".md")?;
    let (_, id) = stem.rsplit_once('_')?;
    // A ULID is 26 characters of Crockford base32. Checking the shape stops us
    // adopting `my_notes.md` as a note with the id "notes".
    let looks_like_ulid = id.len() == 26
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase());
    looks_like_ulid.then_some(id)
}

/// Write `contents` to `path` atomically.
///
/// The sequence is: write a temp file in the *same directory*, flush it to the
/// disk itself, then rename it over the target.
///
/// Same directory matters. A rename within one filesystem is a metadata
/// operation the OS guarantees is atomic — a concurrent reader sees either the
/// whole old file or the whole new one. A rename across volumes is secretly a
/// copy, and copies can be interrupted halfway.
///
/// `sync_all` is the part people leave out. Without it the rename can reach
/// the disk before the file contents do, and a power cut in that window leaves
/// a note that exists but is empty. With it, the worst case is a leftover temp
/// file and the previous version of the note intact.
///
/// `fs::rename` replaces an existing destination on Windows as well as Unix —
/// std uses `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING` — so this does not
/// need a delete-then-rename dance, which would open a window where the note
/// does not exist at all.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(directory)?;

    let temp = temp_path(path);

    // A scope so the File is closed before the rename. Windows will not rename
    // a file that still has an open handle.
    {
        let mut file = File::create(&temp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }

    // If the rename fails, clean up rather than littering the vault with temp
    // files. The rename error is the one worth reporting, so the removal's own
    // result is deliberately discarded.
    if let Err(e) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(e.into());
    }

    Ok(())
}

/// A sibling of `path`, dot-prefixed so it is hidden on Unix and sorts out of
/// the way, and ULID-suffixed so two concurrent saves cannot collide.
fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("note");
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    directory.join(format!(".{}.{}.tmp", name, ulid::Ulid::generate()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_readable() {
        assert_eq!(slugify("CVT runs"), "CVT-runs");
        assert_eq!(slugify("Sb2Se3 growth log"), "Sb2Se3-growth-log");
    }

    #[test]
    fn slugs_drop_characters_windows_rejects() {
        assert_eq!(slugify(r#"a/b\c:d*e?f"g<h>i|j"#), "abcdefghij");
    }

    #[test]
    fn slugs_collapse_and_trim_spacing() {
        assert_eq!(slugify("  lots   of   space  "), "lots-of-space");
        assert_eq!(slugify("trailing---"), "trailing");
    }

    #[test]
    fn slugs_never_end_in_a_dot() {
        // Windows strips these silently, so we must strip them first.
        assert_eq!(slugify("version 1.0."), "version-1.0");
    }

    #[test]
    fn slugs_are_never_empty() {
        assert_eq!(slugify(""), "untitled");
        assert_eq!(slugify("///"), "untitled");
        assert_eq!(slugify("   "), "untitled");
    }

    #[test]
    fn slugs_truncate_on_a_character_boundary() {
        // Multi-byte characters: slicing by bytes here would panic.
        let slug = slugify(&"é".repeat(200));
        assert_eq!(slug.chars().count(), MAX_SLUG);
    }

    #[test]
    fn reserved_windows_names_gain_a_suffix() {
        // CON and NUL are illegal alone but fine with the ULID appended.
        let name = file_name("CON", "01HQ3M8K2P0000000000000000");
        assert_eq!(name, "CON_01HQ3M8K2P0000000000000000.md");
    }

    #[test]
    fn ids_come_back_out_of_filenames() {
        let id = "01HQ3M8K2P0000000000000000";
        let name = file_name("Some title", id);
        assert_eq!(id_from_file_name(&name), Some(id));
    }

    #[test]
    fn foreign_files_are_not_mistaken_for_notes() {
        assert_eq!(id_from_file_name("README.md"), None);
        assert_eq!(id_from_file_name("my_notes.md"), None);
        assert_eq!(id_from_file_name("notes.txt"), None);
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_files() {
        let dir = std::env::temp_dir().join(format!("sutra-test-{}", ulid::Ulid::generate()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.md");

        write_atomic(&path, "first").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");

        write_atomic(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        fs::remove_dir_all(&dir).unwrap();
    }
}
