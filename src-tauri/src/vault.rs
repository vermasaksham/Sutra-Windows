//! The vault: a tree of markdown files, and the operations on it.
//!
//! Notes live in real, nested directories, because a folder tree is the part
//! of the layout a person reads. Identity does not: the ULID lives inside the
//! file, in frontmatter. Keeping those two apart is what lets a note be
//! renamed and moved freely without a single `[[id]]` link anywhere in the
//! vault having to change.

use crate::citations;
use crate::error::{Result, SutraError};
use crate::frontmatter::{self, Citation, Frontmatter, NoteType, SourceMeta};
use crate::note;
use crate::tags;
use crate::views;
use serde::{Deserialize, Serialize};
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

/// Where attachments lived before they moved beside their notes. Still read,
/// never written — an old vault's pictures have to keep working.
const LEGACY_ATTACHMENTS: &str = "attachments";

/// Where a capture lands when the user has not said where it belongs.
///
/// An ordinary folder, deliberately: it can be opened in Explorer, notes can be
/// dragged out of it by hand, and nothing breaks if someone deletes it.
pub const INBOX: &str = "Inbox";

/// Where source notes are kept, so the note explorer is not half papers.
///
/// A convention, not a rule: a source is an ordinary note and works from
/// anywhere. This is only where new ones are put.
pub const LIBRARY: &str = "Library";

