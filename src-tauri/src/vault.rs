//! The vault: a directory of markdown files, and the operations on it.

use crate::error::{Result, SutraError};
use crate::frontmatter::{self, Frontmatter};
use crate::note;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use ulid::Ulid;

/// Sidecar directories. Notes themselves stay flat in the root — hierarchy is
/// frontmatter, never folders — but deleted notes and attachments need
/// somewhere to live that is not the note namespace.
const ATTACHMENTS: &str = "attachments";
const TRASH: &str = "trash";

/// A note's metadata without its body. This is what the sidebar needs, and
/// loading bodies for a whole vault to draw a tree would be wasteful.
#[derive(Debug, Clone, Serialize)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub parent: Option<String>,
    pub position: i64,
    pub tags: Vec<String>,
    pub icon: Option<String>,
    pub cover: Option<String>,
    /// The opening prose, for the list to show beneath the title.
    pub excerpt: String,
    #[serde(with = "time::serde::rfc3339")]
    pub updated: OffsetDateTime,
}

/// A note with its body, ready for the editor.
///
/// Note what is *not* here: no path. The frontend addresses notes by id and
/// never learns where the vault sits on disk.
#[derive(Debug, Clone, Serialize)]
pub struct NoteDoc {
    #[serde(flatten)]
    pub summary: NoteSummary,
    pub body: String,
    /// Set when the file on disk had no frontmatter and we adopted it. The UI
    /// can mention that the note has been taken over on first save.
    pub adopted: bool,
}

pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open a directory as a vault, creating the sidecar folders if needed.
    ///
    /// Takes `PathBuf` by value rather than `&Path` because the Vault stores it
    /// — asking for ownership up front is honest about that, and saves the
    /// caller from a clone they would otherwise have to make anyway.
    pub fn open(root: PathBuf) -> Result<Self> {
        if !root.is_dir() {
            return Err(SutraError::NotADirectory(root.display().to_string()));
        }
        fs::create_dir_all(root.join(ATTACHMENTS))?;
        fs::create_dir_all(root.join(TRASH))?;
        Ok(Self { root })
    }

    /// Only the tests need this. Production code reaches the root through the
    /// methods above, which is the point — nothing outside should be building
    /// paths by hand.
    #[cfg(test)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The name shown in the UI. The full path stays on this side of the
    /// boundary.
    pub fn display_name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.root.display().to_string())
    }

    /// Every note in the vault.
    ///
    /// This is a linear scan, and it re-reads every file. That is fine for now
    /// and deliberately not optimised: Phase 4 puts a SQLite index in front of
    /// it, and the index has to be rebuildable from exactly this scan.
    ///
    /// Unreadable or malformed files are skipped rather than failing the whole
    /// listing — one corrupt note must not make the vault unopenable.
    pub fn list_notes(&self) -> Result<Vec<NoteSummary>> {
        let mut notes = Vec::new();

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(id) = note::id_from_file_name(name) else {
                continue;
            };
            let Ok(contents) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok((parsed, body)) = frontmatter::split(&contents) else {
                continue;
            };
            let fm = parsed.unwrap_or_else(|| Self::synthesise(id, name));
            notes.push(summary_of(&fm, body));
        }

        // Siblings sort by position, then title, so the order is stable even
        // when positions collide.
        notes.sort_by(|a, b| {
            a.position
                .cmp(&b.position)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        Ok(notes)
    }

    /// Read one note.
    pub fn read_note(&self, id: &str) -> Result<NoteDoc> {
        let path = self.path_for(id)?;
        let contents = fs::read_to_string(&path)?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let adopted = parsed.is_none();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let fm = parsed.unwrap_or_else(|| Self::synthesise(id, name));
        Ok(NoteDoc {
            summary: summary_of(&fm, body),
            body: body.to_string(),
            adopted,
        })
    }

    /// Create an empty note and return it.
    pub fn create_note(&self, title: &str, parent: Option<String>) -> Result<NoteDoc> {
        let id = Ulid::generate().to_string();
        let position = self.next_position(parent.as_deref())?;
        let fm = Frontmatter::new(id.clone(), title.to_string(), parent, position);
        let path = self.root.join(note::file_name(title, &id));
        note::write_atomic(&path, &frontmatter::join(&fm, "")?)?;
        Ok(NoteDoc {
            summary: summary_of(&fm, ""),
            body: String::new(),
            adopted: false,
        })
    }

    /// Save a note's title and body.
    ///
    /// `created` is preserved from whatever is on disk; `updated` is stamped
    /// now. If the title changed the file is renamed, because the slug is part
    /// of the filename — the ULID does not move, so no link can break.
    pub fn save_note(&self, id: &str, title: &str, body: &str) -> Result<NoteSummary> {
        let path = self.path_for(id)?;
        let existing = fs::read_to_string(&path)?;
        let (parsed, _) = frontmatter::split(&existing)?;

        let mut fm = parsed.unwrap_or_else(|| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            Self::synthesise(id, name)
        });
        let renamed = fm.title != title;
        fm.title = title.to_string();
        fm.updated = frontmatter::now();

        let target = if renamed {
            self.root.join(note::file_name(title, id))
        } else {
            path.clone()
        };

        note::write_atomic(&target, &frontmatter::join(&fm, body)?)?;

        // Written first, removed second. The reverse order would leave a window
        // with no file at all, and a crash in that window would lose the note.
        if renamed && target != path {
            fs::remove_file(&path)?;
        }

        Ok(summary_of(&fm, body))
    }

    /// Replace a note's page-level metadata.
    ///
    /// The caller sends the complete desired state rather than a patch. A patch
    /// would need to distinguish "leave this alone" from "set this to null",
    /// which over an IPC boundary means a nested Option and a lot of ceremony
    /// for no benefit — the frontend always has the whole note loaded anyway.
    ///
    /// Icon, cover and tags are page-level, so they belong in frontmatter. The
    /// body is untouched.
    pub fn set_meta(
        &self,
        id: &str,
        icon: Option<String>,
        cover: Option<String>,
        tags: Vec<String>,
    ) -> Result<NoteSummary> {
        let path = self.path_for(id)?;
        let contents = fs::read_to_string(&path)?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let mut fm = parsed.unwrap_or_else(|| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            Self::synthesise(id, name)
        });

        // An empty string means "no icon", not an icon that renders as nothing.
        fm.icon = icon.filter(|i| !i.trim().is_empty());
        fm.cover = cover.filter(|c| !c.trim().is_empty());
        // Tags are normalised here rather than in the UI so that a tag typed
        // in one note matches the same tag typed in another, whatever case or
        // stray whitespace it arrived with.
        fm.tags = normalise_tags(tags);
        fm.updated = frontmatter::now();

        let body = body.to_string();
        note::write_atomic(&path, &frontmatter::join(&fm, &body)?)?;
        Ok(summary_of(&fm, &body))
    }

    /// Move a note to `trash/` rather than unlinking it.
    ///
    /// A rename, so it is atomic and instant regardless of file size, and the
    /// note is recoverable by dragging it back out in Explorer.
    pub fn delete_note(&self, id: &str) -> Result<()> {
        let path = self.path_for(id)?;
        let name = path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        let mut target = self.root.join(TRASH).join(&name);

        // Deleting, restoring, and deleting again must not silently overwrite
        // the first copy.
        if target.exists() {
            target = self.root.join(TRASH).join(format!(
                "{}.{}",
                Ulid::generate(),
                name.to_string_lossy()
            ));
        }
        fs::create_dir_all(self.root.join(TRASH))?;
        fs::rename(&path, &target)?;
        Ok(())
    }

    /// Copy a file into `attachments/` under a ULID-prefixed name, returning
    /// the vault-relative path a note should reference.
    pub fn import_attachment(&self, source: &Path) -> Result<String> {
        let original = source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        // Slug the original name so an attachment can never introduce a
        // separator or a character the filesystem rejects.
        let (stem, extension) = match original.rsplit_once('.') {
            Some((s, e)) => (s, format!(".{}", note::slugify(e))),
            None => (original.as_str(), String::new()),
        };
        let name = format!("{}_{}{}", Ulid::generate(), note::slugify(stem), extension);

        let directory = self.root.join(ATTACHMENTS);
        fs::create_dir_all(&directory)?;
        fs::copy(source, directory.join(&name))?;

        // Forward slashes: this string goes into markdown, where the separator
        // is `/` on every platform including Windows.
        Ok(format!("{ATTACHMENTS}/{name}"))
    }

    /// Read an attachment by its vault-relative reference.
    ///
    /// The reference is whatever a note's markdown contains, so it is
    /// attacker-controlled in the sense that anything could be typed into a
    /// note by hand or arrive in a synced file. Two rules keep it inside the
    /// vault:
    ///
    /// 1. Every path component must be an ordinary name — no `..`, no root,
    ///    no Windows prefix like `C:`. That alone stops traversal.
    /// 2. The first component must be `attachments`, so a note cannot read
    ///    another note, the trash, or the index by asking for it.
    ///
    /// Checking the components rather than canonicalising and comparing
    /// prefixes is deliberate: canonicalisation follows symlinks, which on a
    /// synced folder can point anywhere, and it only works for paths that
    /// already exist.
    pub fn read_attachment(&self, reference: &str) -> Result<Vec<u8>> {
        let relative = Path::new(reference);

        let mut components = relative.components();
        let first = components.next();
        let is_attachments = matches!(
            first,
            Some(std::path::Component::Normal(name)) if name == ATTACHMENTS
        );
        if !is_attachments {
            return Err(SutraError::NoteNotFound(reference.to_string()));
        }
        if !components.all(|c| matches!(c, std::path::Component::Normal(_))) {
            return Err(SutraError::NoteNotFound(reference.to_string()));
        }

        Ok(fs::read(self.root.join(relative))?)
    }

    /// Locate a note's file by scanning for the id suffix.
    fn path_for(&self, id: &str) -> Result<PathBuf> {
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if note::id_from_file_name(name) == Some(id) {
                return Ok(entry.path());
            }
        }
        Err(SutraError::NoteNotFound(id.to_string()))
    }

    /// One past the highest position among a parent's children.
    fn next_position(&self, parent: Option<&str>) -> Result<i64> {
        let highest = self
            .list_notes()?
            .iter()
            .filter(|n| n.parent.as_deref() == parent)
            .map(|n| n.position)
            .max();
        Ok(highest.map_or(0, |p| p + 1))
    }

    /// Metadata for a file that has none — someone dropped a plain `.md` into
    /// the vault, or hand-deleted the frontmatter. We adopt it rather than
    /// refusing it: the title comes from the filename, the timestamps from now.
    fn synthesise(id: &str, file_name: &str) -> Frontmatter {
        let title = file_name
            .strip_suffix(".md")
            .and_then(|stem| stem.rsplit_once('_').map(|(t, _)| t))
            .filter(|t| !t.is_empty())
            .unwrap_or("Untitled")
            .replace('-', " ");
        Frontmatter::new(id.to_string(), title, None, 0)
    }
}

