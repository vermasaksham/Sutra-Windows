//! The vault: a tree of markdown files, and the operations on it.
//!
//! Notes live in real, nested directories, because a folder tree is the part
//! of the layout a person reads. Identity does not: the ULID lives inside the
//! file, in frontmatter. Keeping those two apart is what lets a note be
//! renamed and moved freely without a single `[[id]]` link anywhere in the
//! vault having to change.

use crate::error::{Result, SutraError};
use crate::frontmatter::{self, Frontmatter};
use crate::note;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;
use time::OffsetDateTime;
use ulid::Ulid;

/// Everything the app owns and the user never edits. Hidden, and safe to
/// delete — nothing lives in here that is not derived from the markdown beside
/// it, or already on its way out.
const SUTRA: &str = ".sutra";
const TRASH: &str = "trash";

/// Attachments sit beside the notes that use them, one hidden folder per
/// directory, so the note explorer only ever contains notes.
const ATTACHMENTS: &str = ".attachments";

/// How deep a note may sit below the root. The brief asks for three or four
/// levels; four is the cap. It is also roughly what keeps a Windows path under
/// the 260-character default once a long title and a deep `Documents\...`
/// prefix are accounted for.
pub const MAX_DEPTH: usize = 4;

/// A note's metadata without its body. This is what the sidebar needs, and
/// loading bodies for a whole vault to draw a tree would be wasteful.
#[derive(Debug, Clone, Serialize)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    /// Vault-relative directory, `/`-separated. Empty string means the root.
    ///
    /// This replaced a `parent` id in frontmatter. Location is now a fact about
    /// where the file is, not a claim the file makes about itself, so the two
    /// can never disagree.
    pub folder: String,
    /// Sort order among siblings. Optional: absent or equal positions fall back
    /// to sorting by title, which is what a plain folder listing does anyway.
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
/// Note what is *not* here: no absolute path. The frontend addresses notes by
/// id, knows folders only as vault-relative strings, and never learns where the
/// vault sits on disk.
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
    /// id -> vault-relative path of the note file.
    ///
    /// Identity used to be in the filename, so finding a note was a directory
    /// listing. Now it is inside the file, and reading every file on every
    /// lookup would be absurd — so the map is built by a scan and kept current
    /// by the operations that create, rename, move and delete files.
    ///
    /// `RwLock` rather than `Mutex` because lookups vastly outnumber writes and
    /// several reads can safely happen at once. A poisoned lock is recovered
    /// rather than propagated: a panic in an unrelated command must not make
    /// the vault permanently unopenable.
    paths: RwLock<HashMap<String, String>>,
}

impl Vault {
    /// Open a directory as a vault, creating the app's own folder if needed.
    ///
    /// Takes `PathBuf` by value rather than `&Path` because the Vault stores it
    /// — asking for ownership up front is honest about that, and saves the
    /// caller from a clone they would otherwise have to make anyway.
    pub fn open(root: PathBuf) -> Result<Self> {
        if !root.is_dir() {
            return Err(SutraError::NotADirectory(root.display().to_string()));
        }
        fs::create_dir_all(root.join(SUTRA).join(TRASH))?;
        let vault = Self {
            root,
            paths: RwLock::new(HashMap::new()),
        };
        // Populate the map once up front, so the first note the user opens does
        // not pay for a full scan.
        vault.list_notes()?;
        Ok(vault)
    }

    /// Only the tests need this. Production code reaches the root through the
    /// methods below, which is the point — nothing outside should be building
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

    /// Every note in the vault, and a refreshed id -> path map as a side effect.
    ///
    /// A full recursive scan that re-reads every file. That is fine and
    /// deliberately not optimised: the SQLite index sits in front of it, and
    /// the index has to be rebuildable from exactly this scan.
    ///
    /// Unreadable or malformed files are skipped rather than failing the whole
    /// listing — one corrupt note must not make the vault unopenable.
    pub fn list_notes(&self) -> Result<Vec<NoteSummary>> {
        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files)?;

        let mut notes = Vec::new();
        let mut map = HashMap::with_capacity(files.len());

        for relative in files {
            let Ok(contents) = fs::read_to_string(self.root.join(&relative)) else {
                continue;
            };
            let Ok((parsed, body)) = frontmatter::split(&contents) else {
                continue;
            };
            let fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
            // Two files claiming one id is possible — a copied note, a bad
            // merge. First one wins and the second is left out of the map
            // rather than silently shadowing it.
            map.entry(fm.id.clone()).or_insert_with(|| relative.clone());
            notes.push(summary_of(&fm, body, folder_of(&relative)));
        }

        notes.sort_by(|a, b| {
            a.folder
                .cmp(&b.folder)
                .then_with(|| a.position.cmp(&b.position))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });

        *self.paths.write().unwrap_or_else(|e| e.into_inner()) = map;
        Ok(notes)
    }

    /// Every folder in the vault, `/`-separated, shallowest first.
    ///
    /// Derived from the directories that exist, not from a stored list: the
    /// filesystem is the truth about where things are, so a folder made in
    /// Explorer appears here without the app being told.
    pub fn list_folders(&self) -> Result<Vec<String>> {
        let mut folders = Vec::new();
        collect_dirs(&self.root, &self.root, 0, &mut folders)?;
        folders.sort();
        Ok(folders)
    }

    /// Make a folder. Parents are created as needed.
    pub fn create_folder(&self, folder: &str) -> Result<String> {
        let relative = self.checked_folder(folder)?;
        fs::create_dir_all(self.root.join(&relative))?;
        Ok(relative)
    }

    /// Read one note.
    pub fn read_note(&self, id: &str) -> Result<NoteDoc> {
        let relative = self.relative_for(id)?;
        let contents = fs::read_to_string(self.root.join(&relative))?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let adopted = parsed.is_none();
        let fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
        Ok(NoteDoc {
            summary: summary_of(&fm, body, folder_of(&relative)),
            body: body.to_string(),
            adopted,
        })
    }

    /// Create an empty note in a folder and return it.
    pub fn create_note(&self, title: &str, folder: Option<String>) -> Result<NoteDoc> {
        let folder = self.checked_folder(folder.as_deref().unwrap_or(""))?;
        let directory = self.root.join(&folder);
        fs::create_dir_all(&directory)?;

        let id = Ulid::generate().to_string();
        let mut fm = Frontmatter::new(id.clone(), title.to_string());
        fm.position = self.next_position(&folder)?;

        let relative = join_relative(&folder, &unique_name(&directory, title, None));
        note::write_atomic(&self.root.join(&relative), &frontmatter::join(&fm, "")?)?;
        self.remember(&id, &relative);

        Ok(NoteDoc {
            summary: summary_of(&fm, "", folder),
            body: String::new(),
            adopted: false,
        })
    }

    /// Save a note's title and body.
    ///
    /// `created` is preserved from whatever is on disk; `updated` is stamped
    /// now. If the title changed the file is renamed within its folder — the
    /// id does not live in the name, so nothing that points at this note
    /// notices.
    pub fn save_note(&self, id: &str, title: &str, body: &str) -> Result<NoteSummary> {
        let relative = self.relative_for(id)?;
        let path = self.root.join(&relative);
        let existing = fs::read_to_string(&path)?;
        let (parsed, _) = frontmatter::split(&existing)?;

        let mut fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
        // Adopting: the file had no frontmatter, so its id was derived from its
        // path. Give it a real one now, while nothing can be linking to it yet.
        if fm.id != id {
            fm.id = id.to_string();
        }
        let renamed = fm.title != title;
        fm.title = title.to_string();
        fm.updated = frontmatter::now();

        let folder = folder_of(&relative);
        let target = if renamed {
            join_relative(
                &folder,
                &unique_name(&self.root.join(&folder), title, Some(&relative)),
            )
        } else {
            relative.clone()
        };

        note::write_atomic(&self.root.join(&target), &frontmatter::join(&fm, body)?)?;

        // Written first, removed second. The reverse order would leave a window
        // with no file at all, and a crash in that window would lose the note.
        if target != relative {
            fs::remove_file(&path)?;
        }
        self.remember(&fm.id, &target);

        Ok(summary_of(&fm, body, folder))
    }

    /// Move a note into another folder.
    ///
    /// This is the operation the whole layout is arranged around, and it is
    /// almost nothing: a rename. No note file anywhere is rewritten, because
    /// links name the id and the id is not in the path. Attachments are not
    /// moved either — a reference in the body is vault-relative, so it keeps
    /// resolving from wherever the note ends up.
    pub fn move_note(&self, id: &str, folder: &str) -> Result<NoteSummary> {
        let relative = self.relative_for(id)?;
        let folder = self.checked_folder(folder)?;
        if folder_of(&relative) == folder {
            return self.read_note(id).map(|d| d.summary);
        }

        let directory = self.root.join(&folder);
        fs::create_dir_all(&directory)?;

        let name = Path::new(&relative)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled.md");
        let stem = name.strip_suffix(".md").unwrap_or(name);
        let target = join_relative(&folder, &unique_name(&directory, stem, None));

        fs::rename(self.root.join(&relative), self.root.join(&target))?;
        self.remember(id, &target);

        // Re-read rather than trusting a cached summary: the file may have been
        // edited outside the app between the scan and this call.
        let contents = fs::read_to_string(self.root.join(&target))?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let fm = parsed.unwrap_or_else(|| Self::synthesise(&target));
        Ok(summary_of(&fm, body, folder))
    }

    /// Replace a note's page-level metadata.
    ///
    /// The caller sends the complete desired state rather than a patch. A patch
    /// would need to distinguish "leave this alone" from "set this to null",
    /// which over an IPC boundary means a nested Option and a lot of ceremony
    /// for no benefit — the frontend always has the whole note loaded anyway.
    pub fn set_meta(
        &self,
        id: &str,
        icon: Option<String>,
        cover: Option<String>,
        tags: Vec<String>,
    ) -> Result<NoteSummary> {
        let relative = self.relative_for(id)?;
        let path = self.root.join(&relative);
        let contents = fs::read_to_string(&path)?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let mut fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
        if fm.id != id {
            fm.id = id.to_string();
        }

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
        Ok(summary_of(&fm, &body, folder_of(&relative)))
    }

    /// Move a note to the trash rather than unlinking it.
    ///
    /// A rename, so it is atomic and instant regardless of file size, and the
    /// note is recoverable by dragging it back out in Explorer. The folder it
    /// came from is flattened into the trashed name, so two notes called the
    /// same thing in different folders stay distinguishable.
    pub fn delete_note(&self, id: &str) -> Result<()> {
        let relative = self.relative_for(id)?;
        let flattened = relative.replace('/', " - ");
        let trash = self.root.join(SUTRA).join(TRASH);
        fs::create_dir_all(&trash)?;

        let mut target = trash.join(&flattened);
        // Deleting, restoring, and deleting again must not silently overwrite
        // the first copy.
        if target.exists() {
            target = trash.join(format!("{}.{}", Ulid::generate(), flattened));
        }
        fs::rename(self.root.join(&relative), &target)?;
        self.paths
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        Ok(())
    }

    /// Copy a file into a folder's `.attachments/`, returning the
    /// vault-relative path a note should reference.
    pub fn import_attachment(&self, source: &Path, folder: Option<String>) -> Result<String> {
        let folder = self.checked_folder(folder.as_deref().unwrap_or(""))?;
        let original = source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        // Slug the original name so an attachment can never introduce a
        // separator or a character the filesystem rejects. Attachments keep the
        // dashed style — nobody reads these names, and the ULID prefix already
        // makes them unlovely.
        let (stem, extension) = match original.rsplit_once('.') {
            Some((s, e)) => (s, format!(".{}", note::slugify(e))),
            None => (original.as_str(), String::new()),
        };
        let name = format!("{}_{}{}", Ulid::generate(), note::slugify(stem), extension);

        let directory = self.root.join(&folder).join(ATTACHMENTS);
        fs::create_dir_all(&directory)?;
        fs::copy(source, directory.join(&name))?;

        // Forward slashes: this string goes into markdown, where the separator
        // is `/` on every platform including Windows.
        Ok(join_relative(&join_relative(&folder, ATTACHMENTS), &name))
    }

    /// Read an attachment by its vault-relative reference.
    ///
    /// The reference is whatever a note's markdown contains, so it is
    /// attacker-controlled in the sense that anything could be typed into a
    /// note by hand or arrive in a synced file. Two rules keep it inside the
    /// vault and out of the note namespace:
    ///
    /// 1. Every path component must be an ordinary name — no `..`, no root, no
    ///    Windows prefix like `C:`. That alone stops traversal.
    /// 2. The file's own directory must be named `.attachments`, so a note
    ///    cannot read another note, the trash, or the index by asking for it.
    ///
    /// Checking the components rather than canonicalising and comparing
    /// prefixes is deliberate: canonicalisation follows symlinks, which on a
    /// synced folder can point anywhere, and it only works for paths that
    /// already exist.
    pub fn read_attachment(&self, reference: &str) -> Result<Vec<u8>> {
        let relative = Path::new(reference);
        let refused = || SutraError::NoteNotFound(reference.to_string());

        if !relative.components().all(is_plain) {
            return Err(refused());
        }
        let in_attachments = relative
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == ATTACHMENTS);
        if !in_attachments {
            return Err(refused());
        }

        Ok(fs::read(self.root.join(relative))?)
    }

    /// The id of the note at an absolute path, for the file watcher.
    ///
    /// Tries the map first, which covers a note the app already knows about,
    /// including one that has just been deleted underneath us. Falls back to
    /// reading the file, which is how a note created in another editor gets
    /// noticed.
    pub fn id_at(&self, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(&self.root).ok().map(to_relative)?;

        {
            let map = self.paths.read().unwrap_or_else(|e| e.into_inner());
            if let Some((id, _)) = map.iter().find(|(_, p)| **p == relative) {
                return Some(id.clone());
            }
        }

        let contents = fs::read_to_string(path).ok()?;
        let (parsed, _) = frontmatter::split(&contents).ok()?;
        Some(match parsed {
            Some(fm) => fm.id,
            None => note::adopted_id(&relative),
        })
    }

    /// Where a note's file is, relative to the vault root.
    ///
    /// A miss triggers one rescan and one retry, which is how a note created
    /// outside the app becomes reachable without the user doing anything.
    fn relative_for(&self, id: &str) -> Result<String> {
        if let Some(found) = self.lookup(id) {
            return Ok(found);
        }
        self.list_notes()?;
        self.lookup(id)
            .ok_or_else(|| SutraError::NoteNotFound(id.to_string()))
    }

    /// The absolute path of a note's file. Tests only — production code goes
    /// through the methods above, so no path escapes this module.
    #[cfg(test)]
    pub fn path_for(&self, id: &str) -> Result<PathBuf> {
        Ok(self.root.join(self.relative_for(id)?))
    }

    fn lookup(&self, id: &str) -> Option<String> {
        self.paths
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    fn remember(&self, id: &str, relative: &str) {
        self.paths
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), relative.to_string());
    }

    /// Validate a folder the frontend asked for, and normalise its separators.
    ///
    /// The frontend sends folder strings straight from user input, so this is
    /// the boundary where a path stops being a suggestion.
    ///
    /// The components are checked *before* any normalisation, deliberately. It
    /// is tempting to trim a leading `/` and carry on, but then `/etc` quietly
    /// becomes the vault's own `etc` folder — which is safe, and still not what
    /// anyone typing it meant. Refusing is the honest answer, and it keeps this
    /// function to one rule: every component must be an ordinary name.
    fn checked_folder(&self, folder: &str) -> Result<String> {
        let trimmed = folder.trim();
        if trimmed.is_empty() {
            return Ok(String::new());
        }
        let refused = || SutraError::NotADirectory(folder.to_string());

        let mut parts = Vec::new();
        for component in Path::new(trimmed).components() {
            // `..`, a root, and a Windows drive prefix are all not-Normal.
            let Component::Normal(name) = component else {
                return Err(refused());
            };
            let Some(name) = name.to_str() else {
                return Err(refused());
            };
            // A backslash is not a separator on Unix, so `a\..\b` would arrive
            // as one component and escape the check above.
            if name.contains('\\') {
                return Err(refused());
            }
            // Hidden names are the app's: `.sutra` and every `.attachments`.
            if name.starts_with('.') {
                return Err(refused());
            }
            parts.push(name);
        }

        if parts.is_empty() {
            return Err(refused());
        }
        if parts.len() > MAX_DEPTH {
            return Err(SutraError::NotADirectory(format!(
                "{folder} is deeper than {MAX_DEPTH} folders"
            )));
        }
        Ok(parts.join("/"))
    }

    /// One past the highest position among a folder's notes.
    fn next_position(&self, folder: &str) -> Result<i64> {
        let highest = self
            .list_notes()?
            .iter()
            .filter(|n| n.folder == folder)
            .map(|n| n.position)
            .max();
        Ok(highest.map_or(0, |p| p + 1))
    }

    /// Metadata for a file that has none — someone dropped a plain `.md` into
    /// the vault, or hand-deleted the frontmatter. We adopt it rather than
    /// refusing it: the title comes from the filename, the id from the path,
    /// and both are replaced by real ones the first time it is saved.
    fn synthesise(relative: &str) -> Frontmatter {
        let title = Path::new(relative)
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|t| !t.is_empty())
            .unwrap_or("Untitled")
            .to_string();
        Frontmatter::new(note::adopted_id(relative), title)
    }
}