/// Where saved views are kept, for the same reason and with the same force: a
/// convention, not a rule.
pub const VIEWS: &str = "Views";

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
    #[serde(rename = "type")]
    pub note_type: NoteType,
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
    /// Present on a note of `type: source`. What the paper is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceMeta>,
    /// The sources this note draws on.
    ///
    /// Carried on the summary rather than only the full note because the index
    /// needs it to answer "what cites this source", and every path that
    /// re-indexes a note already has a summary in hand. The cost is that a
    /// vault listing carries the quotes too; at a few hundred literature notes
    /// that is tens of kilobytes, and if it ever stops being negligible the fix
    /// is to split the type rather than to duplicate the plumbing now.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Citation>,
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
        fs::create_dir_all(root.join(INBOX))?;
        // A leading dot means nothing to Windows Explorer, and this is a
        // Windows-first application. Without this the app's own folder sits in
        // the middle of the user's research vault looking like theirs.
        hide_from_explorer(&root.join(SUTRA));
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
        fm.tags = tags::normalise_all(tags);
        fm.updated = frontmatter::now();

        let body = body.to_string();
        note::write_atomic(&path, &frontmatter::join(&fm, &body)?)?;
        Ok(summary_of(&fm, &body, folder_of(&relative)))
    }

    /// Change what kind of note this is.
    ///
    /// Its own operation rather than another argument to `set_meta`, because
    /// the type is not page decoration: it decides which views a note falls
    /// into, and a call that changes it should say so.
    pub fn set_type(&self, id: &str, note_type: NoteType) -> Result<NoteSummary> {
        let relative = self.relative_for(id)?;
        let path = self.root.join(&relative);
        let contents = fs::read_to_string(&path)?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let mut fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
        if fm.id != id {
            fm.id = id.to_string();
        }
        fm.note_type = note_type;
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
        hide_from_explorer(&directory);
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
        // Either the hidden folder beside a note, or — for a vault written
        // before attachments moved there — the old top-level `attachments/`.
        // References live in note bodies, so refusing the old spelling would
        // break every picture in an existing vault to no purpose.
        let directory = relative.parent().and_then(|p| p.file_name());
        let in_attachments = directory.is_some_and(|n| n == ATTACHMENTS)
            || (directory.is_some_and(|n| n == LEGACY_ATTACHMENTS)
                && relative.components().count() == 2);
        if !in_attachments {
            return Err(refused());
        }

        Ok(fs::read(self.root.join(relative))?)
    }

    // ---- migrating a vault laid out the old way -----------------------------

    /// Whether this vault still records its hierarchy in frontmatter.
    ///
    /// True the moment any note claims a `parent`. That claim is now dead
    /// weight — folders are the truth — but it is not thrown away, so the
    /// hierarchy someone built is still recoverable until they say what to do
    /// with it.
    pub fn needs_migration(&self) -> Result<bool> {
        Ok(self.legacy_notes()?.0.iter().any(|n| n.parent.is_some()))
    }

    /// What migrating would do, without doing any of it.
    ///
    /// Shown before anything moves, because reorganising someone's research
    /// vault on their behalf without telling them first is the failure this
    /// whole design is trying to avoid.
    pub fn migration_plan(&self) -> Result<MigrationPlan> {
        let (notes, skipped) = self.legacy_notes()?;
        let by_id: HashMap<&str, &LegacyNote> = notes.iter().map(|n| (n.id.as_str(), n)).collect();

        let mut moves = Vec::new();
        let mut flattened = Vec::new();
        // Tracks names already claimed in each target folder, so two notes
        // that would land on one filename get a suffix instead of one of them
        // overwriting the other.
        let mut taken: HashMap<String, Vec<String>> = HashMap::new();

        for note in &notes {
            let (ancestors, deep) = ancestry(note, &by_id);
            if deep {
                flattened.push(note.title.clone());
            }

            let folder = ancestors.join("/");
            let claimed = taken.entry(folder.clone()).or_default();
            let stem = note::file_stem(&note.title);
            let mut name = format!("{stem}.md");
            let mut attempt = 1;
            while claimed.contains(&name) {
                attempt += 1;
                name = format!("{stem} {attempt}.md");
            }
            claimed.push(name.clone());

            let to = join_relative(&folder, &name);
            if to != note.relative {
                moves.push((note.relative.clone(), to));
            }
        }

        moves.sort();
        Ok(MigrationPlan {
            moves,
            flattened,
            skipped,
        })
    }

    /// Carry out the plan. Returns how many files moved.
    ///
    /// Every markdown file is copied into `.sutra/backups/` first. Only the
    /// markdown — attachments are not touched by any of this, and copying a
    /// vault's worth of PDFs to rename some text files would be absurd.
    ///
    /// Renames happen before any frontmatter is rewritten, so an interrupted
    /// run leaves files in their new homes still claiming their old parents,
    /// which is exactly the state a second run knows how to finish.
    pub fn migrate(&self) -> Result<usize> {
        let plan = self.migration_plan()?;
        self.back_up()?;

        for (from, to) in &plan.moves {
            let target = self.root.join(to);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(self.root.join(from), &target)?;
        }

        // Now the claim is redundant, and a redundant claim is one that can
        // disagree with the truth later. Timestamps are left alone: moving a
        // file is not editing a note.
        for note in self.legacy_notes()?.0 {
            if note.parent.is_none() {
                continue;
            }
            let path = self.root.join(&note.relative);
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok((Some(mut fm), body)) = frontmatter::split(&contents) else {
                continue;
            };
            fm.parent = None;
            let body = body.to_string();
            note::write_atomic(&path, &frontmatter::join(&fm, &body)?)?;
        }

        self.list_notes()?;
        Ok(plan.moves.len())
    }

    /// Copy every markdown file into a timestamped folder under `.sutra`.
    fn back_up(&self) -> Result<PathBuf> {
        let directory = self
            .root
            .join(SUTRA)
            .join("backups")
            .join(Ulid::generate().to_string());

        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files)?;
        for relative in files {
            let target = directory.join(&relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(self.root.join(&relative), &target)?;
        }
        Ok(directory)
    }

    /// Every note with the fields the migration reasons about, and the paths of
    /// the files it could not read.
    ///
    /// The second list matters. A hand-edited note can easily hold frontmatter
    /// that is not valid YAML — `title: Cp: 300 K` is one colon away — and such
    /// a file is left exactly where it is. Leaving it is right; leaving it
    /// *quietly* is not, so the plan says which ones and the dialog shows them.
    fn legacy_notes(&self) -> Result<(Vec<LegacyNote>, Vec<String>)> {
        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files)?;

        let mut out = Vec::new();
        let mut skipped = Vec::new();
        for relative in files {
            let Ok(contents) = fs::read_to_string(self.root.join(&relative)) else {
                skipped.push(relative);
                continue;
            };
            match frontmatter::split(&contents) {
                Ok((Some(fm), _)) => out.push(LegacyNote {
                    id: fm.id,
                    title: fm.title,
                    parent: fm.parent,
                    relative,
                }),
                // No frontmatter at all is not a problem: the file has no
                // parent to honour, so leaving it where it is loses nothing.
                Ok((None, _)) => {}
                Err(_) => skipped.push(relative),
            }
        }
        skipped.sort();
        Ok((out, skipped))
    }

    // ---- sources -------------------------------------------------------------

    /// Create a source note in the library.
    ///
    /// A source is a note like any other — it can be written in, linked to,
    /// tagged and moved. That is the whole point: a citation that points at a
    /// note keeps meaning something when Zotero is not installed, which a
    /// citation pointing at a Zotero key does not.
    pub fn create_source(&self, title: &str, meta: SourceMeta) -> Result<NoteDoc> {
        let folder = self.checked_folder(LIBRARY)?;
        fs::create_dir_all(self.root.join(&folder))?;

        let id = Ulid::generate().to_string();
        let title = if title.trim().is_empty() {
            "Untitled source"
        } else {
            title.trim()
        };
        let mut fm = Frontmatter::new(id.clone(), title.to_string());
        fm.note_type = NoteType::Source;
        fm.source = Some(meta);

        let relative = join_relative(&folder, &unique_name(&self.root.join(&folder), title, None));
        note::write_atomic(&self.root.join(&relative), &frontmatter::join(&fm, "")?)?;
        self.remember(&id, &relative);

        Ok(NoteDoc {
            summary: summary_of(&fm, "", folder),
            body: String::new(),
            adopted: false,
        })
    }

    /// Create a literature note about a source.
    ///
    /// The note is the researcher's, not the paper's: it holds their reading,
    /// and the paper's own details stay on the source note it cites. That
    /// separation is the point of the whole feature — a summary a person wrote
    /// and a summary a publisher wrote must never end up in the same paragraph
    /// with no way to tell them apart.
    ///
    /// So the body is headings and nothing else, except the abstract, which is
    /// included because reading a literature note offline without the paper's
    /// own claim in front of you is most of the value gone — and which is
    /// marked, in the body, as the publisher's words. Nothing here is ever
    /// filled in on the user's behalf.
    pub fn create_literature_note(
        &self,
        title: &str,
        folder: Option<String>,
        source_id: &str,
        abstract_text: Option<&str>,
    ) -> Result<NoteDoc> {
        let doc = self.create_note(title, folder)?;
        let body = literature_body(abstract_text);
        self.edit(&doc.summary.id, |fm| {
            fm.note_type = NoteType::Literature;
            fm.sources = vec![Citation {
                id: source_id.to_string(),
                captured: Some(frontmatter::now()),
                ..Default::default()
            }];
        })?;
        let summary = self.save_note(&doc.summary.id, title, &body)?;
        Ok(NoteDoc {
            summary,
            body,
            adopted: false,
        })
    }

    /// Every source note that came from the reference manager.
    ///
    /// Returns the note id beside the library key, because restyling needs
    /// both: the key to ask the library about, and the id to write the answer
    /// back to.
    pub fn linked_sources(&self) -> Result<Vec<(String, String)>> {
        Ok(self
            .list_notes()?
            .into_iter()
            .filter(|n| n.note_type == NoteType::Source)
            .filter_map(|n| {
                n.source
                    .as_ref()
                    .and_then(|meta| meta.zotero.clone())
                    .map(|key| (n.id.clone(), key))
            })
            .collect())
    }

    /// Cache one rendered citation on a source note.
    ///
    /// Additive: a style already cached under another id is left alone, so
    /// switching style and switching back costs nothing. `updated` is
    /// deliberately not stamped — caching how a paper is *formatted* is not an
    /// edit to the note, and a vault whose timestamps move because someone
    /// changed a dropdown has lost real information about when the work
    /// happened.
    pub fn cache_style(
        &self,
        id: &str,
        style: &str,
        styled: crate::references::StyledCitation,
    ) -> Result<()> {
        self.amend(id, |fm| {
            if let Some(meta) = fm.source.as_mut() {
                meta.styled.insert(style.to_string(), styled.clone());
            }
        })
    }

    /// Replace what a source note records about its paper.
    ///
    /// Every note citing it shows the new details immediately, because none of
    /// them holds a copy — they hold the source's id, and the details are read
    /// from the one note that owns them.
    pub fn set_source_meta(&self, id: &str, meta: SourceMeta) -> Result<NoteSummary> {
        self.edit(id, |fm| {
            fm.note_type = NoteType::Source;
            fm.source = Some(meta);
        })
    }

    /// Replace a note's citations. The caller sends the complete desired list,
    /// for the same reason `set_meta` does.
    pub fn set_citations(&self, id: &str, citations: Vec<Citation>) -> Result<NoteSummary> {
        self.edit(id, |fm| {
            fm.sources = citations;
        })
    }

    /// Every source note in the vault.
    pub fn list_sources(&self) -> Result<Vec<NoteSummary>> {
        Ok(self
            .list_notes()?
            .into_iter()
            .filter(|n| n.note_type == NoteType::Source)
            .collect())
    }

    // ---- views ---------------------------------------------------------------

    /// Create a view note holding `query`.
    ///
    /// A view is a note, so this is the same three lines as creating any other
    /// one. Its body is empty and stays yours: the place to write down why the
    /// view exists, which is the thing that stops a saved search from rotting
    /// into a list nobody remembers the purpose of.
    pub fn create_view(&self, title: &str, query: views::Query) -> Result<NoteDoc> {
        let folder = self.checked_folder(VIEWS)?;
        fs::create_dir_all(self.root.join(&folder))?;

        let id = Ulid::generate().to_string();
        let title = if title.trim().is_empty() {
            "Untitled view"
        } else {
            title.trim()
        };
        let mut fm = Frontmatter::new(id.clone(), title.to_string());
        fm.note_type = NoteType::View;
        fm.view = Some(query);

        let relative = join_relative(&folder, &unique_name(&self.root.join(&folder), title, None));
        note::write_atomic(&self.root.join(&relative), &frontmatter::join(&fm, "")?)?;
        self.remember(&id, &relative);

        Ok(NoteDoc {
            summary: summary_of(&fm, "", folder),
            body: String::new(),
            adopted: false,
        })
    }

    /// The query a view note holds.
    ///
    /// `None` for a note that is not a view, or a view whose block was deleted
    /// by hand — both of which are recoverable states, not errors: the note is
    /// still there and still says what it was for.
    pub fn view_query(&self, id: &str) -> Result<Option<views::Query>> {
        let relative = self.relative_for(id)?;
        let contents = fs::read_to_string(self.root.join(&relative))?;
        let (parsed, _) = frontmatter::split(&contents)?;
        Ok(parsed.and_then(|fm| fm.view))
    }

    /// Replace a view note's query. Makes the note a view if it was not one.
    pub fn set_view_query(&self, id: &str, query: views::Query) -> Result<NoteSummary> {
        self.edit(id, |fm| {
            fm.note_type = NoteType::View;
            fm.view = Some(query);
        })
    }

    /// Every view note in the vault, wherever it sits.
    pub fn list_views(&self) -> Result<Vec<NoteSummary>> {
        Ok(self
            .list_notes()?
            .into_iter()
            .filter(|n| n.note_type == NoteType::View)
            .collect())
    }

    /// The source note already standing for this Zotero item, if there is one.
    ///
    /// Importing the same paper twice must update one note rather than making
    /// a second, or the vault grows a duplicate every time a citation is added.
    pub fn source_for_zotero(&self, key: &str) -> Result<Option<NoteSummary>> {
        Ok(self.list_sources()?.into_iter().find(|n| {
            n.source
                .as_ref()
                .and_then(|s| s.zotero.as_deref())
                .is_some_and(|k| k == key)
        }))
    }

    /// Every legacy `[@KEY]` citation in the vault, with how many notes use it.
    ///
    /// These only mean something while Zotero is running. Finding them is the
    /// first half of getting rid of them.
    pub fn legacy_citations(&self) -> Result<HashMap<String, usize>> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files)?;

        for relative in files {
            let Ok(contents) = fs::read_to_string(self.root.join(&relative)) else {
                continue;
            };
            let Ok((_, body)) = frontmatter::split(&contents) else {
                continue;
            };
            for key in citations::legacy_keys(body) {
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    /// Point every `[@KEY]` at the source note that now stands for it.
    ///
    /// `mapping` is Zotero key to source note id, built by the caller because
    /// producing it needs the network and this does not. Keys absent from the
    /// mapping are left exactly as they are: a citation nobody can resolve is
    /// still better than one silently deleted, and the migration can be run
    /// again once Zotero can answer for them.
    ///
    /// Timestamps are untouched. Rewriting a reference into the form that
    /// means the same thing is not an edit, and stamping `updated` across a
    /// whole vault would destroy the one signal telling you what you were
    /// actually working on.
    pub fn migrate_citations(&self, mapping: &HashMap<String, String>) -> Result<usize> {
        let mut changed = 0;
        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files)?;

        for relative in files {
            let path = self.root.join(&relative);
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok((Some(fm), body)) = frontmatter::split(&contents) else {
                continue;
            };

            let mut rewritten = body.to_string();
            for (key, id) in mapping {
                rewritten = citations::rewrite(&rewritten, key, id);
            }
            if rewritten == body {
                continue;
            }

            note::write_atomic(&path, &frontmatter::join(&fm, &rewritten)?)?;
            changed += 1;
        }
        Ok(changed)
    }

    /// Take a source's details into the vault, updating rather than duplicating.
    ///
    /// Keyed on the Zotero item key, so importing the same paper on Monday and
    /// again on Friday leaves one note with Friday's details. A source typed in
    /// by hand has no key and is never matched by this — which is right: two
    /// hand-written sources with the same title are the user's business.
    pub fn import_source(&self, title: &str, meta: SourceMeta) -> Result<NoteSummary> {
        let existing = match meta.zotero.as_deref() {
            Some(key) => self.source_for_zotero(key)?,
            None => None,
        };
        match existing {
            Some(found) => self.set_source_meta(&found.id, meta),
            None => Ok(self.create_source(title, meta)?.summary),
        }
    }

    /// Read, change, write. Every metadata setter is this shape.
    fn edit(&self, id: &str, change: impl FnOnce(&mut Frontmatter)) -> Result<NoteSummary> {
        let relative = self.relative_for(id)?;
        let path = self.root.join(&relative);
        let contents = fs::read_to_string(&path)?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let mut fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
        if fm.id != id {
            fm.id = id.to_string();
        }
        change(&mut fm);
        fm.updated = frontmatter::now();

        let body = body.to_string();
        note::write_atomic(&path, &frontmatter::join(&fm, &body)?)?;
        Ok(summary_of(&fm, &body, folder_of(&relative)))
    }

    // ---- duplicates ----------------------------------------------------------

    /// Record that two notes are not duplicates of each other.
    ///
    /// Written on both, so either can filter its own suggestions without
    /// consulting the other, and so the fact survives in the markdown rather
    /// than only in a database that is meant to be disposable.
    ///
    /// `updated` is deliberately left alone. Saying "these two are different
    /// notes" is a statement about a suggestion, not an edit to either note,
    /// and a vault whose timestamps move when someone dismisses a prompt has
    /// lost real information about when the work happened.
    pub fn not_duplicates(&self, a: &str, b: &str) -> Result<()> {
        for (note, other) in [(a, b), (b, a)] {
            self.amend(note, |fm| {
                if !fm.not_duplicates.iter().any(|id| id == other) {
                    fm.not_duplicates.push(other.to_string());
                }
            })?;
        }
        Ok(())
    }

    /// The notes this one has been said not to duplicate.
    pub fn dismissed_duplicates(&self, id: &str) -> Result<Vec<String>> {
        let relative = self.relative_for(id)?;
        let contents = fs::read_to_string(self.root.join(&relative))?;
        let (parsed, _) = frontmatter::split(&contents)?;
        Ok(parsed.map(|fm| fm.not_duplicates).unwrap_or_default())
    }

    /// Fold `absorb` into `keep`, then delete it.
    ///
    /// What "merge" has to mean if it is to be safe:
    ///
    /// - Nothing is thrown away. The absorbed body is appended under a heading
    ///   naming where it came from, rather than interleaved, because a person
    ///   has to be able to see afterwards which half was which.
    /// - Tags and citations are unioned, so provenance the absorbed note
    ///   carried is not lost with it.
    /// - Every `[[link]]` pointing at the absorbed note is rewritten to point
    ///   at the kept one, so no note in the vault is left holding a dead
    ///   reference.
    /// - The absorbed note goes to the trash rather than being unlinked, so
    ///   the whole operation is recoverable by hand.
    ///
    /// Returns the kept note.
    pub fn merge_notes(&self, keep: &str, absorb: &str) -> Result<NoteSummary> {
        if keep == absorb {
            return Err(SutraError::NoteNotFound(absorb.to_string()));
        }
        let taken = self.read_note(absorb)?;
        let kept = self.read_note(keep)?;

        let mut body = kept.body.trim_end().to_string();
        let addition = taken.body.trim();
        if !addition.is_empty() {
            if !body.is_empty() {
                body.push_str("\n\n");
            }
            body.push_str(&format!(
                "## Merged from {}\n\n{addition}\n",
                taken.summary.title
            ));
        }

        let mut tags = kept.summary.tags.clone();
        for tag in taken.summary.tags {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        let mut citations = kept.summary.sources.clone();
        for citation in taken.summary.sources {
            if !citations.iter().any(|c| {
                c.id == citation.id && c.page == citation.page && c.quote == citation.quote
            }) {
                citations.push(citation);
            }
        }

        self.edit(keep, |fm| {
            fm.tags = tags;
            fm.sources = citations;
            // The pair cannot be offered again: one of them is gone.
            fm.not_duplicates.retain(|id| id != absorb);
        })?;
        self.save_note(keep, &kept.summary.title, &body)?;
        self.repoint_links(absorb, keep)?;
        self.delete_note(absorb)?;
        Ok(self.read_note(keep)?.summary)
    }

    /// Point every `[[from]]` in the vault at `to`.
    ///
    /// Only reached by a merge, where the target is about to stop existing.
    /// Ordinary moves and renames never need this — that is the whole point of
    /// the id living in frontmatter — and it is written here rather than
    /// offered generally so it stays that way.
    fn repoint_links(&self, from: &str, to: &str) -> Result<usize> {
        let needle = format!("[[{from}]]");
        let replacement = format!("[[{to}]]");
        let mut changed = 0;
        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files)?;

        for relative in files {
            let path = self.root.join(&relative);
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            if !contents.contains(&needle) {
                continue;
            }
            // A file whose frontmatter will not parse is left exactly as it
            // is. Rewriting it would mean writing back a block we could not
            // read, and a dangling link is a smaller loss than that.
            let Ok((Some(fm), body)) = frontmatter::split(&contents) else {
                continue;
            };
            if fm.id == from {
                continue;
            }
            let body = body.replace(&needle, &replacement);
            // Rewriting a link into the one that means the same thing is not
            // an edit, for the same reason migrating a citation is not.
            note::write_atomic(&path, &frontmatter::join(&fm, &body)?)?;
            changed += 1;
        }
        Ok(changed)
    }

    /// Read, change, write, without touching `updated`.
    ///
    /// The bookkeeping twin of [`Vault::edit`]. Used where what is being
    /// written is a fact about a suggestion rather than a change to the note.
    fn amend(&self, id: &str, change: impl FnOnce(&mut Frontmatter)) -> Result<()> {
        let relative = self.relative_for(id)?;
        let path = self.root.join(&relative);
        let contents = fs::read_to_string(&path)?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let mut fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
        if fm.id != id {
            fm.id = id.to_string();
        }
        change(&mut fm);
        let body = body.to_string();
        note::write_atomic(&path, &frontmatter::join(&fm, &body)?)?;
        Ok(())
    }

    // ---- tags ----------------------------------------------------------------

    /// Every tag in the vault, exactly as written, with how many notes carry it.
    ///
    /// As written, not rolled up: a suggestion to merge two tags has to be
    /// about tags someone actually typed, and the implied ancestors of
    /// `research/materials/sb2se3` were never typed by anyone.
    pub fn list_tags(&self) -> Result<HashMap<String, usize>> {
        let mut counts = HashMap::new();
        for note in self.list_notes()? {
            for tag in note.tags {
                *counts.entry(tag).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    /// Tags that look like they were meant to be the same. Offered, never applied.
    pub fn similar_tags(&self) -> Result<Vec<tags::Suggestion>> {
        Ok(tags::similar(&self.list_tags()?))
    }

    /// Rename a tag across the whole vault, or merge it into another.
    ///
    /// One operation for both, because they are the same edit: renaming onto a
    /// name that already exists *is* a merge, and pretending otherwise would
    /// mean two code paths that must agree about hierarchy and de-duplication.
    ///
    /// Hierarchy comes along. Renaming `research/materials` to `materials` also
    /// moves `research/materials/sb2se3` to `materials/sb2se3`, because a tag
    /// tree that only half-moves is worse than one that does not move at all.
    ///
    /// Returns what every touched note's tags used to be, which is what makes
    /// this undoable — including for a merge, where the inverse rename would
    /// not restore the original state.
    pub fn retag(&self, from: &str, to: &str) -> Result<Retag> {
        let from = tags::normalise(from)
            .ok_or_else(|| SutraError::NoteNotFound(format!("not a tag: {from}")))?;
        let to = tags::normalise(to)
            .ok_or_else(|| SutraError::NoteNotFound(format!("not a tag: {to}")))?;

        let mut changed = Vec::new();
        if from == to {
            return Ok(Retag { changed });
        }

        let prefix = format!("{from}/");
        for summary in self.list_notes()? {
            if !summary
                .tags
                .iter()
                .any(|t| *t == from || t.starts_with(&prefix))
            {
                continue;
            }
            let previous = summary.tags.clone();
            let rewritten: Vec<String> = previous
                .iter()
                .map(|tag| {
                    if *tag == from {
                        to.clone()
                    } else if let Some(rest) = tag.strip_prefix(&prefix) {
                        format!("{to}/{rest}")
                    } else {
                        tag.clone()
                    }
                })
                .collect();

            // Normalising again is what collapses a merge: two tags that have
            // just become the same one must not both survive.
            self.write_tags(&summary.id, tags::normalise_all(rewritten))?;
            changed.push(TagChange {
                id: summary.id,
                previous,
            });
        }

        Ok(Retag { changed })
    }

    /// Put the tags back exactly as they were before a retag.
    ///
    /// Replays a recording rather than inverting an operation, so it undoes a
    /// merge as faithfully as a rename. Notes deleted in the meantime are
    /// skipped rather than failing the whole undo.
    pub fn undo_retag(&self, changed: &[TagChange]) -> Result<usize> {
        let mut restored = 0;
        for entry in changed {
            if self.write_tags(&entry.id, entry.previous.clone()).is_ok() {
                restored += 1;
            }
        }
        Ok(restored)
    }

    /// Replace one note's tags, touching nothing else.
    fn write_tags(&self, id: &str, tags: Vec<String>) -> Result<()> {
        let relative = self.relative_for(id)?;
        let path = self.root.join(&relative);
        let contents = fs::read_to_string(&path)?;
        let (parsed, body) = frontmatter::split(&contents)?;
        let mut fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
        if fm.id != id {
            fm.id = id.to_string();
        }
        fm.tags = tags;
        fm.updated = frontmatter::now();
        let body = body.to_string();
        note::write_atomic(&path, &frontmatter::join(&fm, &body)?)
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

/// One note's tags before a retag, so the operation can be undone exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagChange {
    pub id: String,
    pub previous: Vec<String>,
}

/// What a retag did, and everything needed to put it back.
#[derive(Debug, Clone, Serialize)]
pub struct Retag {
    pub changed: Vec<TagChange>,
}

/// What a migration would do.
#[derive(Debug, Clone, Serialize)]
pub struct MigrationPlan {
    /// Vault-relative `from` and `to`, sorted so the list reads stably.
    pub moves: Vec<(String, String)>,
    /// Notes whose chain of parents was deeper than the folder cap, so they
    /// were placed as deep as folders go rather than deeper.
    pub flattened: Vec<String>,
    /// Files whose frontmatter could not be parsed. Left untouched.
    pub skipped: Vec<String>,
}

/// A note as the migration sees it: an id, a title, a claimed parent, a path.
struct LegacyNote {
    id: String,
    title: String,
    parent: Option<String>,
    relative: String,
}

/// The folder names a note's ancestors imply, outermost first.
///
/// Returns whether the chain had to be cut short. A note that was six deep in
/// the old tree cannot be six folders deep in the new one, so it is placed at
/// the cap — a note in a shallower folder than you expected is recoverable, a
/// note the filesystem refused to create is not.
///
/// A `parent` pointing at nothing, or at a cycle, yields a shorter chain rather
/// than an error. These files are hand-editable and both happen.
fn ancestry(note: &LegacyNote, by_id: &HashMap<&str, &LegacyNote>) -> (Vec<String>, bool) {
    let mut chain = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = note.parent.as_deref();

    while let Some(id) = current {
        if !seen.insert(id.to_string()) {
            break;
        }
        let Some(ancestor) = by_id.get(id) else { break };
        chain.push(note::file_stem(&ancestor.title));
        current = ancestor.parent.as_deref();
    }

    chain.reverse();
    let deep = chain.len() > MAX_DEPTH;
    chain.truncate(MAX_DEPTH);
    (chain, deep)
}

/// Mark a directory hidden, where the platform has such a concept.
///
/// On Unix a leading dot is the whole story and this does nothing. On Windows a
/// dot is just a character — `.sutra` shows up in Explorer like any other
/// folder — so the attribute has to be set explicitly.
///
/// Failure is ignored on purpose. A vault on a FAT volume, a network share, or
/// a directory someone else owns may refuse, and a visible folder is a
/// cosmetic problem, not a reason to fail opening the vault.
#[cfg(windows)]
fn hide_from_explorer(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_HIDDEN, GetFileAttributesW, INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives both calls,
    // which is the whole contract of these two functions.
    unsafe {
        let current = GetFileAttributesW(wide.as_ptr());
        if current == INVALID_FILE_ATTRIBUTES || current & FILE_ATTRIBUTE_HIDDEN != 0 {
            return;
        }
        SetFileAttributesW(wide.as_ptr(), current | FILE_ATTRIBUTE_HIDDEN);
    }
}

#[cfg(not(windows))]
fn hide_from_explorer(_path: &Path) {}

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

/// The sections a literature note starts with.
///
/// Section 7's list, in its order. Empty on purpose: the app supplies the
/// shape of a reading, never the reading. An assistant may later offer text
/// for Summary or Key Evidence, but it arrives as a draft the user accepts,
/// and it is never written here at creation time where it would be
/// indistinguishable from something they wrote themselves.
fn literature_body(abstract_text: Option<&str>) -> String {
    let mut out = String::new();

    // The publisher's words, marked as such. A blockquote rather than a
    // paragraph because the distinction between what the paper claims and what
    // the reader concluded has to survive being skim-read at midnight.
    if let Some(text) = abstract_text.map(str::trim).filter(|t| !t.is_empty()) {
        out.push_str("> **Abstract, as published.** ");
        out.push_str(&text.replace('\n', " "));
        out.push_str("\n\n");
    }

    for heading in [
        "Summary",
        "Key Evidence",
        "Important Quotes",
        "My Interpretation",
        "Research Questions",
        "Limitations",
        "Related Notes",
    ] {
        out.push_str("## ");
        out.push_str(heading);
        out.push_str("\n\n");
    }
    out
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

fn summary_of(fm: &Frontmatter, body: &str, folder: String) -> NoteSummary {
    NoteSummary {
        id: fm.id.clone(),
        note_type: fm.note_type,
        title: fm.title.clone(),
        folder,
        position: fm.position,
        tags: fm.tags.clone(),
        icon: fm.icon.clone(),
        cover: fm.cover.clone(),
        source: fm.source.clone(),
        sources: fm.sources.clone(),
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

/// One heading found somewhere in the vault.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heading {
    /// The note it is in.
    pub note: String,
    pub note_title: String,
    /// The heading's own text, exactly as written.
    pub text: String,
    /// How much prose follows it, before the next heading. Zero means the
    /// question was asked and nothing has been written under it yet.
    pub words: usize,
}

/// What the research overview is built from.
///
/// Deliberately *not* an analysis. This gathers what is already written and
/// counts it; deciding which question matters is the researcher's job, and a
/// dashboard that ranked them would be inventing a judgement it cannot make.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    /// Every heading in the vault, in note order. The frontend classifies them
    /// by voice — those rules live in one place, in TypeScript, and copying
    /// them into Rust would be two definitions of the same idea.
    pub headings: Vec<Heading>,
    /// Source note id -> how many notes cite it. A source missing from this
    /// map is cited by nothing.
    pub citations: HashMap<String, usize>,
    /// Every source note, so "imported but never cited" can be shown.
    pub sources: Vec<NoteSummary>,
    /// How many citations carry a page reference, and how many of those also
    /// carry the source's own words. The provenance record, counted.
    pub with_page: usize,
    pub with_quote: usize,
}

impl Vault {
    /// Read the whole vault once and gather what a research overview needs.
    ///
    /// One pass, because the alternative is a command per source and a body
    /// fetch per note — and at a few thousand notes that is the difference
    /// between a panel that opens and one that hangs.
    pub fn overview(&self) -> Result<Overview> {
        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files)?;

        let mut headings = Vec::new();
        let mut citations: HashMap<String, usize> = HashMap::new();
        let mut sources = Vec::new();
        let (mut with_page, mut with_quote) = (0, 0);

        for relative in files {
            let Ok(contents) = fs::read_to_string(self.root.join(&relative)) else {
                continue;
            };
            let Ok((parsed, body)) = frontmatter::split(&contents) else {
                continue;
            };
            let fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));

            for citation in &fm.sources {
                *citations.entry(citation.id.clone()).or_default() += 1;
                if citation
                    .page
                    .as_deref()
                    .is_some_and(|p| !p.trim().is_empty())
                {
                    with_page += 1;
                }
                if citation
                    .quote
                    .as_deref()
                    .is_some_and(|q| !q.trim().is_empty())
                {
                    with_quote += 1;
                }
            }

            let summary = summary_of(&fm, body, folder_of(&relative));
            if summary.note_type == NoteType::Source {
                sources.push(summary.clone());
            }

            for (text, words) in headings_in(body) {
                headings.push(Heading {
                    note: fm.id.clone(),
                    note_title: fm.title.clone(),
                    text,
                    words,
                });
            }
        }

        sources.sort_by_key(|s| s.title.to_lowercase());
        Ok(Overview {
            headings,
            citations,
            sources,
            with_page,
            with_quote,
        })
    }
}

/// Every ATX heading in a body, with how many words follow it.
///
/// Fenced code is skipped: `# include <stdio.h>` inside a listing is not a
/// heading, and counting it as one would put C in a list of research
/// questions.
fn headings_in(body: &str) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut fenced = false;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let text = rest.trim_start_matches('#').trim();
            if !text.is_empty() {
                out.push((text.to_string(), 0));
            }
            continue;
        }
        // Prose belongs to the heading above it. A blockquote counts: under a
        // source-voice heading, the quote *is* the content.
        if let Some(last) = out.last_mut() {
            last.1 += trimmed.split_whitespace().count();
        }
    }
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
    /// Migrate a real vault named by $SUTRA_VAULT, so the result can be looked
    /// at with something other than our own assertions.
    ///
    /// Run with `SUTRA_VAULT=... cargo test --bins -- --ignored migrate_a_real_vault`.
    #[test]
    #[ignore]
    fn migrate_a_real_vault() {
        let root = std::path::PathBuf::from(std::env::var("SUTRA_VAULT").unwrap());
        let vault = Vault::open(root).unwrap();
        let plan = vault.migration_plan().unwrap();
        for (from, to) in &plan.moves {
            println!("  {from}  ->  {to}");
        }
        println!("flattened: {:?}", plan.flattened);
        println!("moved {} files", vault.migrate().unwrap());
    }

    // ---- types and capture ---------------------------------------------------

    #[test]
    fn a_vault_always_has_an_inbox() {
        let vault = TempVault::new();
        assert!(vault.root().join(INBOX).is_dir());
    }

    #[test]
    fn a_captured_note_needs_no_decisions() {
        let vault = TempVault::new();
        // No title, no folder, no type — that is the whole point.
        let doc = vault.create_note("", Some(INBOX.to_string())).unwrap();
        assert_eq!(doc.summary.folder, INBOX);
        assert_eq!(doc.summary.note_type, NoteType::Standard);
        assert!(vault.root().join("Inbox/Untitled.md").is_file());
    }

    #[test]
    fn a_notes_type_can_be_changed_later() {
        let vault = TempVault::new();
        let note = vault.create_note("Zhou 2019", folder("Library")).unwrap();
        let id = note.summary.id.clone();
        vault
            .save_note(&id, "Zhou 2019", "Quasi-1D ribbons.")
            .unwrap();

        let after = vault.set_type(&id, NoteType::Literature).unwrap();
        assert_eq!(after.note_type, NoteType::Literature);

        let read = vault.read_note(&id).unwrap();
        assert_eq!(read.summary.note_type, NoteType::Literature);
        assert_eq!(read.body, "Quasi-1D ribbons.\n", "the body is untouched");
    }

    #[test]
    fn the_inbox_is_an_ordinary_folder_notes_can_leave() {
        let vault = TempVault::new();
        let doc = vault.create_note("", Some(INBOX.to_string())).unwrap();
        let moved = vault.move_note(&doc.summary.id, "Research/Sb2Se3").unwrap();
        assert_eq!(moved.folder, "Research/Sb2Se3");
        assert!(!vault.root().join("Inbox/Untitled.md").exists());
    }

    // ---- migration -----------------------------------------------------------

    /// Write a note the old way: flat in the root, id in the filename, and the
    /// hierarchy claimed in frontmatter.
    fn legacy(vault: &Vault, id: &str, title: &str, parent: Option<&str>) {
        let mut fm = Frontmatter::new(id.to_string(), title.to_string());
        fm.parent = parent.map(str::to_string);
        let name = format!("{}_{id}.md", title.to_lowercase().replace(' ', "-"));
        note::write_atomic(
            &vault.root().join(name),
            &frontmatter::join(&fm, "the body").unwrap(),
        )
        .unwrap();
    }

    const A: &str = "01HQ3M8K2P00000000000000A1";
    const B: &str = "01HQ3M8K2P00000000000000B1";
    const C: &str = "01HQ3M8K2P00000000000000C1";

    #[test]
    fn a_note_whose_frontmatter_will_not_parse_is_reported_not_moved() {
        let vault = TempVault::new();
        legacy(&vault, A, "Research", None);
        // An unquoted colon in a title: valid to type, not valid YAML.
        fs::write(
            vault.root().join("hand-edited.md"),
            "---\nid: x\ntitle: Cp: 300 K\n---\n\nbody\n",
        )
        .unwrap();

        let plan = vault.migration_plan().unwrap();
        assert_eq!(plan.skipped, vec!["hand-edited.md"]);

        vault.migrate().unwrap();
        assert!(
            vault.root().join("hand-edited.md").is_file(),
            "it must be left exactly where it was"
        );
    }

    #[test]
    fn an_old_vaults_attachment_references_still_resolve() {
        // Bodies written before attachments moved beside their notes point at
        // a top-level `attachments/`, and those pictures have to keep working.
        let vault = TempVault::new();
        let legacy_dir = vault.root().join("attachments");
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(legacy_dir.join("fig.png"), b"old bytes").unwrap();

        assert_eq!(
            vault.read_attachment("attachments/fig.png").unwrap(),
            b"old bytes"
        );
        // But it is still only that one directory, not a way in anywhere else.
        assert!(vault.read_attachment("attachments/sub/fig.png").is_err());
    }

    #[test]
    fn a_flat_vault_is_recognised_and_a_folder_one_is_not() {
        let vault = TempVault::new();
        assert!(!vault.needs_migration().unwrap(), "empty vault");

        vault.create_note("Modern", folder("Research")).unwrap();
        assert!(!vault.needs_migration().unwrap(), "no note claims a parent");

        legacy(&vault, A, "Research", None);
        legacy(&vault, B, "Sb2Se3", Some(A));
        assert!(vault.needs_migration().unwrap());
    }

    #[test]
    fn the_plan_turns_claimed_parents_into_folders() {
        let vault = TempVault::new();
        legacy(&vault, A, "Research", None);
        legacy(&vault, B, "Sb2Se3", Some(A));
        legacy(&vault, C, "Cp", Some(B));

        let plan = vault.migration_plan().unwrap();
        let targets: Vec<&str> = plan.moves.iter().map(|(_, to)| to.as_str()).collect();

        assert!(targets.contains(&"Research.md"));
        assert!(targets.contains(&"Research/Sb2Se3.md"));
        assert!(targets.contains(&"Research/Sb2Se3/Cp.md"));
        assert!(plan.flattened.is_empty());
    }

    #[test]
    fn migrating_moves_the_files_and_clears_the_claim() {
        let vault = TempVault::new();
        legacy(&vault, A, "Research", None);
        legacy(&vault, B, "Sb2Se3", Some(A));
        legacy(&vault, C, "Cp", Some(B));

        let moved = vault.migrate().unwrap();
        assert_eq!(moved, 3);

        assert!(vault.root().join("Research.md").is_file());
        assert!(vault.root().join("Research/Sb2Se3.md").is_file());
        assert!(vault.root().join("Research/Sb2Se3/Cp.md").is_file());

        // A parent note keeps being a note, and gains a folder beside it.
        assert!(vault.root().join("Research").is_dir());

        // Ids survive, so nothing that linked to these notes broke.
        let deepest = vault.read_note(C).unwrap();
        assert_eq!(deepest.summary.id, C);
        assert_eq!(deepest.summary.folder, "Research/Sb2Se3");
        assert_eq!(deepest.body, "the body\n");

        // And the claim is gone, so nothing can disagree with the path later.
        assert!(!vault.needs_migration().unwrap());
    }

    #[test]
    fn migrating_keeps_a_copy_of_every_note_first() {
        let vault = TempVault::new();
        legacy(&vault, A, "Research", None);
        legacy(&vault, B, "Sb2Se3", Some(A));

        vault.migrate().unwrap();

        let backups = vault.root().join(SUTRA).join("backups");
        let run = fs::read_dir(&backups).unwrap().next().unwrap().unwrap();
        let kept: Vec<_> = fs::read_dir(run.path()).unwrap().flatten().collect();
        assert_eq!(kept.len(), 2, "both notes should have been copied");
    }

    #[test]
    fn a_chain_deeper_than_the_cap_is_placed_at_the_cap() {
        let vault = TempVault::new();
        let ids: Vec<String> = (0..7)
            .map(|i| format!("01HQ3M8K2P0000000000000{i:03}"))
            .collect();
        for (i, id) in ids.iter().enumerate() {
            legacy(
                &vault,
                id,
                &format!("L{i}"),
                if i == 0 { None } else { Some(&ids[i - 1]) },
            );
        }

        let plan = vault.migration_plan().unwrap();
        assert!(!plan.flattened.is_empty(), "it must say what it cut short");
        for (_, to) in &plan.moves {
            // Folders, not path segments: four folders is at the cap.
            assert!(
                to.matches('/').count() <= MAX_DEPTH,
                "{to} is deeper than the cap"
            );
        }
        vault.migrate().unwrap();
        assert_eq!(vault.list_notes().unwrap().len(), 7, "nothing lost");
    }

    #[test]
    fn a_parent_that_does_not_exist_lands_the_note_at_the_top() {
        let vault = TempVault::new();
        legacy(&vault, B, "Orphan", Some("01HQNOSUCHPARENT0000000000"));
        vault.migrate().unwrap();
        assert_eq!(vault.read_note(B).unwrap().summary.folder, "");
    }

    #[test]
    fn a_parent_cycle_does_not_hang_the_migration() {
        let vault = TempVault::new();
        legacy(&vault, A, "One", Some(B));
        legacy(&vault, B, "Two", Some(A));

        let plan = vault.migration_plan().unwrap();
        assert_eq!(plan.moves.len(), 2, "both must still be placed somewhere");
        vault.migrate().unwrap();
        assert_eq!(vault.list_notes().unwrap().len(), 2);
    }

    #[test]
    fn two_siblings_with_one_title_do_not_overwrite_each_other() {
        let vault = TempVault::new();
        legacy(&vault, A, "Parent", None);
        legacy(&vault, B, "Cp", Some(A));
        legacy(&vault, C, "Cp", Some(A));

        vault.migrate().unwrap();

        assert!(vault.root().join("Parent/Cp.md").is_file());
        assert!(vault.root().join("Parent/Cp 2.md").is_file());
        assert!(vault.read_note(B).is_ok());
        assert!(vault.read_note(C).is_ok());
    }
    // ---- tags -----------------------------------------------------------------

    fn tag(vault: &Vault, id: &str, tags: &[&str]) {
        vault
            .set_meta(id, None, None, tags.iter().map(|t| t.to_string()).collect())
            .unwrap();
    }

    #[test]
    fn tags_are_normalised_into_a_hierarchy() {
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        tag(
            &vault,
            &note.summary.id,
            &["#Research / Materials / Sb2Se3", "thermal conductivity"],
        );
        assert_eq!(
            vault.read_note(&note.summary.id).unwrap().summary.tags,
            vec!["research/materials/sb2se3", "thermal-conductivity"]
        );
    }

    #[test]
    fn the_vault_can_count_its_tags() {
        let vault = TempVault::new();
        for (i, tags) in [vec!["cvt", "sb2se3"], vec!["cvt"], vec!["xrd"]]
            .into_iter()
            .enumerate()
        {
            let n = vault.create_note(&format!("N{i}"), None).unwrap();
            tag(&vault, &n.summary.id, &tags);
        }
        let counts = vault.list_tags().unwrap();
        assert_eq!(counts.get("cvt"), Some(&2));
        assert_eq!(counts.get("sb2se3"), Some(&1));
        assert_eq!(counts.len(), 3);
    }

    /// The proof from the plan: rename a tag used by 200 notes, and check that
    /// every file was rewritten, nothing was lost, and it can be put back.
    #[test]
    fn renaming_a_tag_across_two_hundred_notes_is_complete_and_undoable() {
        let vault = TempVault::new();

        for i in 0..200 {
            let n = vault
                .create_note(&format!("Note {i:03}"), folder("Research"))
                .unwrap();
            // Every note carries the tag; half also carry one that must not move.
            if i % 2 == 0 {
                tag(&vault, &n.summary.id, &["thermodynamics", "cvt"]);
            } else {
                tag(&vault, &n.summary.id, &["thermodynamics"]);
            }
        }
        // One note that must be left completely alone.
        let bystander = vault.create_note("Untagged", None).unwrap();
        tag(&vault, &bystander.summary.id, &["xrd"]);

        let result = vault
            .retag("thermodynamics", "research/thermodynamics")
            .unwrap();
        assert_eq!(
            result.changed.len(),
            200,
            "every tagged note must be rewritten"
        );

        let after = vault.list_notes().unwrap();
        assert_eq!(after.len(), 201, "no note may be lost");
        assert_eq!(
            after
                .iter()
                .filter(|n| n.tags.contains(&"research/thermodynamics".into()))
                .count(),
            200
        );
        assert!(
            !after
                .iter()
                .any(|n| n.tags.contains(&"thermodynamics".into())),
            "the old tag must be gone everywhere"
        );
        // The unrelated tags survived, on exactly the notes that had them.
        assert_eq!(
            after
                .iter()
                .filter(|n| n.tags.contains(&"cvt".into()))
                .count(),
            100
        );
        assert_eq!(
            after
                .iter()
                .filter(|n| n.tags.contains(&"xrd".into()))
                .count(),
            1
        );

        let restored = vault.undo_retag(&result.changed).unwrap();
        assert_eq!(restored, 200);
        let back = vault.list_notes().unwrap();
        assert_eq!(
            back.iter()
                .filter(|n| n.tags.contains(&"thermodynamics".into()))
                .count(),
            200
        );
        assert!(
            !back
                .iter()
                .any(|n| n.tags.contains(&"research/thermodynamics".into()))
        );
        assert_eq!(
            back.iter()
                .filter(|n| n.tags.contains(&"cvt".into()))
                .count(),
            100
        );
    }

    #[test]
    fn renaming_a_tag_brings_its_children_with_it() {
        let vault = TempVault::new();
        let a = vault.create_note("A", None).unwrap();
        let b = vault.create_note("B", None).unwrap();
        tag(&vault, &a.summary.id, &["research/materials"]);
        tag(&vault, &b.summary.id, &["research/materials/sb2se3"]);

        vault.retag("research/materials", "materials").unwrap();

        assert_eq!(
            vault.read_note(&a.summary.id).unwrap().summary.tags,
            vec!["materials"]
        );
        assert_eq!(
            vault.read_note(&b.summary.id).unwrap().summary.tags,
            vec!["materials/sb2se3"],
            "a half-moved tag tree is worse than one that did not move"
        );
    }

    #[test]
    fn renaming_onto_an_existing_tag_merges_without_duplicating() {
        let vault = TempVault::new();
        let note = vault.create_note("Both", None).unwrap();
        tag(
            &vault,
            &note.summary.id,
            &["thermalconductivity", "thermal-conductivity", "cvt"],
        );

        let result = vault
            .retag("thermalconductivity", "thermal-conductivity")
            .unwrap();
        assert_eq!(result.changed.len(), 1);

        let after = vault.read_note(&note.summary.id).unwrap().summary.tags;
        assert_eq!(
            after,
            vec!["thermal-conductivity", "cvt"],
            "no duplicate survives"
        );

        // A merge cannot be undone by renaming back, which is why the previous
        // tags are recorded rather than the operation inverted.
        vault.undo_retag(&result.changed).unwrap();
        assert_eq!(
            vault.read_note(&note.summary.id).unwrap().summary.tags,
            vec!["thermalconductivity", "thermal-conductivity", "cvt"]
        );
    }

    #[test]
    fn a_tag_that_matches_nothing_changes_nothing() {
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        tag(&vault, &note.summary.id, &["cvt"]);
        let result = vault.retag("nosuchtag", "other").unwrap();
        assert!(result.changed.is_empty());
        assert_eq!(
            vault.read_note(&note.summary.id).unwrap().summary.tags,
            vec!["cvt"]
        );
    }

    #[test]
    fn a_prefix_that_is_not_a_tag_boundary_is_left_alone() {
        // `thermo` must not match `thermodynamics`. Only the tag itself and
        // things actually beneath it in the tree move.
        let vault = TempVault::new();
        let note = vault.create_note("T", None).unwrap();
        tag(
            &vault,
            &note.summary.id,
            &["thermodynamics", "thermo/notes"],
        );

        vault.retag("thermo", "heat").unwrap();

        let after = vault.read_note(&note.summary.id).unwrap().summary.tags;
        assert_eq!(after, vec!["thermodynamics", "heat/notes"]);
    }

    #[test]
    fn retagging_to_nothing_is_refused() {
        let vault = TempVault::new();
        assert!(vault.retag("cvt", "  ").is_err());
        assert!(vault.retag("###", "cvt").is_err());
    }

    #[test]
    fn similar_tags_are_found_across_the_vault() {
        let vault = TempVault::new();
        for (i, t) in [
            "thermal-conductivity",
            "thermal-conductivity",
            "thermalconductivity",
        ]
        .iter()
        .enumerate()
        {
            let n = vault.create_note(&format!("N{i}"), None).unwrap();
            tag(&vault, &n.summary.id, &[t]);
        }
        let found = vault.similar_tags().unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].from, "thermalconductivity");
        assert_eq!(found[0].from_count, 1);
        assert_eq!(found[0].into_count, 2);
    }
    // ---- sources ---------------------------------------------------------------

    fn paper(doi: &str) -> SourceMeta {
        SourceMeta {
            authors: Some("Zhou, Y.; Wang, L.".into()),
            year: Some("2019".into()),
            container: Some("Nature Energy".into()),
            doi: Some(doi.into()),
            url: None,
            zotero: Some("ABCD1234".into()),
            ..Default::default()
        }
    }

    fn cite(id: &str, page: &str) -> Citation {
        Citation {
            id: id.to_string(),
            page: Some(page.to_string()),
            quote: Some(format!("what it says on {page}")),
            captured: Some(frontmatter::now()),
            ..Default::default()
        }
    }

    #[test]
    fn a_source_is_a_note_in_the_library() {
        let vault = TempVault::new();
        let doc = vault
            .create_source("Quasi-1D Sb2Se3 ribbons", paper("10.1000/xyz"))
            .unwrap();

        assert_eq!(doc.summary.note_type, NoteType::Source);
        assert_eq!(doc.summary.folder, LIBRARY);
        assert!(
            vault
                .root()
                .join("Library/Quasi-1D Sb2Se3 ribbons.md")
                .is_file()
        );
        assert_eq!(
            doc.summary.source.as_ref().unwrap().doi.as_deref(),
            Some("10.1000/xyz")
        );
        // And it is an ordinary note: it can be moved, tagged and written in.
        vault.move_note(&doc.summary.id, "Research").unwrap();
        assert_eq!(
            vault.read_note(&doc.summary.id).unwrap().summary.note_type,
            NoteType::Source
        );
    }

    #[test]
    fn source_details_survive_a_round_trip_through_the_file() {
        let vault = TempVault::new();
        let doc = vault
            .create_source("Zhou 2019", paper("10.1000/xyz"))
            .unwrap();
        // Read back from disk, not from the value we just built.
        let read = vault.read_note(&doc.summary.id).unwrap();
        let meta = read.summary.source.unwrap();
        assert_eq!(meta.authors.as_deref(), Some("Zhou, Y.; Wang, L."));
        assert_eq!(meta.container.as_deref(), Some("Nature Energy"));
        assert_eq!(meta.zotero.as_deref(), Some("ABCD1234"));
    }

    #[test]
    fn a_citation_records_page_and_quote_in_the_note_itself() {
        let vault = TempVault::new();
        let source = vault
            .create_source("Zhou 2019", paper("10.1000/xyz"))
            .unwrap();
        let note = vault.create_note("Sb2Se3 Cp", folder("Research")).unwrap();

        vault
            .set_citations(&note.summary.id, vec![cite(&source.summary.id, "6")])
            .unwrap();

        // On disk, in the note's own frontmatter — which is what makes it
        // readable in ten years with none of this software installed.
        let raw = fs::read_to_string(vault.path_for(&note.summary.id).unwrap()).unwrap();
        assert!(raw.contains("sources:"), "{raw}");
        assert!(
            raw.contains("page: '6'") || raw.contains("page: \"6\""),
            "{raw}"
        );

        let read = vault.read_note(&note.summary.id).unwrap();
        assert_eq!(read.summary.sources.len(), 1);
        assert_eq!(read.summary.sources[0].id, source.summary.id);
        assert_eq!(read.summary.sources[0].page.as_deref(), Some("6"));
    }

    #[test]
    fn importing_the_same_zotero_item_twice_updates_one_note() {
        let vault = TempVault::new();
        let first = vault
            .import_source("Zhou 2019", paper("10.1000/old"))
            .unwrap();
        let second = vault
            .import_source("Zhou 2019 — corrected", paper("10.1000/new"))
            .unwrap();

        assert_eq!(first.id, second.id, "the same paper must be the same note");
        assert_eq!(vault.list_sources().unwrap().len(), 1);
        assert_eq!(
            vault
                .read_note(&first.id)
                .unwrap()
                .summary
                .source
                .unwrap()
                .doi
                .as_deref(),
            Some("10.1000/new")
        );
    }

    #[test]
    fn a_hand_written_source_is_never_matched_by_an_import() {
        let vault = TempVault::new();
        let by_hand = SourceMeta {
            authors: Some("Someone".into()),
            ..SourceMeta::default()
        };
        vault.create_source("A paper", by_hand.clone()).unwrap();
        vault.import_source("A paper", by_hand).unwrap();
        // Two sources with no Zotero key are two sources. Guessing they are one
        // would silently merge someone's notes.
        assert_eq!(vault.list_sources().unwrap().len(), 2);
    }
    // ---- legacy citations --------------------------------------------------

    #[test]
    fn legacy_citations_are_found_across_the_vault() {
        let vault = TempVault::new();
        let a = vault.create_note("A", folder("Research")).unwrap();
        let b = vault.create_note("B", None).unwrap();
        vault
            .save_note(&a.summary.id, "A", "As [@ABCD1234] shows, and [@ZZZZ9999].")
            .unwrap();
        vault
            .save_note(&b.summary.id, "B", "Also [@ABCD1234].")
            .unwrap();

        let counts = vault.legacy_citations().unwrap();
        assert_eq!(counts.get("ABCD1234"), Some(&2));
        assert_eq!(counts.get("ZZZZ9999"), Some(&1));
        assert_eq!(counts.len(), 2);
    }

    #[test]
    fn migrating_points_citations_at_source_notes_and_leaves_the_rest() {
        let vault = TempVault::new();
        let source = vault
            .create_source("Zhou 2019", paper("10.1000/xyz"))
            .unwrap();
        let note = vault.create_note("Citing", folder("Research")).unwrap();
        vault
            .save_note(
                &note.summary.id,
                "Citing",
                "As [@ABCD1234] shows, unlike [@ZZZZ9999].",
            )
            .unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("ABCD1234".to_string(), source.summary.id.clone());
        let changed = vault.migrate_citations(&mapping).unwrap();
        assert_eq!(changed, 1);

        let body = vault.read_note(&note.summary.id).unwrap().body;
        assert!(body.contains(&format!("[@{}]", source.summary.id)));
        assert!(
            body.contains("[@ZZZZ9999]"),
            "a key Zotero could not answer for is left alone, not deleted: {body}"
        );
    }

    #[test]
    fn migrating_does_not_stamp_updated_across_the_vault() {
        // Rewriting a reference into the form that means the same thing is not
        // an edit. Stamping every note would destroy the one signal saying
        // what you were actually working on.
        let vault = TempVault::new();
        let source = vault.create_source("S", paper("10.1000/x")).unwrap();
        let note = vault.create_note("Citing", None).unwrap();
        vault
            .save_note(&note.summary.id, "Citing", "See [@ABCD1234].")
            .unwrap();

        let before = frontmatter::split(
            &fs::read_to_string(vault.path_for(&note.summary.id).unwrap()).unwrap(),
        )
        .unwrap()
        .0
        .unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("ABCD1234".to_string(), source.summary.id.clone());
        vault.migrate_citations(&mapping).unwrap();

        let after = frontmatter::split(
            &fs::read_to_string(vault.path_for(&note.summary.id).unwrap()).unwrap(),
        )
        .unwrap()
        .0
        .unwrap();
        assert_eq!(before.updated, after.updated);
        assert_eq!(before.created, after.created);
    }

    #[test]
    fn migrating_twice_is_harmless() {
        let vault = TempVault::new();
        let source = vault.create_source("S", paper("10.1000/x")).unwrap();
        let note = vault.create_note("Citing", None).unwrap();
        vault
            .save_note(&note.summary.id, "Citing", "See [@ABCD1234].")
            .unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("ABCD1234".to_string(), source.summary.id.clone());
        assert_eq!(vault.migrate_citations(&mapping).unwrap(), 1);
        // Nothing left to do, so nothing is written.
        assert_eq!(vault.migrate_citations(&mapping).unwrap(), 0);
        assert!(vault.legacy_citations().unwrap().is_empty());
    }
    // ---- views ---------------------------------------------------------------

    #[test]
    fn a_view_is_an_ordinary_note_with_a_query_in_its_frontmatter() {
        // The whole design in one assertion: a view is a markdown file. Open
        // it in any editor, read what it looks for, delete Sutra, and the
        // query is still there in plain text.
        let vault = TempVault::new();
        let query: views::Query =
            serde_yaml_ng::from_str("all:\n- under: Research\n- tag: method/xrd\nsort: title\n")
                .unwrap();
        let doc = vault.create_view("Everything XRD", query.clone()).unwrap();

        let path = vault.root().join(VIEWS).join("Everything XRD.md");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("type: view"), "{text}");
        assert!(text.contains("under: Research"), "{text}");
        assert!(text.contains("tag: method/xrd"), "{text}");
        // Written the way a person would write it, not with YAML tags.
        assert!(!text.contains('!'), "{text}");

        assert_eq!(vault.view_query(&doc.summary.id).unwrap(), Some(query));
    }

    #[test]
    fn a_view_note_can_be_written_in_moved_and_tagged_like_any_other() {
        // "A view is a note" is only true if every ordinary thing works on it.
        let vault = TempVault::new();
        let doc = vault
            .create_view(
                "Unread papers",
                serde_yaml_ng::from_str("all: [{tag: unread}]").unwrap(),
            )
            .unwrap();
        let id = doc.summary.id;

        vault
            .save_note(
                &id,
                "Unread papers",
                "Why: chapter 3 needs these read first.",
            )
            .unwrap();
        vault
            .set_meta(&id, Some("📥".into()), None, vec!["chapter/3".into()])
            .unwrap();
        vault.move_note(&id, "Research").unwrap();

        let read = vault.read_note(&id).unwrap();
        assert_eq!(read.summary.folder, "Research");
        assert_eq!(read.summary.tags, ["chapter/3"]);
        assert_eq!(read.body.trim(), "Why: chapter 3 needs these read first.");
        // And the query survived all of it.
        assert!(vault.view_query(&id).unwrap().is_some());
    }

    #[test]
    fn a_view_whose_query_was_deleted_by_hand_is_still_a_readable_note() {
        // Someone will delete the `view:` block. That must leave a note that
        // opens and says what it was for, not a file the app refuses to read.
        let vault = TempVault::new();
        let doc = vault
            .create_view(
                "Broken",
                serde_yaml_ng::from_str("all: [{tag: x}]").unwrap(),
            )
            .unwrap();
        let id = doc.summary.id;
        vault
            .save_note(&id, "Broken", "The prose survives.")
            .unwrap();

        let path = vault.root().join(VIEWS).join("Broken.md");
        let text = fs::read_to_string(&path).unwrap();
        let stripped: String = text
            .lines()
            .filter(|l| !l.starts_with("view:") && !l.starts_with("  ") && !l.starts_with("- "))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, stripped).unwrap();

        assert_eq!(vault.view_query(&id).unwrap(), None);
        assert_eq!(
            vault.read_note(&id).unwrap().body.trim(),
            "The prose survives."
        );
    }

    #[test]
    fn saving_a_query_onto_an_existing_note_makes_it_a_view() {
        let vault = TempVault::new();
        let doc = vault.create_note("Was a note", None).unwrap();
        let summary = vault
            .set_view_query(
                &doc.summary.id,
                serde_yaml_ng::from_str("all: [{type: question}]").unwrap(),
            )
            .unwrap();
        assert_eq!(summary.note_type, NoteType::View);
        assert_eq!(vault.list_views().unwrap().len(), 1);
    }

    #[test]
    fn a_view_written_by_a_newer_sutra_survives_a_round_trip_through_this_one() {
        // The forward-compatibility rule. An unknown term is ignored when the
        // view runs, but it is still in the file after this build saves it —
        // opening a vault on an older machine must not quietly edit the query.
        let vault = TempVault::new();
        let path = vault.root().join("From the future.md");
        fs::write(
            &path,
            "---\nid: 01HQ3M8K2PVIEWFROMTHEFUTURE\ntype: view\ntitle: From the future\n\
             position: 0\ncreated: 2026-08-31T00:00:00Z\nupdated: 2026-08-31T00:00:00Z\n\
             view:\n  all:\n  - tag: xrd\n  - written-by: alice\n---\n\nBody.\n",
        )
        .unwrap();

        let id = "01HQ3M8K2PVIEWFROMTHEFUTURE";
        let query = vault.view_query(id).unwrap().unwrap();
        assert_eq!(query.unreadable().len(), 1);
        assert_eq!(query.compile().ignored, 1);

        vault.set_view_query(id, query).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("written-by"),
            "the unknown term was dropped:\n{text}"
        );
        assert!(text.contains("alice"), "{text}");
        assert!(text.contains("tag: xrd"), "{text}");
    }

    /// How the vault behaves at the size a PhD reaches, rather than the size a
    /// unit test reaches.
    ///
    /// Ignored because it writes thousands of files and takes seconds; run it
    /// deliberately:
    ///
    ///     cargo test --bins -- --ignored --nocapture scales_to_a_real_vault
    ///
    /// It asserts a ceiling rather than printing a number, because a benchmark
    /// nobody fails is a benchmark nobody reads. The ceilings are deliberately
    /// loose — they are there to catch an accidental O(n²), not to police
    /// milliseconds on someone else's laptop.
    #[test]
    #[ignore = "writes thousands of files"]
    fn scales_to_a_real_vault() {
        use std::time::Instant;

        let vault = TempVault::new();
        const NOTES: usize = 5_000;

        // Spread across folders, the way a vault actually grows, so the
        // directory walk is not measured against one enormous flat folder.
        let started = Instant::now();
        for i in 0..NOTES {
            let folder = format!("Strand {}/Sub {}", i % 7, (i / 7) % 5);
            vault.create_folder(&folder).unwrap();
            let doc = vault
                .create_note(&format!("Note {i} about Sb2Se3 growth"), Some(folder))
                .unwrap();
            vault
                .save_note(
                    &doc.summary.id,
                    &doc.summary.title,
                    &format!(
                        "Antimony selenide ribbons grow along the c axis in run {i}. \
                         The seed layer decides the texture, and iodine transports \
                         the material as $\\ce{{SbI3}}$ in the vapour."
                    ),
                )
                .unwrap();
        }
        eprintln!("built {NOTES} notes in {:?}", started.elapsed());

        let started = Instant::now();
        let notes = vault.list_notes().unwrap();
        let listing = started.elapsed();
        eprintln!("list_notes: {listing:?} for {} notes", notes.len());
        assert_eq!(notes.len(), NOTES);

        // Listing reads every file, so it is linear by construction. The
        // ceiling is what keeps it linear: at 5,000 notes this is the single
        // call standing between launching the app and seeing anything.
        assert!(
            listing.as_millis() < 4_000,
            "listing {NOTES} notes took {listing:?}, which a person waits through"
        );

        let started = Instant::now();
        let folders = vault.list_folders().unwrap();
        eprintln!(
            "list_folders: {:?} for {} folders",
            started.elapsed(),
            folders.len()
        );

        let started = Instant::now();
        let tags = vault.list_tags().unwrap();
        eprintln!("list_tags: {:?} for {} tags", started.elapsed(), tags.len());

        // Reading one note must not depend on how many others there are. If
        // this ever tracks NOTES, something has started scanning the vault to
        // find a file it already knows the path of.
        let started = Instant::now();
        vault.read_note(&notes[NOTES / 2].id).unwrap();
        let one = started.elapsed();
        eprintln!("read_note: {one:?}");
        assert!(one.as_millis() < 50, "reading one note took {one:?}");
    }

    /// A sync client rewriting the vault underneath a save.
    ///
    /// The manual tells people to put their vault in OneDrive or Dropbox, so
    /// this is not a hypothetical: another process replaces files on its own
    /// schedule while Sutra is writing. The vault is a thesis, so the bar is
    /// that no state is ever *torn* — a note may hold either version, but it
    /// must never hold half of one, and it must never disappear.
    #[test]
    fn a_sync_client_rewriting_files_never_tears_a_note() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let vault = Arc::new(TempVault::new());
        let doc = vault.create_note("Growth", None).unwrap();
        let id = doc.summary.id.clone();
        let path = vault.root().join(vault.relative_for(&id).unwrap());

        let stop = Arc::new(AtomicBool::new(false));

        // The impostor: another process writing a complete, valid version of
        // the same note, the way a sync client lands a remote edit.
        let intruder = {
            let path = path.clone();
            let id = id.clone();
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut n = 0;
                while !stop.load(Ordering::Relaxed) {
                    n += 1;
                    let contents = format!(
                        "---\nid: {id}\ntype: note\ntitle: Growth\ncreated: 2026-08-21T10:14:00Z\nupdated: 2026-08-21T10:14:00Z\n---\n\nfrom the other machine, revision {n}\n"
                    );
                    let _ = note::write_atomic(&path, &contents);
                }
            })
        };

        for i in 0..200 {
            vault
                .save_note(&id, "Growth", &format!("written here, revision {i}"))
                .unwrap();

            // Whatever is on disk at this instant must be a whole note. This is
            // the assertion that matters: a half-written file is unrecoverable
            // work, and the reason writes go through a temp file and a rename.
            let raw = fs::read_to_string(&path).unwrap();
            let (parsed, body) = frontmatter::split(&raw)
                .unwrap_or_else(|e| panic!("torn note on disk: {e}\n---\n{raw}"));
            let fm = parsed.expect("a note with no frontmatter appeared");
            assert_eq!(fm.id, id, "the note's identity changed under it");
            assert!(
                body.contains("revision"),
                "a note was truncated mid-write: {body:?}"
            );
        }

        stop.store(true, Ordering::Relaxed);
        intruder.join().unwrap();

        assert!(path.exists(), "the note was lost entirely");
        assert_eq!(vault.list_notes().unwrap().len(), 1);
    }

    /// The other thing a sync client does: leave a second copy behind.
    ///
    /// Dropbox writes "note (conflicted copy).md" and OneDrive writes
    /// "note-LAPTOP.md", both carrying the same `id` in their frontmatter. The
    /// rule here is that neither copy may be *hidden*: whichever one the app
    /// opens, the other is still a file in the vault with its text intact, and
    /// the listing does not silently drop it.
    #[test]
    fn a_conflicted_copy_hides_neither_version() {
        let vault = TempVault::new();
        let doc = vault.create_note("Growth", None).unwrap();
        let id = doc.summary.id.clone();
        vault
            .save_note(&id, "Growth", "the version written here")
            .unwrap();

        let original = vault.root().join(vault.relative_for(&id).unwrap());
        let conflicted = vault.root().join("Growth (conflicted copy).md");
        let mut raw = fs::read_to_string(&original).unwrap();
        raw = raw.replace("the version written here", "the version from the laptop");
        fs::write(&conflicted, &raw).unwrap();

        let notes = vault.list_notes().unwrap();
        assert_eq!(
            notes.len(),
            2,
            "a conflicted copy must be visible, not swallowed"
        );

        // Opening by id is unambiguous — first file wins — and, crucially,
        // reading does not delete or rewrite the other copy.
        let opened = vault.read_note(&id).unwrap();
        assert!(opened.body.contains("written here"));
        assert!(
            fs::read_to_string(&conflicted)
                .unwrap()
                .contains("from the laptop"),
            "the other copy must still be on disk, untouched"
        );

        // And saving does not clobber it either.
        vault
            .save_note(&id, "Growth", "edited after the conflict")
            .unwrap();
        assert!(
            fs::read_to_string(&conflicted)
                .unwrap()
                .contains("from the laptop"),
            "saving one copy overwrote the other"
        );
    }

    #[test]
    fn headings_are_found_with_the_weight_of_what_follows() {
        let body = "# Growth\n\nTwo words here.\n\n## My question\n\n## Answered\n\nThree words follow this.\n";
        let found = headings_in(body);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0], ("Growth".into(), 3));
        // A question with nothing under it is the thing the overview is for.
        assert_eq!(found[1], ("My question".into(), 0));
        assert_eq!(found[2], ("Answered".into(), 4));
    }

    #[test]
    fn a_hash_inside_a_code_fence_is_not_a_heading() {
        // `# include <stdio.h>` in a listing is not a research question, and
        // counting it as one would put C in the list.
        let body = "## Real\n\n```c\n#include <stdio.h>\n# not a heading\n```\n\nprose\n";
        let found = headings_in(body);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "Real");
    }

    #[test]
    fn an_overview_counts_citations_and_their_provenance() {
        let vault = TempVault::new();

        let source = vault.create_note("Ko 2024", None).unwrap();
        vault
            .set_type(&source.summary.id, NoteType::Source)
            .unwrap();

        let note = vault.create_note("Thermal conductivity", None).unwrap();
        vault
            .save_note(
                &note.summary.id,
                "Thermal conductivity",
                "## My question\n\n## Source says\n\nquoted\n",
            )
            .unwrap();
        vault
            .set_citations(
                &note.summary.id,
                vec![Citation {
                    id: source.summary.id.clone(),
                    page: Some("6".into()),
                    quote: Some("kappa = 0.037".into()),
                    ..Default::default()
                }],
            )
            .unwrap();

        let overview = vault.overview().unwrap();
        assert_eq!(overview.citations.get(&source.summary.id), Some(&1));
        assert_eq!(overview.with_page, 1);
        assert_eq!(overview.with_quote, 1);
        assert_eq!(overview.sources.len(), 1, "the source note is listed");

        let texts: Vec<_> = overview.headings.iter().map(|h| h.text.as_str()).collect();
        assert!(texts.contains(&"My question"));
        assert!(texts.contains(&"Source says"));
    }
}