/// Trim, lowercase, drop empties, and de-duplicate while keeping order.
///
/// Lowercasing is the part that matters: "CVT" and "cvt" are one tag, and a
/// vault where they are two is a vault where filtering silently misses notes.
fn normalise_tags(tags: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() || out.contains(&tag) {
            continue;
        }
        out.push(tag);
    }
    out
}

fn summary_of(fm: &Frontmatter, body: &str) -> NoteSummary {
    NoteSummary {
        id: fm.id.clone(),
        title: fm.title.clone(),
        parent: fm.parent.clone(),
        position: fm.position,
        tags: fm.tags.clone(),
        icon: fm.icon.clone(),
        cover: fm.cover.clone(),
        excerpt: excerpt_of(body),
        updated: fm.updated,
    }
}

/// How much of the opening prose the list shows. One line at the widths the
/// list column is ever given, with a little slack for a narrow window.
const EXCERPT_LIMIT: usize = 160;

/// The first prose in a note, flattened to a single line.
///
/// Not a markdown renderer, and not trying to be. It drops the markers that
/// would read as noise in a preview — heading hashes, bullets, emphasis, the
/// 26 characters of a `[[id]]` link — and leaves everything else exactly as
/// written. Cheap enough to do for every note on every listing, which matters:
/// the whole vault is re-listed whenever a file changes on disk.
fn excerpt_of(body: &str) -> String {
    let mut out = String::new();

    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("```") || line == "---" {
            continue;
        }
        // A heading's hashes, a quote's caret, a bullet — but only where they
        // are actually markers, so a line like "-5 C" keeps its minus sign.
        let line = line.trim_start_matches('#');
        let line = line.strip_prefix("> ").unwrap_or(line);
        let line = ["- ", "* ", "+ "]
            .iter()
            .find_map(|marker| line.strip_prefix(marker))
            .unwrap_or(line)
            .trim();
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
        if out.len() >= EXCERPT_LIMIT {
            break;
        }
    }

    let out = strip_links(&out);
    let out: String = out.chars().filter(|c| !"*_`".contains(*c)).collect();
    let out = out.trim();

    // Truncate on a character boundary — `out` is UTF-8 and a formula or a
    // chemical name can put a multi-byte character anywhere.
    match out.char_indices().nth(EXCERPT_LIMIT) {
        Some((at, _)) => format!("{}…", out[..at].trim_end()),
        None => out.to_string(),
    }
}