/// A path component that is an ordinary name — not `..`, not a root, not a
/// Windows drive prefix.
fn is_plain(component: Component<'_>) -> bool {
    matches!(component, Component::Normal(_))
}

/// `a/b/c.md` -> `a/b`. The root's notes get an empty string.
fn folder_of(relative: &str) -> String {
    match relative.rsplit_once('/') {
        Some((folder, _)) => folder.to_string(),
        None => String::new(),
    }
}

fn join_relative(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// Always `/`, whatever the platform's separator is.
fn to_relative(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(n) => n.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// A filename for `title` that nothing else in `directory` is already using.
///
/// `keep` is the note's own current path, so re-saving a note under its
/// existing name does not see itself as a collision and add a suffix.
fn unique_name(directory: &Path, title: &str, keep: Option<&str>) -> String {
    let stem = note::file_stem(title);
    let keep_name = keep
        .and_then(|k| Path::new(k).file_name())
        .and_then(|n| n.to_str());

    for attempt in 0..1000 {
        let name = if attempt == 0 {
            note::file_name(title)
        } else {
            format!("{stem} {}.md", attempt + 1)
        };
        if Some(name.as_str()) == keep_name || !directory.join(&name).exists() {
            return name;
        }
    }
    // A thousand notes with one title in one folder is not a real vault, but
    // returning something unique beats looping forever.
    format!("{stem} {}.md", Ulid::generate())
}

/// Recursively collect note paths, relative to `base` and `/`-separated.
///
/// Anything whose name starts with `.` is skipped, which is how `.sutra` and
/// every `.attachments` stay out of the note namespace with one rule rather
/// than a list of exceptions.
fn collect(dir: &Path, base: &Path, depth: usize, out: &mut Vec<String>) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        // `file_type` does not follow symlinks, so a link pointing outside the
        // vault is not walked into.
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect(&path, base, depth + 1, out)?;
        } else if kind.is_file() && name.ends_with(".md") {
            if let Ok(relative) = path.strip_prefix(base) {
                out.push(to_relative(relative));
            }
        }
    }
    Ok(())
}

