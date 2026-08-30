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

/// Names Windows refuses outright, with or without an extension.
///
/// `CON.md` is not a file you can create on Windows. These are historical
/// device names and the restriction is still enforced.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Turn a title into a filename stem a person would recognise.
///
/// Deliberately *not* `slugify`. The whole point of a real folder tree is that
/// it reads well in Explorer or Finder, and `Sb2Se3 Cp.md` reads better than
/// `sb2se3-cp.md`. So spaces and case survive; only what the filesystem
/// actually refuses is removed.
///
/// Still lossy, and still never round-trips — the title of record is in
/// frontmatter. Two notes in one folder can want the same stem, which the
/// caller resolves by adding a suffix.
pub fn file_stem(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_space = false;

    for ch in title.chars() {
        if ch.is_whitespace() {
            // Collapse runs of whitespace, and only emit once real text
            // follows, which trims the trailing space for free.
            pending_space = !out.is_empty();
        } else if ch.is_control() || FORBIDDEN.contains(&ch) {
            continue;
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(ch);
        }
    }

    if out.chars().count() > MAX_SLUG {
        out = out.chars().take(MAX_SLUG).collect();
    }
    // Windows silently strips trailing dots and spaces, which would make the
    // name on disk disagree with the one we think we wrote.
    let mut out = out.trim_end_matches(['.', ' ']).to_string();

    if out.is_empty() {
        out = "Untitled".to_string();
    }
    // A reserved name is only reserved as the whole stem, so a suffix is enough.
    if RESERVED
        .iter()
        .any(|r| r.eq_ignore_ascii_case(out.split('.').next().unwrap_or(&out)))
    {
        out.push('_');
    }
    out
}

/// `<stem>.md`
///
/// The id is no longer here. It lives in the frontmatter, which is what lets a
/// note be renamed or moved without a single link changing.
pub fn file_name(title: &str) -> String {
    format!("{}.md", file_stem(title))
}

/// Crockford base32 — the alphabet ULIDs use. No I, L, O or U, so nothing
/// reads as a different character.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A stable id for a file that has no frontmatter.
///
/// Adopting a stray `.md` used to be easy: its id was in its name. Now the id
/// lives inside the file, and a file without frontmatter has none — so one is
/// derived from its path instead. Deterministic, so the same file keeps the
/// same id from one listing to the next and the editor can open it; replaced
/// by a real ULID the first time the note is saved.
///
/// Moving such a file changes its id, which is acceptable precisely because
/// nothing can link to a note whose id was never written down.
pub fn adopted_id(relative: &str) -> String {
    let bits = ((fnv1a(relative.as_bytes(), 0xcbf2_9ce4_8422_2325) as u128) << 64)
        | fnv1a(relative.as_bytes(), 0x9e37_79b9_7f4a_7c15) as u128;

    let mut buf = [0u8; 26];
    let mut n = bits;
    for slot in buf.iter_mut().rev() {
        *slot = CROCKFORD[(n & 0x1f) as usize];
        n >>= 5;
    }
    // Every byte came from CROCKFORD, so this is ASCII by construction.
    String::from_utf8(buf.to_vec()).unwrap_or_else(|_| "0".repeat(26))
}

/// FNV-1a, 64-bit. Not cryptographic and does not need to be — this only has
/// to be stable across runs and spread paths out.
fn fnv1a(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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
        // `CON.md` is not a file Windows will create, whatever the extension.
        assert_eq!(file_name("CON"), "CON_.md");
        assert_eq!(file_name("nul"), "nul_.md");
        // Only as the whole stem, though.
        assert_eq!(file_name("CONduction"), "CONduction.md");
    }

    #[test]
    fn a_note_filename_keeps_spaces_and_case() {
        // The folder tree is meant to be read in Explorer, so titles are not
        // slugged the way attachment names are.
        assert_eq!(file_name("Sb2Se3 Cp"), "Sb2Se3 Cp.md");
        assert_eq!(file_name("Zhou 2019 — ribbons"), "Zhou 2019 — ribbons.md");
    }

    #[test]
    fn a_note_filename_drops_what_the_filesystem_refuses() {
        assert_eq!(file_name("Cp: 300/800 K?"), "Cp 300800 K.md");
        assert_eq!(file_name("   "), "Untitled.md");
        assert_eq!(file_name("trailing.  "), "trailing.md");
    }

    #[test]
    fn an_adopted_id_is_deterministic_and_ulid_shaped() {
        let a = adopted_id("Research/Sb2Se3/Cp.md");
        assert_eq!(a, adopted_id("Research/Sb2Se3/Cp.md"), "must be stable");
        assert_ne!(a, adopted_id("Research/SbSeI/Cp.md"));
        assert_eq!(a.chars().count(), 26);
        assert!(
            a.bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase()),
            "{a} is not ULID-shaped"
        );
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