/// Drop `[[id]]` links, `![alt](src)` images and `$...$` formulas from a
/// preview line.
///
/// All three are unreadable as source. A wikilink is a raw ULID on disk — the
/// title only exists at render time — so it would be 26 characters of noise,
/// and a formula in a one-line preview is backslashes.
fn strip_links(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;

    while let Some(at) = ["[[", "![", "$"]
        .iter()
        .filter_map(|marker| rest.find(marker))
        .min()
    {
        out.push_str(&rest[..at]);
        let closing = if rest[at..].starts_with("[[") {
            rest[at..].find("]]").map(|end| at + end + 2)
        } else if rest[at..].starts_with("![") {
            rest[at..].find(')').map(|end| at + end + 1)
        } else {
            // A formula runs to its closing delimiter. `$$` is a display block,
            // which is on its own line and so already gone by here.
            rest[at + 1..].find('$').map(|end| at + end + 2)
        };
        match closing {
            Some(end) => rest = &rest[end..],
            // An unclosed marker is just text; keep it and stop looking.
            None => {
                out.push_str(&rest[at..]);
                return out;
            }
        }
    }

    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway vault in the OS temp directory, removed on drop so a failing
    /// assertion cannot leave litter behind.
    struct TempVault(Vault);

    impl TempVault {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("sutra-vault-{}", Ulid::generate()));
            fs::create_dir_all(&root).unwrap();
            Self(Vault::open(root).unwrap())
        }
    }

    impl std::ops::Deref for TempVault {
        type Target = Vault;
        fn deref(&self) -> &Vault {
            &self.0
        }
    }

    impl Drop for TempVault {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.0.root());
        }
    }

    #[test]
    fn opening_creates_the_sidecar_directories() {
        let vault = TempVault::new();
        assert!(vault.root().join(ATTACHMENTS).is_dir());
        assert!(vault.root().join(TRASH).is_dir());
    }

    #[test]
    fn opening_a_file_is_rejected() {
        let path = std::env::temp_dir().join(format!("sutra-{}.txt", Ulid::generate()));
        fs::write(&path, "x").unwrap();
        assert!(Vault::open(path.clone()).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn create_read_and_save_round_trip() {
        let vault = TempVault::new();
        let created = vault.create_note("CVT runs", None).unwrap();

        vault
            .save_note(&created.summary.id, "CVT runs", "Ribbons along [001].")
            .unwrap();

        let read = vault.read_note(&created.summary.id).unwrap();
        assert_eq!(read.summary.title, "CVT runs");
        assert_eq!(read.body, "Ribbons along [001].\n");
        assert!(!read.adopted);
    }

    #[test]
    fn the_file_is_named_from_the_title_and_id() {
        let vault = TempVault::new();
        let note = vault.create_note("Sb2Se3 growth log", None).unwrap();
        let expected = format!("Sb2Se3-growth-log_{}.md", note.summary.id);
        assert!(vault.root().join(&expected).is_file(), "missing {expected}");
    }

    #[test]
    fn renaming_moves_the_file_but_keeps_the_id() {
        let vault = TempVault::new();
        let note = vault.create_note("Old title", None).unwrap();
        let id = note.summary.id.clone();
        let old = vault.root().join(format!("Old-title_{id}.md"));
        assert!(old.is_file());

        vault.save_note(&id, "New title", "body").unwrap();

        assert!(!old.exists(), "old filename should be gone");
        assert!(vault.root().join(format!("New-title_{id}.md")).is_file());
        // The id is the identity; a rename must not disturb it.
        assert_eq!(vault.read_note(&id).unwrap().summary.id, id);
    }

    #[test]
    fn saving_preserves_created_and_advances_updated() {
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        let id = note.summary.id.clone();

        let path = vault.path_for(&id).unwrap();
        let before = frontmatter::split(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .0
            .unwrap();

        vault.save_note(&id, "T", "changed").unwrap();

        let after = frontmatter::split(&fs::read_to_string(vault.path_for(&id).unwrap()).unwrap())
            .unwrap()
            .0
            .unwrap();

        assert_eq!(before.created, after.created, "created must not move");
        // Not `>`: timestamps are truncated to whole seconds, so two saves
        // inside the same second are legitimately equal.
        assert!(
            after.updated >= before.updated,
            "updated must not go backwards"
        );
        assert_eq!(after.updated.nanosecond(), 0, "no sub-second noise on disk");
    }

    #[test]
    fn positions_increment_among_siblings() {
        let vault = TempVault::new();
        let a = vault.create_note("A", None).unwrap();
        let b = vault.create_note("B", None).unwrap();
        let child = vault.create_note("C", Some(a.summary.id.clone())).unwrap();

        assert_eq!(a.summary.position, 0);
        assert_eq!(b.summary.position, 1);
        // A different parent means a separate sequence.
        assert_eq!(child.summary.position, 0);
    }

    #[test]
    fn listing_skips_files_that_are_not_notes() {
        let vault = TempVault::new();
        vault.create_note("Real", None).unwrap();
        fs::write(vault.root().join("README.md"), "not a note").unwrap();
        fs::write(vault.root().join("notes.txt"), "nor this").unwrap();

        let notes = vault.list_notes().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Real");
    }

    #[test]
    fn a_corrupt_note_does_not_break_the_listing() {
        let vault = TempVault::new();
        vault.create_note("Good", None).unwrap();
        // Opening fence with no closing fence: unparseable.
        let broken = format!("broken_{}.md", Ulid::generate());
        fs::write(vault.root().join(broken), "---\nid: x\nno closing fence\n").unwrap();

        let notes = vault.list_notes().unwrap();
        assert_eq!(notes.len(), 1, "the good note must still be listed");
    }

    #[test]
    fn a_plain_markdown_file_is_adopted() {
        let vault = TempVault::new();
        let id = Ulid::generate().to_string();
        fs::write(
            vault.root().join(format!("Dropped-in_{id}.md")),
            "Just prose, no frontmatter.\n",
        )
        .unwrap();

        let note = vault.read_note(&id).unwrap();
        assert!(note.adopted);
        assert_eq!(note.summary.title, "Dropped in");
        assert_eq!(note.body, "Just prose, no frontmatter.\n");
    }

    #[test]
    fn set_meta_writes_frontmatter_and_leaves_the_body_alone() {
        let vault = TempVault::new();
        let note = vault.create_note("Runs", None).unwrap();
        let id = note.summary.id.clone();
        vault
            .save_note(&id, "Runs", "Body that must survive.")
            .unwrap();

        let updated = vault
            .set_meta(
                &id,
                Some("\u{1f9ea}".into()),
                Some("attachments/01H_cover.png".into()),
                vec!["Sb2Se3".into(), "CVT".into()],
            )
            .unwrap();

        assert_eq!(updated.icon.as_deref(), Some("\u{1f9ea}"));
        assert_eq!(updated.tags, vec!["sb2se3", "cvt"]);
        assert_eq!(
            vault.read_note(&id).unwrap().body,
            "Body that must survive.\n"
        );
    }

    #[test]
    fn tags_are_normalised_so_one_tag_is_one_tag() {
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        let updated = vault
            .set_meta(
                &note.summary.id,
                None,
                None,
                vec![
                    "  CVT  ".into(),
                    "cvt".into(),
                    "".into(),
                    "   ".into(),
                    "Sb2Se3".into(),
                ],
            )
            .unwrap();
        assert_eq!(updated.tags, vec!["cvt", "sb2se3"]);
    }

    #[test]
    fn clearing_an_icon_removes_it() {
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        let id = note.summary.id.clone();
        vault
            .set_meta(&id, Some("\u{1f9ea}".into()), None, vec![])
            .unwrap();
        // An empty string is how the UI says "none"; it must not become an
        // icon that renders as nothing.
        let cleared = vault
            .set_meta(&id, Some("  ".into()), None, vec![])
            .unwrap();
        assert_eq!(cleared.icon, None);
    }

    #[test]
    fn delete_moves_to_trash_and_keeps_the_bytes() {
        let vault = TempVault::new();
        let note = vault.create_note("Doomed", None).unwrap();
        let id = note.summary.id.clone();
        vault
            .save_note(&id, "Doomed", "irreplaceable data")
            .unwrap();

        vault.delete_note(&id).unwrap();

        assert!(vault.path_for(&id).is_err(), "should be gone from the root");
        let trashed =
            fs::read_to_string(vault.root().join(TRASH).join(format!("Doomed_{id}.md"))).unwrap();
        assert!(trashed.contains("irreplaceable data"));
    }

    #[test]
    fn deleting_the_same_name_twice_does_not_overwrite_the_first() {
        let vault = TempVault::new();
        let first = vault.create_note("Same", None).unwrap();
        vault
            .save_note(&first.summary.id, "Same", "first copy")
            .unwrap();
        vault.delete_note(&first.summary.id).unwrap();

        // Recreate a note that slugs to the same filename, then delete it too.
        let path = vault.root().join(format!("Same_{}.md", first.summary.id));
        fs::write(&path, "second copy").unwrap();
        vault.delete_note(&first.summary.id).unwrap();

        let trash: Vec<_> = fs::read_dir(vault.root().join(TRASH))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(trash.len(), 2, "both copies must survive in the trash");
    }

    #[test]
    fn attachments_are_ulid_prefixed_and_relative() {
        let vault = TempVault::new();
        let source = std::env::temp_dir().join(format!("sutra-src-{}.png", Ulid::generate()));
        fs::write(&source, b"\x89PNG fake").unwrap();

        let reference = vault.import_attachment(&source).unwrap();

        assert!(reference.starts_with("attachments/"), "got {reference}");
        assert!(reference.ends_with(".png"), "extension lost: {reference}");
        // Forward slashes, because this goes into markdown.
        assert!(!reference.contains('\\'));
        assert!(vault.root().join(&reference).is_file());
        let _ = fs::remove_file(source);
    }

    #[test]
    fn an_attachment_reads_back_by_its_reference() {
        let vault = TempVault::new();
        let source = std::env::temp_dir().join(format!("sutra-src-{}.png", Ulid::generate()));
        fs::write(&source, b"PNG BYTES").unwrap();
        let reference = vault.import_attachment(&source).unwrap();

        assert_eq!(vault.read_attachment(&reference).unwrap(), b"PNG BYTES");
        let _ = fs::remove_file(source);
    }

    #[test]
    fn attachment_reads_cannot_escape_the_attachments_folder() {
        let vault = TempVault::new();
        // A note is next to attachments/, and the trash holds deleted work.
        // Neither may be reachable by asking the attachment reader for it.
        let note = vault.create_note("Secret", None).unwrap();
        let file_name = note::file_name("Secret", &note.summary.id);

        for reference in [
            &format!("attachments/../{file_name}"),
            &format!("../{file_name}"),
            &file_name,
            "attachments/../trash/anything.md",
            "attachments/../../etc/passwd",
            "/etc/passwd",
            "trash/anything.md",
        ] {
            assert!(
                vault.read_attachment(reference).is_err(),
                "should have refused {reference:?}"
            );
        }
    }

    #[test]
    fn a_missing_attachment_is_an_error_not_a_panic() {
        let vault = TempVault::new();
        assert!(vault.read_attachment("attachments/nothing.png").is_err());
    }

    #[test]
    fn two_attachments_with_one_name_do_not_collide() {
        let vault = TempVault::new();
        let source = std::env::temp_dir().join(format!("sutra-src-{}.png", Ulid::generate()));
        fs::write(&source, b"data").unwrap();

        let first = vault.import_attachment(&source).unwrap();
        let second = vault.import_attachment(&source).unwrap();

        assert_ne!(first, second);
        assert!(vault.root().join(&first).is_file());
        assert!(vault.root().join(&second).is_file());
        let _ = fs::remove_file(source);
    }
    #[test]
    fn an_excerpt_is_the_opening_prose_without_the_markers() {
        let body = "# Growth log\n\n- Source at 560 C\n- **Sink** at 380 C\n";
        assert_eq!(excerpt_of(body), "Growth log Source at 560 C Sink at 380 C");
    }

    #[test]
    fn an_excerpt_keeps_a_minus_sign_that_is_not_a_bullet() {
        // "-5" is a temperature, not a list. Only "- " starts a bullet.
        assert_eq!(excerpt_of("-5 C overnight"), "-5 C overnight");
    }

    #[test]
    fn an_excerpt_drops_wikilinks_and_images() {
        let body = "See [[01H8XGJWBWBAQ4ZQ2XYZ0000AA]] and ![plot](attachments/x.png) here";
        assert_eq!(excerpt_of(body), "See  and  here");
    }

    #[test]
    fn an_unclosed_link_is_kept_as_text() {
        assert_eq!(excerpt_of("a [[ b"), "a [[ b");
    }

    #[test]
    fn a_lone_dollar_is_kept() {
        // No closing delimiter means it was never a formula.
        assert_eq!(excerpt_of("costs $5 total"), "costs $5 total");
    }

    #[test]
    fn an_excerpt_drops_inline_formulas() {
        // Raw LaTeX in a one-line preview is backslashes, not information.
        assert_eq!(
            excerpt_of("Band gap $E_g = 1.2\\,\\mathrm{eV}$ measured"),
            "Band gap  measured"
        );
    }

    #[test]
    fn a_long_excerpt_is_cut_on_a_character_boundary() {
        // A multi-byte character sitting exactly on the limit would panic a
        // byte slice, so the cut is by characters.
        let body = "é".repeat(400);
        let excerpt = excerpt_of(&body);
        assert!(excerpt.ends_with('…'));
        assert_eq!(excerpt.chars().count(), EXCERPT_LIMIT + 1);
    }

    #[test]
    fn a_fence_and_its_rule_are_skipped() {
        assert_eq!(
            excerpt_of("```rust\nfn main() {}\n```\n---\ntext"),
            "fn main() {} text"
        );
    }

    #[test]
    fn listing_carries_an_excerpt() {
        let vault = TempVault::new();
        let note = vault.0.create_note("Anneal", None).unwrap();
        vault
            .0
            .save_note(
                &note.summary.id,
                "Anneal",
                "Ramped to 400 C over two hours.",
            )
            .unwrap();
        let listed = vault.0.list_notes().unwrap();
        assert_eq!(listed[0].excerpt, "Ramped to 400 C over two hours.");
    }
}