/// The same walk, for directories.
fn collect_dirs(dir: &Path, base: &Path, depth: usize, out: &mut Vec<String>) -> Result<()> {
    if depth >= MAX_DEPTH {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') || !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if let Ok(relative) = path.strip_prefix(base) {
            out.push(to_relative(relative));
        }
        collect_dirs(&path, base, depth + 1, out)?;
    }
    Ok(())
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

fn summary_of(fm: &Frontmatter, body: &str, folder: String) -> NoteSummary {
    NoteSummary {
        id: fm.id.clone(),
        title: fm.title.clone(),
        folder,
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

    fn folder(name: &str) -> Option<String> {
        Some(name.to_string())
    }

    // ---- the critical test -------------------------------------------------

    /// Section 30's critical test, and the reason the layout is arranged the
    /// way it is.
    ///
    /// Move a note between two folders and check that every relationship it
    /// had survives. Nothing here is bookkeeping the move has to perform — the
    /// links, the backlinks and the attachment reference all keep working
    /// because none of them ever mentioned where the file was.
    #[test]
    fn moving_a_note_preserves_every_relationship() {
        let vault = TempVault::new();

        let target = vault
            .create_note("Sb2Se3 Cp", folder("Research/Sb2Se3/Thermodynamics"))
            .unwrap();
        let id = target.summary.id.clone();

        // Something links to it, and it links to something.
        let other = vault
            .create_note("Phonon transport", folder("Research"))
            .unwrap();
        vault
            .save_note(
                &other.summary.id,
                "Phonon transport",
                &format!("See [[{id}]]."),
            )
            .unwrap();

        // An attachment, referenced from the note's body.
        let source = std::env::temp_dir().join(format!("dsc-{}.png", Ulid::generate()));
        fs::write(&source, b"\x89PNG fake").unwrap();
        let reference = vault
            .import_attachment(&source, folder("Research/Sb2Se3/Thermodynamics"))
            .unwrap();
        let body = format!(
            "Ribbons align. ![DSC]({reference}) and [[{}]].",
            other.summary.id
        );
        vault.save_note(&id, "Sb2Se3 Cp", &body).unwrap();

        // Metadata worth losing.
        vault
            .set_meta(
                &id,
                Some("🧪".into()),
                None,
                vec!["Sb2Se3".into(), "cvt".into()],
            )
            .unwrap();
        let before = vault.read_note(&id).unwrap();
        let created_before =
            frontmatter::split(&fs::read_to_string(vault.path_for(&id).unwrap()).unwrap())
                .unwrap()
                .0
                .unwrap()
                .created;

        // ---- the move ----
        let moved = vault
            .move_note(&id, "Research/SbSeI/Thermodynamics")
            .unwrap();

        assert_eq!(moved.folder, "Research/SbSeI/Thermodynamics");
        assert_eq!(moved.id, id, "the id must not change");

        let after = vault.read_note(&id).unwrap();
        assert_eq!(after.body, before.body, "the body must be untouched");
        assert_eq!(after.summary.title, "Sb2Se3 Cp");
        assert_eq!(after.summary.tags, vec!["sb2se3", "cvt"]);
        assert_eq!(after.summary.icon.as_deref(), Some("🧪"));
        let created_after =
            frontmatter::split(&fs::read_to_string(vault.path_for(&id).unwrap()).unwrap())
                .unwrap()
                .0
                .unwrap()
                .created;
        assert_eq!(created_after, created_before, "created must survive a move");

        // The outgoing link still names the same note...
        assert!(after.body.contains(&format!("[[{}]]", other.summary.id)));
        // ...and the incoming one was never rewritten, so it still resolves.
        let linker = vault.read_note(&other.summary.id).unwrap();
        assert!(linker.body.contains(&format!("[[{id}]]")));
        assert!(
            vault.read_note(&id).is_ok(),
            "the link target still resolves"
        );

        // The attachment is still readable by the reference in the body, even
        // though the file did not move with the note.
        assert_eq!(vault.read_attachment(&reference).unwrap(), b"\x89PNG fake");

        // And the old location is empty.
        assert!(
            !vault
                .root()
                .join("Research/Sb2Se3/Thermodynamics/Sb2Se3 Cp.md")
                .exists()
        );

        let _ = fs::remove_file(source);
    }

    // ---- opening and layout ------------------------------------------------

    #[test]
    fn opening_creates_the_app_folder() {
        let vault = TempVault::new();
        assert!(vault.root().join(SUTRA).join(TRASH).is_dir());
    }

    #[test]
    fn opening_a_file_is_rejected() {
        let path = std::env::temp_dir().join(format!("sutra-{}.txt", Ulid::generate()));
        fs::write(&path, "x").unwrap();
        assert!(Vault::open(path.clone()).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_note_is_a_readable_filename_with_no_id_in_it() {
        let vault = TempVault::new();
        let note = vault
            .create_note("Sb2Se3 Cp", folder("Research/Sb2Se3"))
            .unwrap();
        assert!(vault.root().join("Research/Sb2Se3/Sb2Se3 Cp.md").is_file());
        // The id is nowhere in the path.
        assert!(
            !vault
                .root()
                .join("Research/Sb2Se3/Sb2Se3 Cp.md")
                .to_string_lossy()
                .contains(&note.summary.id)
        );
    }

    #[test]
    fn two_notes_with_one_title_in_one_folder_do_not_collide() {
        let vault = TempVault::new();
        let a = vault.create_note("Cp", folder("Research")).unwrap();
        let b = vault.create_note("Cp", folder("Research")).unwrap();

        assert_ne!(a.summary.id, b.summary.id);
        assert!(vault.root().join("Research/Cp.md").is_file());
        assert!(vault.root().join("Research/Cp 2.md").is_file());
        // Both are still individually reachable.
        assert!(vault.read_note(&a.summary.id).is_ok());
        assert!(vault.read_note(&b.summary.id).is_ok());
    }

    #[test]
    fn the_same_title_in_different_folders_keeps_the_clean_name() {
        let vault = TempVault::new();
        vault.create_note("Cp", folder("Research/Sb2Se3")).unwrap();
        vault.create_note("Cp", folder("Research/SbSeI")).unwrap();
        assert!(vault.root().join("Research/Sb2Se3/Cp.md").is_file());
        assert!(vault.root().join("Research/SbSeI/Cp.md").is_file());
    }

    #[test]
    fn folders_deeper_than_the_cap_are_refused() {
        let vault = TempVault::new();
        assert!(vault.create_note("Deep", folder("a/b/c/d")).is_ok());
        assert!(
            vault.create_note("Deeper", folder("a/b/c/d/e")).is_err(),
            "MAX_DEPTH is a limit, not a suggestion"
        );
    }

    #[test]
    fn a_folder_cannot_climb_out_of_the_vault() {
        let vault = TempVault::new();
        for attempt in ["../escape", "a/../../escape", "/etc", "a/./../.."] {
            assert!(
                vault.create_note("X", folder(attempt)).is_err(),
                "{attempt} should be refused"
            );
        }
    }

    #[test]
    fn a_folder_cannot_hide_inside_the_app_directory() {
        let vault = TempVault::new();
        assert!(vault.create_note("X", folder(".sutra/trash")).is_err());
        assert!(vault.create_note("X", folder("a/.attachments")).is_err());
    }

    #[test]
    fn folders_are_listed_from_the_filesystem() {
        let vault = TempVault::new();
        vault.create_note("A", folder("Research/Sb2Se3")).unwrap();
        // Made outside the app; it should still appear.
        fs::create_dir_all(vault.root().join("Library")).unwrap();

        let folders = vault.list_folders().unwrap();
        assert!(folders.contains(&"Research".to_string()));
        assert!(folders.contains(&"Research/Sb2Se3".to_string()));
        assert!(folders.contains(&"Library".to_string()));
        assert!(!folders.iter().any(|f| f.starts_with('.')), "{folders:?}");
    }

    // ---- the round trip ----------------------------------------------------

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
        assert_eq!(read.summary.folder, "");
        assert!(!read.adopted);
    }

    #[test]
    fn renaming_moves_the_file_but_keeps_the_id_and_the_folder() {
        let vault = TempVault::new();
        let note = vault.create_note("Old title", folder("Research")).unwrap();
        let id = note.summary.id.clone();

        vault.save_note(&id, "New title", "body").unwrap();

        assert!(!vault.root().join("Research/Old title.md").exists());
        assert!(vault.root().join("Research/New title.md").is_file());
        let read = vault.read_note(&id).unwrap();
        assert_eq!(read.summary.title, "New title");
        assert_eq!(read.summary.folder, "Research");
    }

    #[test]
    fn saving_preserves_created_and_advances_updated() {
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        let id = note.summary.id.clone();

        let before = frontmatter::split(&fs::read_to_string(vault.path_for(&id).unwrap()).unwrap())
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
    fn positions_increment_within_a_folder() {
        let vault = TempVault::new();
        let a = vault.create_note("A", folder("Research")).unwrap();
        let b = vault.create_note("B", folder("Research")).unwrap();
        let elsewhere = vault.create_note("C", folder("Library")).unwrap();

        assert_eq!(a.summary.position, 0);
        assert_eq!(b.summary.position, 1);
        // A different folder means a separate sequence.
        assert_eq!(elsewhere.summary.position, 0);
    }

    // ---- tolerating what is already on disk ---------------------------------

    #[test]
    fn listing_skips_files_that_are_not_markdown() {
        let vault = TempVault::new();
        vault.create_note("Real", None).unwrap();
        fs::write(vault.root().join("notes.txt"), "not a note").unwrap();
        fs::write(vault.root().join("data.csv"), "nor this").unwrap();

        let notes = vault.list_notes().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Real");
    }

    #[test]
    fn a_corrupt_note_does_not_break_the_listing() {
        let vault = TempVault::new();
        vault.create_note("Good", None).unwrap();
        // Opening fence with no closing fence: unparseable.
        fs::write(
            vault.root().join("broken.md"),
            "---\nid: x\nno closing fence\n",
        )
        .unwrap();

        let notes = vault.list_notes().unwrap();
        assert_eq!(notes.len(), 1, "the good note must still be listed");
    }

    #[test]
    fn a_plain_markdown_file_is_adopted() {
        let vault = TempVault::new();
        fs::create_dir_all(vault.root().join("Research")).unwrap();
        fs::write(
            vault.root().join("Research/Dropped in.md"),
            "Just prose, no frontmatter.\n",
        )
        .unwrap();

        let listed = vault.list_notes().unwrap();
        let found = listed.iter().find(|n| n.title == "Dropped in").unwrap();

        let note = vault.read_note(&found.id).unwrap();
        assert!(note.adopted);
        assert_eq!(note.summary.folder, "Research");
        assert_eq!(note.body, "Just prose, no frontmatter.\n");
    }

    #[test]
    fn an_adopted_id_is_stable_until_the_note_is_saved() {
        let vault = TempVault::new();
        fs::write(vault.root().join("Stray.md"), "prose\n").unwrap();

        let first = vault.list_notes().unwrap()[0].id.clone();
        let second = vault.list_notes().unwrap()[0].id.clone();
        assert_eq!(first, second, "the same file must keep the same id");

        // Saving gives it a real, permanent one and writes frontmatter.
        vault.save_note(&first, "Stray", "prose").unwrap();
        let after = vault.read_note(&first).unwrap();
        assert!(!after.adopted, "it should have frontmatter now");
        assert_eq!(after.summary.id, first);
    }

    // ---- metadata -----------------------------------------------------------

    #[test]
    fn set_meta_writes_frontmatter_and_leaves_the_body_alone() {
        let vault = TempVault::new();
        let note = vault.create_note("Runs", None).unwrap();
        let id = note.summary.id.clone();
        vault.save_note(&id, "Runs", "The body.").unwrap();

        vault
            .set_meta(&id, Some("🧪".into()), None, vec!["cvt".into()])
            .unwrap();

        let read = vault.read_note(&id).unwrap();
        assert_eq!(read.summary.icon.as_deref(), Some("🧪"));
        assert_eq!(read.summary.tags, vec!["cvt"]);
        assert_eq!(read.body, "The body.\n", "the body must be untouched");
    }

    #[test]
    fn tags_are_normalised_so_one_tag_is_one_tag() {
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        vault
            .set_meta(
                &note.summary.id,
                None,
                None,
                vec![" CVT ".into(), "cvt".into(), "".into(), "Sb2Se3".into()],
            )
            .unwrap();

        let read = vault.read_note(&note.summary.id).unwrap();
        assert_eq!(read.summary.tags, vec!["cvt", "sb2se3"]);
    }

    #[test]
    fn clearing_an_icon_removes_it() {
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        let id = note.summary.id.clone();
        vault
            .set_meta(&id, Some("🧪".into()), None, vec![])
            .unwrap();
        vault
            .set_meta(&id, Some("  ".into()), None, vec![])
            .unwrap();
        assert_eq!(vault.read_note(&id).unwrap().summary.icon, None);
    }

    // ---- deleting -----------------------------------------------------------

    #[test]
    fn delete_moves_to_trash_and_keeps_the_bytes() {
        let vault = TempVault::new();
        let note = vault.create_note("Doomed", folder("Research")).unwrap();
        let id = note.summary.id.clone();
        vault.save_note(&id, "Doomed", "worth keeping").unwrap();

        vault.delete_note(&id).unwrap();

        assert!(vault.read_note(&id).is_err());
        let trash = vault.root().join(SUTRA).join(TRASH);
        let entries: Vec<_> = fs::read_dir(&trash).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);
        // The folder it came from is in the trashed name, so two notes with one
        // title from different folders stay distinguishable.
        assert_eq!(entries[0].file_name(), "Research - Doomed.md");
        let contents = fs::read_to_string(entries[0].path()).unwrap();
        assert!(contents.contains("worth keeping"));
    }

    #[test]
    fn deleting_the_same_name_twice_does_not_overwrite_the_first() {
        let vault = TempVault::new();
        for _ in 0..2 {
            let note = vault.create_note("Twice", None).unwrap();
            vault.delete_note(&note.summary.id).unwrap();
        }
        let trash = vault.root().join(SUTRA).join(TRASH);
        assert_eq!(fs::read_dir(trash).unwrap().count(), 2);
    }

    // ---- attachments ---------------------------------------------------------

    #[test]
    fn attachments_land_beside_their_note_and_out_of_sight() {
        let vault = TempVault::new();
        let source = std::env::temp_dir().join(format!("sutra-src-{}.png", Ulid::generate()));
        fs::write(&source, b"bytes").unwrap();

        let reference = vault
            .import_attachment(&source, folder("Research/Sb2Se3"))
            .unwrap();

        assert!(reference.starts_with("Research/Sb2Se3/.attachments/"));
        assert!(reference.ends_with(".png"));
        assert!(vault.root().join(&reference).is_file());
        // And it is not a note.
        assert!(vault.list_notes().unwrap().is_empty());
        let _ = fs::remove_file(source);
    }

    #[test]
    fn an_attachment_reads_back_by_its_reference() {
        let vault = TempVault::new();
        let source = std::env::temp_dir().join(format!("sutra-src-{}.png", Ulid::generate()));
        fs::write(&source, b"the bytes").unwrap();
        let reference = vault
            .import_attachment(&source, folder("Research"))
            .unwrap();
        assert_eq!(vault.read_attachment(&reference).unwrap(), b"the bytes");
        let _ = fs::remove_file(source);
    }

    #[test]
    fn attachment_reads_cannot_escape_the_attachments_folder() {
        let vault = TempVault::new();
        let note = vault.create_note("Secret", folder("Research")).unwrap();
        let _ = note;

        for attempt in [
            "Research/Secret.md",
            "../../etc/passwd",
            "Research/.attachments/../../Research/Secret.md",
            ".sutra/index.sqlite",
            "/etc/passwd",
            "Research/.attachments/../Secret.md",
            "attachments/../Secret.md",
        ] {
            assert!(
                vault.read_attachment(attempt).is_err(),
                "{attempt} should be refused"
            );
        }
    }

    #[test]
    fn a_missing_attachment_is_an_error_not_a_panic() {
        let vault = TempVault::new();
        assert!(vault.read_attachment("R/.attachments/nope.png").is_err());
    }

    #[test]
    fn two_attachments_with_one_name_do_not_collide() {
        let vault = TempVault::new();
        let source = std::env::temp_dir().join(format!("sutra-src-{}.png", Ulid::generate()));
        fs::write(&source, b"x").unwrap();

        let first = vault.import_attachment(&source, None).unwrap();
        let second = vault.import_attachment(&source, None).unwrap();
        assert_ne!(first, second);
        let _ = fs::remove_file(source);
    }

    // ---- the watcher's view --------------------------------------------------

    #[test]
    fn a_path_resolves_to_the_note_it_holds() {
        let vault = TempVault::new();
        let note = vault.create_note("Watched", folder("Research")).unwrap();
        let path = vault.root().join("Research/Watched.md");
        assert_eq!(
            vault.id_at(&path).as_deref(),
            Some(note.summary.id.as_str())
        );
    }

    #[test]
    fn a_deleted_file_still_resolves_from_the_map() {
        // The watcher hears about a file after it is gone, and still has to
        // know which note to drop from the index.
        let vault = TempVault::new();
        let note = vault.create_note("Vanishing", None).unwrap();
        let path = vault.root().join("Vanishing.md");
        fs::remove_file(&path).unwrap();
        assert_eq!(
            vault.id_at(&path).as_deref(),
            Some(note.summary.id.as_str())
        );
    }

    #[test]
    fn a_file_outside_the_vault_resolves_to_nothing() {
        let vault = TempVault::new();
        assert_eq!(vault.id_at(Path::new("/tmp/elsewhere.md")), None);
    }

    // ---- excerpts -------------------------------------------------------------

    #[test]
    fn an_excerpt_is_the_opening_prose_without_the_markers() {
        let body = "# Growth log\n\n- Source at 560 C\n- **Sink** at 380 C\n";
        assert_eq!(excerpt_of(body), "Growth log Source at 560 C Sink at 380 C");
    }

    #[test]
    fn an_excerpt_keeps_a_minus_sign_that_is_not_a_bullet() {
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
        assert_eq!(excerpt_of("costs $5 total"), "costs $5 total");
    }

    #[test]
    fn an_excerpt_drops_inline_formulas() {
        assert_eq!(
            excerpt_of("Band gap $E_g = 1.2\\,\\mathrm{eV}$ measured"),
            "Band gap  measured"
        );
    }

    #[test]
    fn a_long_excerpt_is_cut_on_a_character_boundary() {
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
        let note = vault.create_note("Anneal", None).unwrap();
        vault
            .save_note(
                &note.summary.id,
                "Anneal",
                "Ramped to 400 C over two hours.",
            )
            .unwrap();
        let listed = vault.list_notes().unwrap();
        assert_eq!(listed[0].excerpt, "Ramped to 400 C over two hours.");
    }
}
