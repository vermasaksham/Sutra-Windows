//! The vault: a tree of markdown files, and the operations on it.
//!
//! Notes live in real, nested directories, because a folder tree is the part
//! of the layout a person reads. Identity does not: the ULID lives inside the
//! file, in frontmatter. Keeping those two apart is what lets a note be
//! renamed and moved freely without a single `[[id]]` link anywhere in the
//! vault having to change.

use crate::attachments;
use crate::citations;
use crate::error::{Result, SutraError};
use crate::frontmatter::{self, Citation, Frontmatter, NoteType, ORIGIN_ANNOTATION, SourceMeta};
use crate::note;
use crate::tags;
use crate::views;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    /// Files that claimed an id already taken, from the last scan. Derived
    /// state about the *filesystem*, not about the notes, so it lives here
    /// rather than in the index.
    clashes: RwLock<Vec<IdClash>>,
}

/// The outcome of bringing a note's own attachments with it.
///
/// Two things, because the order they are used in is what makes an interrupted
/// move harmless: the rewritten body has to be on disk before the files it no
/// longer points at are removed.
struct Relocated {
    /// The body with every moved reference retargeted.
    body: String,
    /// Vault-relative paths of the copies left at the old locations, to be
    /// removed once the body above has been written.
    originals: Vec<String>,
}

/// One position in a chapter, resolved against the vault.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterEntry {
    /// The id the chapter's `sequence:` holds at this position.
    pub id: String,
    /// The note that id names, or `None` when the vault no longer has it.
    ///
    /// `None` is a real answer, not an absence of one. The chapter still claims
    /// this note belongs here, and only the author can say whether the right fix
    /// is to remove the entry or to restore the note from the trash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<NoteSummary>,
}

/// A chapter flattened for export: one title and body per section, in order.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterSection {
    pub id: String,
    pub title: String,
    /// The note body, as markdown, exactly as the file holds it.
    pub body: String,
    /// Whether to write the title as a heading above the body. False for the
    /// chapter's own section, whose title is the document's title.
    pub heading: bool,
}

/// A chapter that names a given note, and where in it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterUse {
    pub id: String,
    pub title: String,
    /// Zero-based position in the chapter's sequence.
    pub position: usize,
    /// How many notes the chapter holds, so "3 of 12" can be shown.
    pub of: usize,
}

/// Two files claiming one note id.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdClash {
    pub id: String,
    /// The file `read_note` will open — the canonical one.
    pub opened: String,
    /// The file that is on disk, listed, and unreachable by id.
    pub shadowed: String,
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
            clashes: RwLock::new(Vec::new()),
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

    /// A locator for PDFs attached inside this vault.
    ///
    /// Handed out instead of the root itself, so the rule above holds: the path
    /// stays on this side and callers get something that can only resolve an
    /// attachment the vault already records. `VaultPdf` refuses anything with a
    /// `..` in it, so even a caller passing a path it composed cannot reach out
    /// of the vault.
    pub fn pdf_locator(&self) -> crate::pdfread::VaultPdf {
        crate::pdfread::VaultPdf::new(self.root.clone())
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

        // Two files can claim one id — a sync client's conflicted copy, a note
        // duplicated in Explorer. Which one wins used to depend on the order
        // the filesystem happened to hand them back, which is not the same on
        // Windows as on Linux: the same vault opened the note on one and its
        // conflicted copy on the other.
        files.sort_by(|a, b| canonical_first(a, b));

        let mut notes = Vec::new();
        let mut map = HashMap::with_capacity(files.len());
        let mut clashes = Vec::new();

        for relative in files {
            let Ok(contents) = fs::read_to_string(self.root.join(&relative)) else {
                continue;
            };
            let Ok((parsed, body)) = frontmatter::split(&contents) else {
                continue;
            };
            let fm = parsed.unwrap_or_else(|| Self::synthesise(&relative));
            // Two files claiming one id is possible — a copied note, a bad
            // merge, a sync client's conflicted copy. First one wins and the
            // second is left out of the map rather than silently shadowing it.
            //
            // Recorded rather than only resolved. Both files stay on disk and
            // both stay listed; what was missing was any way for the app to
            // *say so*, which left the second copy visible in the note list
            // but unreachable — clicking it opened the first. Preserve, warn,
            // let the researcher reconcile: guessing which of two versions of
            // their work to discard is not a decision this program gets to
            // make.
            match map.entry(fm.id.clone()) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(relative.clone());
                }
                std::collections::hash_map::Entry::Occupied(taken) => {
                    clashes.push(IdClash {
                        id: fm.id.clone(),
                        opened: taken.get().clone(),
                        shadowed: relative.clone(),
                    });
                }
            }
            notes.push(summary_of(&fm, body, folder_of(&relative)));
        }

        notes.sort_by(|a, b| {
            a.folder
                .cmp(&b.folder)
                .then_with(|| a.position.cmp(&b.position))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });

        *self.paths.write().unwrap_or_else(|e| e.into_inner()) = map;
        *self.clashes.write().unwrap_or_else(|e| e.into_inner()) = clashes;
        Ok(notes)
    }

    /// Files that claim an id another file already claimed.
    ///
    /// Refreshed by every scan, so it describes the vault as last read. Empty
    /// is the normal answer; anything else is worth showing the researcher,
    /// because it means two files on disk disagree about being the same note
    /// and only one of them is reachable by id.
    pub fn id_clashes(&self) -> Vec<IdClash> {
        self.clashes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
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

        // Bring the note's own attachments with it. Without this the file
        // moved and its pictures stayed behind: still resolving, because a
        // reference is resolved against the whole vault, but sitting in a
        // folder the note has left — so deleting that folder in Explorer, or
        // moving the note out of a project being archived, silently broke
        // every figure in it.
        let moved = self.relocate_attachments(id, &folder, body)?;
        let body = match moved {
            Some(Relocated { body, originals }) => {
                // `updated` is deliberately untouched. Moving a note is not an
                // edit to it, and a vault whose timestamps move when someone
                // drags a file has lost the one signal that says when the work
                // happened.
                note::write_atomic(&self.root.join(&target), &frontmatter::join(&fm, &body)?)?;

                // Only now, with the body committed, do the copies at the old
                // paths stop being the ones the note relies on. This ordering is
                // the whole reason attachments are copied rather than renamed:
                // between the copy and this line every reference in the note
                // still resolves, so a crash — or a laptop lid — leaves a
                // duplicate file, which is visible and harmless, rather than a
                // figure that no longer loads.
                //
                // A failure to remove one is not a failure of the move. The note
                // is where it should be and its pictures are beside it; a stray
                // copy in the old folder is untidy, not wrong.
                for original in originals {
                    let _ = fs::remove_file(self.root.join(original));
                }
                body
            }
            None => body.to_string(),
        };

        Ok(summary_of(&fm, &body, folder))
    }

    /// Move this note's own attachments into `folder`, returning the rewritten
    /// body when anything changed.
    ///
    /// "Its own" means referenced by this note and by no other. An attachment
    /// two notes point at belongs to neither, so it stays exactly where it is
    /// and both references keep resolving — moving it would fix one note by
    /// breaking another.
    ///
    /// Establishing that costs a scan of every note in the vault. That is
    /// acceptable here and would not be on a hot path: moving a note is
    /// something a person does by hand, one at a time.
    fn relocate_attachments(
        &self,
        id: &str,
        folder: &str,
        body: &str,
    ) -> Result<Option<Relocated>> {
        let owned = self.owned_attachments(id, body)?;
        if owned.is_empty() {
            return Ok(None);
        }

        let destination = join_relative(folder, ATTACHMENTS);
        let directory = self.root.join(&destination);
        let mut rewritten = body.to_string();
        let mut originals = Vec::new();

        for reference in owned {
            let Some(name) = Path::new(&reference)
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if folder_of(&reference) == destination {
                continue;
            }
            let from = self.root.join(&reference);
            if !from.is_file() {
                // The reference is already dangling. Leave it saying what it
                // says — inventing a new target would hide the fact that the
                // picture is gone.
                continue;
            }

            fs::create_dir_all(&directory)?;
            hide_from_explorer(&directory);

            // Names are ULID-prefixed, so a clash means the same file is
            // already there. Take a fresh name rather than overwrite it.
            let mut to_name = name.clone();
            if directory.join(&to_name).exists() {
                to_name = format!("{}_{}", Ulid::generate(), name);
            }
            let to_reference = join_relative(&destination, &to_name);
            // Copied, not renamed. See the caller for why the original cannot be
            // removed until the rewritten body has been written.
            note::copy_with_retry(&from, &self.root.join(&to_reference))?;

            rewritten = attachments::retarget(&rewritten, &reference, &to_reference);
            originals.push(reference);
        }

        if originals.is_empty() {
            return Ok(None);
        }
        Ok(Some(Relocated {
            body: rewritten,
            originals,
        }))
    }

    /// The attachments `body` references that no *other* note also references.
    ///
    /// The question this answers is "may Sutra move or trash this file?", and
    /// the only safe answer is yes when exactly one note points at it. A file
    /// nothing points at is not reported either: it is not this note's to
    /// take, and something outside the app may well be using it.
    fn owned_attachments(&self, id: &str, body: &str) -> Result<Vec<String>> {
        let mine = attachments::extract(body);
        if mine.is_empty() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        collect(&self.root, &self.root, 0, &mut files)?;

        let mut shared: HashSet<String> = HashSet::new();
        for relative in files {
            let Ok(contents) = fs::read_to_string(self.root.join(&relative)) else {
                continue;
            };
            let Ok((parsed, other)) = frontmatter::split(&contents) else {
                continue;
            };
            let other_id = match parsed {
                Some(fm) => fm.id,
                None => note::adopted_id(&relative),
            };
            if other_id == id {
                continue;
            }
            shared.extend(attachments::extract(other));
        }

        Ok(mine
            .into_iter()
            .filter(|reference| !shared.contains(reference))
            .collect())
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

        // Work out what the note owns *before* moving it: the answer is read
        // from its body, and the scan that decides "owned" has to be able to
        // find every other note while this one is still where it says it is.
        let owned = match fs::read_to_string(self.root.join(&relative)) {
            Ok(contents) => {
                let (_, body) = frontmatter::split(&contents)?;
                self.owned_attachments(id, body)?
            }
            // Unreadable is not a reason to refuse the delete. The note is
            // still moved to the trash; its attachments are simply left alone,
            // which is the safe direction.
            Err(_) => Vec::new(),
        };

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

        // The note's own pictures go to the trash with it. They are moved, not
        // unlinked: everything in `.sutra/trash` can be dragged back out in
        // Explorer, so a delete stays as recoverable for a figure as it is for
        // the note that showed it. A file any other note still references is
        // not touched — see `owned_attachments`.
        for reference in owned {
            let from = self.root.join(&reference);
            if !from.is_file() {
                continue;
            }
            let mut into = trash.join(reference.replace('/', " - "));
            if into.exists() {
                into = trash.join(format!(
                    "{}.{}",
                    Ulid::generate(),
                    reference.replace('/', " - ")
                ));
            }
            // Best effort, and deliberately so: the note is already in the
            // trash, and failing the whole delete because a picture was locked
            // would leave the user with a half-deleted note and an error.
            let _ = note::rename_with_retry(&from, &into);
        }
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

            // This migration turns claimed parents into folders. A note that
            // claims no parent has nothing to turn into anything, so it keeps
            // the folder it is in — the filename may still be tidied, but the
            // location is not the migration's to change.
            //
            // It used to derive every note's folder from its chain of claims,
            // including the notes that had no claim, whose chain is empty and
            // whose folder therefore came out as the vault root. So a single
            // note still claiming a parent was enough to make the plan propose
            // flattening every organised note in the vault into the root — and
            // that is exactly the state a half-finished migration leaves behind.
            let folder = if note.parent.is_none() {
                folder_of(&note.relative)
            } else {
                ancestors.join("/")
            };
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

    /// Copy every markdown file into a folder of its own under `.sutra/backups/`.
    ///
    /// The shared first step of every migration, and the reason `migrate` and
    /// `migrate_citations` can be described by one contract: detect, plan,
    /// preview, **back up**, apply, verify. A migration rewrites files the user
    /// did not ask to have rewritten, in bulk, and the only honest answer to
    /// "what if it gets it wrong" is a copy of what was there before.
    ///
    /// Only the markdown. Attachments are never touched by a migration, and
    /// copying a vault's worth of PDFs to rename some text files would turn a
    /// two-second operation into a ten-minute one.
    pub fn back_up(&self) -> Result<PathBuf> {
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
                eid: Ulid::generate().to_string(),
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
    ///
    /// Every entry that arrives without an evidence id is given one. Minting
    /// happens here, on the way to disk, rather than in the frontend: the id
    /// is a fact about a record that exists, and a record only exists once it
    /// has been written. Entries that already carry one keep it, so editing a
    /// page number does not re-identify the evidence.
    ///
    /// **An entry that arrives without an id it used to have adopts it back.**
    /// Minting into an empty slot is right for a new record and wrong for an
    /// old one that lost its id in transit, and the two are indistinguishable
    /// by the time they arrive here — both are an entry with no `eid`. So
    /// before minting, this looks for an id that is on disk, is *not* in the
    /// incoming list, and belongs to a record with the same source and the
    /// same words. One such record means the id was dropped rather than the
    /// evidence replaced, and it is restored.
    ///
    /// The match must be unique. Two orphans quoting the same source at the
    /// same words are not something to guess between, so both are left to be
    /// minted fresh: a duplicated record is visible and repairable, and a
    /// wrongly re-used id is neither.
    ///
    /// Today nothing in the app drops an `eid` — every path spreads the entry
    /// it was given. This exists because v0.5 makes the id load-bearing: once
    /// an interpretation says it rests on `E7`, a silently re-minted id is a
    /// broken argument rather than a cosmetic change.
    pub fn set_citations(&self, id: &str, citations: Vec<Citation>) -> Result<NoteSummary> {
        let held = self.read_note(id)?.summary.sources;
        let arriving: HashSet<&str> = citations
            .iter()
            .map(|c| c.eid.trim())
            .filter(|eid| !eid.is_empty())
            .collect();
        // On disk, and no longer claimed by anything arriving.
        let orphans: Vec<&Citation> = held
            .iter()
            .filter(|c| !c.eid.trim().is_empty() && !arriving.contains(c.eid.trim()))
            .collect();

        let citations = citations
            .into_iter()
            .map(|mut citation| {
                if !citation.eid.trim().is_empty() {
                    return citation;
                }
                let mut matches = orphans
                    .iter()
                    .filter(|held| held.id == citation.id && held.quote == citation.quote);
                match (matches.next(), matches.next()) {
                    (Some(only), None) => citation.eid = only.eid.clone(),
                    _ => citation.eid = Ulid::generate().to_string(),
                }
                citation
            })
            .collect();

        self.edit(id, |fm| {
            fm.sources = citations;
        })
    }

    /// Record annotations from a source's PDF as evidence on a note.
    ///
    /// The one write in the whole annotation path. It is additive and it is
    /// idempotent, and both matter:
    ///
    /// **Additive.** Evidence already on the note is untouched. Importing
    /// annotations is not a synchronisation — Zotero does not become the
    /// authority on what a researcher has recorded in their own note, and an
    /// annotation deleted in Zotero does not delete the quotation somebody
    /// built a paragraph on.
    ///
    /// **Idempotent**, by Zotero's annotation key. Importing the same paper
    /// twice adds the marks made since and nothing else, so "import
    /// annotations" is a thing a person can press again without thinking about
    /// it. Evidence captured by hand has no annotation key and is never
    /// matched against, so it is never treated as a duplicate of anything.
    ///
    /// An annotation with neither highlighted text nor a comment carries
    /// nothing to record and is skipped: an entry with a page and no content
    /// is not evidence of anything.
    ///
    /// Returns how many were added, so the caller can say "6 added, 14 already
    /// here" rather than claiming work it did not do.
    pub fn capture_annotations(
        &self,
        id: &str,
        source_id: &str,
        annotations: &[crate::references::Annotation],
    ) -> Result<usize> {
        let existing = self.read_note(id)?.summary.sources;
        let already: std::collections::HashSet<String> = existing
            .iter()
            .filter_map(|c| c.annotation.clone())
            .collect();

        let mut added = Vec::new();
        for annotation in annotations {
            if already.contains(&annotation.key) {
                continue;
            }
            if annotation.text.is_none() && annotation.comment.is_none() {
                continue;
            }
            added.push(Citation {
                // Its own identity, minted here, exactly as a hand-written
                // piece of evidence gets one. An imported quotation is
                // evidence like any other.
                eid: Ulid::generate().to_string(),
                id: source_id.to_string(),
                page: annotation.page.clone(),
                // The source's words go in `quote`, the researcher's in
                // `comment`, and they are never joined. This line is the whole
                // Source-versus-Interpretation invariant, in the one place it
                // could be broken silently.
                quote: annotation.text.clone(),
                comment: annotation.comment.clone(),
                colour: annotation.colour.clone(),
                annotation: Some(annotation.key.clone()),
                // Not derived from the annotation's colour or type. Zotero
                // says nothing about what kind of evidence a highlight is, and
                // guessing would be inventing provenance.
                kind: None,
                captured: Some(frontmatter::now()),
                // Zotero's, and said so. The one origin that is not a person
                // sitting in front of the paper.
                origin: Some(ORIGIN_ANNOTATION.into()),
                // Neither is known here. An annotation carries the page label
                // Zotero printed, not an index into the file, and the item key
                // belongs to the caller that resolved the attachment — it is
                // filled in by `commands`, which has it.
                page_index: None,
                zotero: None,
            });
        }

        if added.is_empty() {
            return Ok(0);
        }
        let count = added.len();
        let mut sources = existing;
        sources.extend(added);
        self.edit(id, |fm| {
            fm.sources = sources;
        })?;
        Ok(count)
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

    // ---- chapters ------------------------------------------------------------

    /// Create a chapter note in a folder.
    ///
    /// An ordinary note with a type and an empty sequence. Its body is not
    /// wasted: a chapter shows its notes rather than its body, but the body is
    /// still where the argument the chapter is making gets written down, and it
    /// still exports ahead of the notes it assembles.
    pub fn create_chapter(&self, title: &str, folder: Option<String>) -> Result<NoteDoc> {
        let doc = self.create_note(title, folder)?;
        let summary = self.edit(&doc.summary.id, |fm| {
            fm.note_type = NoteType::Chapter;
        })?;
        Ok(NoteDoc {
            summary,
            body: String::new(),
            adopted: false,
        })
    }

    /// Replace the notes a chapter assembles. Makes the note a chapter if it
    /// was not one.
    ///
    /// The caller sends the complete order, for the same reason `set_meta` does:
    /// a patch would have to distinguish "move this one" from "remove this one"
    /// over an IPC boundary, and the whole list is a dozen ids.
    ///
    /// Nothing is validated away. An id naming a note that has been deleted is
    /// stored as given and reported by [`Vault::chapter`], because a chapter is
    /// a claim about what belongs in it and Sutra deleting that claim would be
    /// deciding something about somebody's thesis. Duplicates are kept for the
    /// same reason.
    pub fn set_sequence(&self, id: &str, sequence: Vec<String>) -> Result<NoteSummary> {
        self.edit(id, |fm| {
            fm.note_type = NoteType::Chapter;
            fm.sequence = sequence.clone();
        })
    }

    /// What a chapter assembles, in order, resolved against the vault.
    ///
    /// Every position in the sequence comes back, including the ones whose id no
    /// longer names anything. That is the point: a note deleted out from under a
    /// chapter is something the author has to see, and a list that silently
    /// closed the gap would be a list that lies about what the chapter said.
    pub fn chapter(&self, id: &str) -> Result<Vec<ChapterEntry>> {
        let sequence = self.sequence_of(id)?;
        // One listing, not one read per entry: a chapter of forty notes would
        // otherwise be forty directory walks.
        let known: HashMap<String, NoteSummary> = self
            .list_notes()?
            .into_iter()
            .map(|note| (note.id.clone(), note))
            .collect();

        Ok(sequence
            .into_iter()
            .map(|id| ChapterEntry {
                note: known.get(&id).cloned(),
                id,
            })
            .collect())
    }

    /// The ids a chapter names, in order, straight from its frontmatter.
    ///
    /// Empty for a note that is not a chapter, which is not an error: the note is
    /// still a note and still says what it says.
    pub fn sequence_of(&self, id: &str) -> Result<Vec<String>> {
        let relative = self.relative_for(id)?;
        let contents = fs::read_to_string(self.root.join(&relative))?;
        let (parsed, _) = frontmatter::split(&contents)?;
        Ok(parsed.map(|fm| fm.sequence).unwrap_or_default())
    }

    /// A chapter as an ordered list of titles and bodies, ready to export.
    ///
    /// The chapter's own title and body come first, then each note it names. The
    /// shape is what `buildDocument` already takes — an ordered list of note
    /// bodies as markdown — which is why the export path was built that way in
    /// v0.3 before anything could assemble one.
    ///
    /// A position whose note is gone is skipped here rather than reported: an
    /// exported document cannot contain a hole, and `chapter` is the call that
    /// exists to say what is missing before anybody exports it.
    pub fn chapter_sections(&self, id: &str) -> Result<Vec<ChapterSection>> {
        let own = self.read_note(id)?;
        let mut sections = vec![ChapterSection {
            id: own.summary.id,
            title: own.summary.title,
            body: own.body,
            heading: false,
        }];

        for entry in self.chapter(id)? {
            let Some(note) = entry.note else { continue };
            let Ok(doc) = self.read_note(&note.id) else {
                continue;
            };
            sections.push(ChapterSection {
                id: doc.summary.id,
                title: doc.summary.title,
                body: doc.body,
                heading: true,
            });
        }
        Ok(sections)
    }

    /// Every chapter note in the vault, wherever it sits.
    pub fn list_chapters(&self) -> Result<Vec<NoteSummary>> {
        Ok(self
            .list_notes()?
            .into_iter()
            .filter(|n| n.note_type == NoteType::Chapter)
            .collect())
    }

    /// The chapters that name this note, and where in each.
    ///
    /// Answered by reading the chapters rather than the index, because it is
    /// asked when a note is open — one note at a time, by a person — and the
    /// number of chapters in a thesis is a dozen.
    pub fn chapters_using(&self, id: &str) -> Result<Vec<ChapterUse>> {
        let mut out = Vec::new();
        for chapter in self.list_chapters()? {
            let sequence = self.sequence_of(&chapter.id)?;
            if let Some(at) = sequence.iter().position(|held| held == id) {
                out.push(ChapterUse {
                    id: chapter.id,
                    title: chapter.title,
                    position: at,
                    of: sequence.len(),
                });
            }
        }
        Ok(out)
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
        // This rewrites prose in every note in the vault, which makes it a
        // migration in every sense that matters — so it takes the same first
        // step as the layout one. It did not, for two releases, and the
        // difference between the two was invisible from outside.
        self.back_up()?;

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
            Some(found) => self.merge_source_meta(&found.id, meta),
            None => Ok(self.create_source(title, meta)?.summary),
        }
    }

    /// Fold freshly-fetched details onto a source note, keeping what the fetch
    /// had no answer for.
    ///
    /// Separate from [`Vault::set_source_meta`], which replaces. Both are
    /// wanted: editing a source by hand means "this is now the whole truth",
    /// and re-importing means "here is what the library says, leave the rest".
    ///
    /// The merge happens inside `edit`, against the file as it is on disk
    /// right now, rather than against the summary the caller looked up. The
    /// two can differ — a sync client may have rewritten the note since — and
    /// merging onto a stale copy would reintroduce exactly the kind of quiet
    /// overwrite this whole change exists to remove.
    pub fn merge_source_meta(&self, id: &str, meta: SourceMeta) -> Result<NoteSummary> {
        self.edit(id, |fm| {
            fm.note_type = NoteType::Source;
            let merged = match fm.source.as_ref() {
                Some(held) => meta.merged_over(held),
                None => meta,
            };
            fm.source = Some(merged);
        })
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
    ///
    /// A *hit* is checked before it is trusted, which is the part that was
    /// missing. The map is only updated by the operations this app performs,
    /// so a note moved in Explorer or by a sync client leaves a stale entry
    /// pointing at a path that no longer exists. That was not a miss, so no
    /// rescan happened: the read failed, the watcher took the note out of the
    /// index, and it stayed gone until something else rebuilt the map.
    ///
    /// One `exists` call per lookup is a stat on a path already in memory,
    /// against a rescan that reads every note in the vault — so the check goes
    /// on the hit and the rescan stays on the failure.
    fn relative_for(&self, id: &str) -> Result<String> {
        if let Some(found) = self.lookup(id)
            && self.root.join(&found).is_file()
        {
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
    ///
    /// Reads that one directory rather than the whole vault. It used to call
    /// `list_notes`, which parses every note in the vault — so creating one
    /// note in a five-thousand-note vault read five thousand files to work out
    /// a single integer.
    ///
    /// Only the frontmatter's `position` is wanted, but the files still have
    /// to be opened to get it; the saving is in how many. A note whose
    /// frontmatter will not parse is skipped rather than treated as position
    /// zero, which would quietly push a new note above it.
    fn next_position(&self, folder: &str) -> Result<i64> {
        let directory = self.root.join(folder);
        let Ok(entries) = fs::read_dir(&directory) else {
            // The folder does not exist yet, so the note being created is its
            // first.
            return Ok(0);
        };

        let mut highest: Option<i64> = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok((Some(fm), _)) = frontmatter::split(&contents) else {
                continue;
            };
            highest = Some(highest.map_or(fm.position, |h: i64| h.max(fm.position)));
        }
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
    // Where the chain of claims ran out, if that ancestor already lives
    // somewhere. See below for why this is not the same as the vault root.
    let mut anchor = String::new();

    while let Some(id) = current {
        if !seen.insert(id.to_string()) {
            break;
        }
        let Some(ancestor) = by_id.get(id) else { break };
        chain.push(note::file_stem(&ancestor.title));
        if ancestor.parent.is_none() {
            // The topmost claim, and its own folder is already the truth — so
            // the chain hangs off that folder rather than off the root.
            //
            // In a vault that has never been migrated this changes nothing:
            // every note is flat in the root and the anchor is empty. It matters
            // when a run was interrupted part-way through clearing the claims,
            // because then an ancestor whose claim has already gone is sitting in
            // `Research/`, and computing this note's home from the root would
            // move it back out of the folder the same migration just put it in.
            anchor = folder_of(&ancestor.relative);
            break;
        }
        current = ancestor.parent.as_deref();
    }

    chain.reverse();
    if !anchor.is_empty() {
        let mut full: Vec<String> = anchor.split('/').map(str::to_string).collect();
        full.append(&mut chain);
        chain = full;
    }
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

/// A filename folded to the form filesystems compare by.
///
/// Case only. Unicode normalisation is deliberately *not* applied: HFS+ and
/// APFS normalise to NFD while Linux stores whatever bytes it was given, so a
/// title like "Sb\u{2082}Se\u{2083}" or an accented name can round-trip
/// differently — but folding it here would make two genuinely different titles
/// collide, and the cost of that is a note that cannot be created. Case is the
/// collision that actually bites, and the one every affected filesystem agrees
/// about.
fn fold_name(name: &str) -> String {
    name.to_lowercase()
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

    // Names already in this directory, folded for comparison.
    //
    // Testing `directory.join(&name).exists()` was the obvious thing and it is
    // wrong across platforms: NTFS and APFS treat "Growth.md" and "growth.md"
    // as one file, ext4 treats them as two. A vault written on Linux with both
    // therefore cannot be checked out on Windows, and a note created there
    // silently overwrites the other. Comparing case-folded names makes the
    // answer the same everywhere, at the cost of a suffix that a Linux-only
    // user did not strictly need — the safe direction, since a vault is a
    // folder people sync between machines.
    let taken: HashSet<String> = fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(fold_name))
        .collect();

    for attempt in 0..1000 {
        let name = if attempt == 0 {
            note::file_name(title)
        } else {
            format!("{stem} {}.md", attempt + 1)
        };
        if Some(name.as_str()) == keep_name || !taken.contains(&fold_name(&name)) {
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

/// Which of two files claiming one id is the one to open.
///
/// Every conflict convention *decorates* the name Sutra wrote — Dropbox adds
/// "(conflicted copy)", OneDrive appends the machine name, Explorer adds
/// "(1)". None of them shortens it. So the shortest file name is Sutra's own,
/// and ties break lexicographically so the answer never depends on the
/// filesystem's iteration order.
///
/// The other copy is not hidden: it is still listed, still on disk, and still
/// readable. This decides only which one `[[links]]` and the note list open.
fn canonical_first(a: &str, b: &str) -> std::cmp::Ordering {
    let name = |path: &str| path.rsplit('/').next().unwrap_or(path).chars().count();
    name(a).cmp(&name(b)).then_with(|| a.cmp(b))
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
    /// had survives.
    ///
    /// Links and backlinks need no bookkeeping at all: they name the note's
    /// id, and the id is not in the path. That is the point of the layout.
    ///
    /// The attachment is the exception, and v0.2.1 corrected what this test
    /// used to claim about it. A reference like
    /// `Research/Sb2Se3/.attachments/01H_dsc.png` *is* a path, so it was only
    /// still resolving after a move because the picture had been left behind
    /// in a folder the note no longer lived in. That is not a surviving
    /// relationship, it is a postponed break — delete the old project folder
    /// and every figure in the moved note goes with it.
    ///
    /// So the note's own attachments now travel with it and the body's
    /// reference is retargeted to match. The body is no longer byte-identical
    /// across a move, and that is the deliberate change: what must survive is
    /// the *relationship*, and asserting the bytes was only ever a proxy for
    /// it. Everything a person wrote is still untouched — see the assertions
    /// below, which pin the prose and the wikilinks exactly.
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

        // The prose and every wikilink are exactly as they were. Only the
        // attachment's path moved with the file it names.
        assert!(
            after.body.contains("Ribbons align."),
            "the prose was altered: {}",
            after.body
        );
        assert!(
            after.body.contains(&format!("[[{}]]", other.summary.id)),
            "a wikilink was rewritten, and links must never be: {}",
            after.body
        );
        assert_eq!(
            crate::links::extract(&after.body),
            crate::links::extract(&before.body),
            "the links out of this note must be untouched"
        );

        // The figure followed the note, and still resolves.
        let moved_reference = attachments::extract(&after.body);
        assert_eq!(moved_reference.len(), 1);
        assert!(
            moved_reference[0].starts_with("Research/SbSeI/Thermodynamics/.attachments/"),
            "the attachment did not follow the note: {}",
            moved_reference[0]
        );
        assert_eq!(
            vault.read_attachment(&moved_reference[0]).unwrap(),
            b"\x89PNG fake"
        );
        assert!(
            !vault.root().join(&reference).exists(),
            "a copy was left behind in the old folder"
        );

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
    fn a_note_moved_outside_the_app_is_found_again() {
        // The stale-hit case. The map still points at the old path, which is
        // not a miss — so before v0.2.1 no rescan happened, the read failed,
        // and the watcher dropped the note out of the index.
        let vault = TempVault::new();
        let note = vault.create_note("Growth", folder("A")).unwrap();
        let id = note.summary.id.clone();
        vault.save_note(&id, "Growth", "Ribbons align.").unwrap();

        // Somebody drags it in Explorer, or a sync client does.
        let from = vault.root().join(vault.relative_for(&id).unwrap());
        fs::create_dir_all(vault.root().join("B")).unwrap();
        let to = vault.root().join("B").join("Growth.md");
        fs::rename(&from, &to).unwrap();

        let found = vault.read_note(&id).expect("the note must still be found");
        assert_eq!(found.summary.folder, "B");
        assert_eq!(found.body.trim(), "Ribbons align.");
        assert_eq!(found.summary.id, id, "and it is the same note");
    }

    #[test]
    fn a_note_deleted_outside_the_app_reads_as_gone_not_as_stale() {
        let vault = TempVault::new();
        let note = vault.create_note("Growth", folder("A")).unwrap();
        let id = note.summary.id.clone();
        fs::remove_file(vault.root().join(vault.relative_for(&id).unwrap())).unwrap();
        assert!(vault.read_note(&id).is_err());
    }

    #[test]
    fn positions_are_counted_from_the_folder_alone() {
        // `next_position` used to read every note in the vault. It now reads
        // one directory, so this pins that a busy neighbouring folder does not
        // push a new note's position up.
        let vault = TempVault::new();
        for title in ["One", "Two", "Three"] {
            vault.create_note(title, folder("Busy")).unwrap();
        }
        let first = vault.create_note("First here", folder("Quiet")).unwrap();
        assert_eq!(
            first.summary.position, 0,
            "a note in an empty folder starts at zero"
        );
        let second = vault.create_note("Second here", folder("Quiet")).unwrap();
        assert_eq!(second.summary.position, 1);
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

    fn annotation(
        key: &str,
        text: Option<&str>,
        comment: Option<&str>,
    ) -> crate::references::Annotation {
        use crate::references::Annotation;
        Annotation {
            key: key.to_string(),
            kind: Some("highlight".to_string()),
            text: text.map(str::to_string),
            comment: comment.map(str::to_string),
            colour: Some("#ffd400".to_string()),
            page: Some("S12".to_string()),
            sort_index: Some("00001|000000|00010".to_string()),
        }
    }

    /// The single most important assertion in the annotation path. Zotero hands
    /// over the highlighted text and the researcher's remark in one object, and
    /// if they arrive in the note as one string the Source-versus-Interpretation
    /// invariant is gone and no later reader can recover which words were the
    /// author's.
    #[test]
    fn an_imported_annotation_keeps_the_authors_words_out_of_the_readers() {
        let vault = TempVault::new();
        let source = vault.create_note("Zhou 2019", None).unwrap();
        let note = vault.create_note("Growth", None).unwrap();

        let added = vault
            .capture_annotations(
                &note.summary.id,
                &source.summary.id,
                &[annotation(
                    "AN1",
                    Some("ribbons grow along [001]"),
                    Some("does this hold above 300 C?"),
                )],
            )
            .unwrap();
        assert_eq!(added, 1);

        let evidence = &vault.read_note(&note.summary.id).unwrap().summary.sources[0];
        assert_eq!(evidence.quote.as_deref(), Some("ribbons grow along [001]"));
        assert_eq!(
            evidence.comment.as_deref(),
            Some("does this hold above 300 C?")
        );
        assert_eq!(evidence.page.as_deref(), Some("S12"));
        assert_eq!(evidence.colour.as_deref(), Some("#ffd400"));
        assert_eq!(evidence.annotation.as_deref(), Some("AN1"));
        assert_eq!(evidence.id, source.summary.id, "evidence names the source");
        assert!(
            !evidence.eid.is_empty(),
            "every piece of evidence gets an id"
        );

        // The colour is carried and nothing is derived from it: it must not
        // have become a kind.
        assert!(
            evidence.kind.is_none(),
            "a colour is not an evidence kind; {:?} was invented",
            evidence.kind
        );
    }

    /// "Import annotations" must be a thing a person can press twice.
    #[test]
    fn importing_the_same_annotations_again_adds_nothing() {
        let vault = TempVault::new();
        let source = vault.create_note("Zhou 2019", None).unwrap();
        let note = vault.create_note("Growth", None).unwrap();
        let marks = [
            annotation("AN1", Some("first claim"), None),
            annotation("AN2", Some("second claim"), None),
        ];

        assert_eq!(
            vault
                .capture_annotations(&note.summary.id, &source.summary.id, &marks)
                .unwrap(),
            2
        );
        assert_eq!(
            vault
                .capture_annotations(&note.summary.id, &source.summary.id, &marks)
                .unwrap(),
            0,
            "the same annotations were captured twice"
        );

        // And a new mark made since is picked up without disturbing the rest.
        let more = [
            marks[0].clone(),
            marks[1].clone(),
            annotation("AN3", Some("a third, highlighted later"), None),
        ];
        assert_eq!(
            vault
                .capture_annotations(&note.summary.id, &source.summary.id, &more)
                .unwrap(),
            1
        );

        let sources = vault.read_note(&note.summary.id).unwrap().summary.sources;
        assert_eq!(sources.len(), 3);
        let eids: std::collections::HashSet<_> = sources.iter().map(|c| c.eid.clone()).collect();
        assert_eq!(eids.len(), 3, "every piece of evidence has its own id");
    }

    /// Zotero does not become the authority on what a researcher has recorded.
    /// Evidence captured by hand, and evidence whose annotation was later
    /// deleted in Zotero, both survive an import untouched.
    #[test]
    fn importing_never_removes_evidence_already_recorded() {
        let vault = TempVault::new();
        let source = vault.create_note("Zhou 2019", None).unwrap();
        let note = vault.create_note("Growth", None).unwrap();

        // Recorded by hand: no annotation key at all.
        vault
            .set_citations(
                &note.summary.id,
                vec![Citation {
                    eid: String::new(),
                    id: source.summary.id.clone(),
                    page: Some("4".to_string()),
                    quote: Some("typed out of the paper by hand".to_string()),
                    ..Default::default()
                }],
            )
            .unwrap();

        vault
            .capture_annotations(
                &note.summary.id,
                &source.summary.id,
                &[annotation("AN1", Some("from Zotero"), None)],
            )
            .unwrap();

        let sources = vault.read_note(&note.summary.id).unwrap().summary.sources;
        assert_eq!(sources.len(), 2);
        assert!(
            sources
                .iter()
                .any(|c| c.quote.as_deref() == Some("typed out of the paper by hand")),
            "hand-written evidence was lost: {sources:?}"
        );
        // The hand-written one was not matched against as a duplicate, and got
        // its own eid when it was written.
        assert!(sources.iter().all(|c| !c.eid.is_empty()));
    }

    /// An annotation with nothing in it is not evidence of anything. Recording
    /// a page with no quotation and no remark would be a row that looks like
    /// provenance and carries none.
    #[test]
    fn an_empty_annotation_records_nothing() {
        let vault = TempVault::new();
        let source = vault.create_note("Zhou 2019", None).unwrap();
        let note = vault.create_note("Growth", None).unwrap();

        let added = vault
            .capture_annotations(
                &note.summary.id,
                &source.summary.id,
                &[annotation("AN1", None, None)],
            )
            .unwrap();
        assert_eq!(added, 0);
        assert!(
            vault
                .read_note(&note.summary.id)
                .unwrap()
                .summary
                .sources
                .is_empty()
        );
    }

    /// A sticky note highlights nothing, and is still worth keeping: it is the
    /// researcher's own thought, attached to a page.
    #[test]
    fn a_comment_with_no_highlight_is_still_captured() {
        let vault = TempVault::new();
        let source = vault.create_note("Zhou 2019", None).unwrap();
        let note = vault.create_note("Growth", None).unwrap();

        let added = vault
            .capture_annotations(
                &note.summary.id,
                &source.summary.id,
                &[annotation("AN1", None, Some("compare with Ko 2024"))],
            )
            .unwrap();
        assert_eq!(added, 1);

        let evidence = &vault.read_note(&note.summary.id).unwrap().summary.sources[0];
        assert!(
            evidence.quote.is_none(),
            "nothing was highlighted, so there is no quotation to claim"
        );
        assert_eq!(evidence.comment.as_deref(), Some("compare with Ko 2024"));
    }

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
    fn an_attachment_follows_its_note_through_a_rename_and_a_move() {
        // The workflow this pins, end to end: attach a figure, rename the
        // note, move it to another folder, reopen it, and the figure still
        // resolves. Before this, the move left the picture in the old folder.
        let vault = TempVault::new();
        let note = vault
            .create_note("Growth", folder("Research/Sb2Se3"))
            .unwrap();
        let id = note.summary.id.clone();

        let source = vault.root().join("figure.png");
        fs::write(&source, b"\x89PNG fake").unwrap();
        let reference = vault
            .import_attachment(&source, folder("Research/Sb2Se3"))
            .unwrap();
        vault
            .save_note(&id, "Growth", &format!("Result: ![plot]({reference})"))
            .unwrap();

        // Rename first: the file moves, the id and the folder do not.
        vault.save_note(&id, "Growth run 4", "").unwrap();
        let renamed = vault.read_note(&id).unwrap();
        vault
            .save_note(
                &id,
                "Growth run 4",
                &format!("Result: ![plot]({reference})"),
            )
            .unwrap();
        assert_eq!(renamed.summary.folder, "Research/Sb2Se3");

        // Then the move.
        vault.move_note(&id, "Archive/2026").unwrap();

        let after = vault.read_note(&id).unwrap();
        assert_eq!(after.summary.folder, "Archive/2026");

        let moved = attachments::extract(&after.body);
        assert_eq!(moved.len(), 1, "the body still has one attachment");
        assert!(
            moved[0].starts_with("Archive/2026/.attachments/"),
            "the reference did not follow the note: {}",
            moved[0]
        );
        assert_eq!(
            vault.read_attachment(&moved[0]).unwrap(),
            b"\x89PNG fake",
            "the file itself did not follow the note"
        );
        assert!(
            !vault.root().join(&reference).exists(),
            "the old copy was left behind"
        );
    }

    #[test]
    fn a_moved_note_keeps_its_edit_time() {
        // Moving is not editing. Rewriting the body to retarget an attachment
        // must not stamp `updated`, or dragging a folder full of notes would
        // destroy the record of when the work actually happened.
        let vault = TempVault::new();
        let note = vault.create_note("Growth", folder("A")).unwrap();
        let id = note.summary.id.clone();
        let source = vault.root().join("f.png");
        fs::write(&source, b"png").unwrap();
        let reference = vault.import_attachment(&source, folder("A")).unwrap();
        vault
            .save_note(&id, "Growth", &format!("![p]({reference})"))
            .unwrap();

        let before = vault.read_note(&id).unwrap().summary.updated;
        vault.move_note(&id, "B").unwrap();
        assert_eq!(vault.read_note(&id).unwrap().summary.updated, before);
    }

    #[test]
    fn an_attachment_two_notes_use_is_left_where_it_is() {
        // Shared, so it belongs to neither. Moving it would fix one note by
        // breaking the other, so nothing moves and both references keep
        // resolving.
        let vault = TempVault::new();
        let first = vault.create_note("First", folder("A")).unwrap();
        let second = vault.create_note("Second", folder("A")).unwrap();
        let source = vault.root().join("shared.png");
        fs::write(&source, b"shared bytes").unwrap();
        let reference = vault.import_attachment(&source, folder("A")).unwrap();

        for note in [&first, &second] {
            vault
                .save_note(
                    &note.summary.id,
                    &note.summary.title,
                    &format!("![s]({reference})"),
                )
                .unwrap();
        }

        vault.move_note(&first.summary.id, "B").unwrap();

        assert!(
            vault.root().join(&reference).exists(),
            "a shared attachment must not be moved"
        );
        assert_eq!(
            attachments::extract(&vault.read_note(&first.summary.id).unwrap().body),
            vec![reference.clone()],
            "the moved note's reference must be left pointing at the shared file"
        );
        assert_eq!(
            vault.read_attachment(&reference).unwrap(),
            b"shared bytes",
            "the note that stayed must still resolve it"
        );
    }

    /// A move interrupted anywhere still leaves every figure loading.
    ///
    /// Moving a note is three writes — rename the note, put its attachments
    /// beside it, rewrite the references — and a laptop lid can close between
    /// any two of them. The ordering is chosen so that no gap between them is a
    /// broken state: the attachment is *copied* first, so both the old and the
    /// new path hold the file while the body still names the old one; the body
    /// is written next; only then is the old copy removed.
    ///
    /// The two intermediate states are built here by hand, because the point is
    /// not that Sutra reaches them — it is that a vault found in one of them is
    /// a vault whose pictures all still load.
    #[test]
    fn an_interrupted_move_never_leaves_a_figure_that_cannot_load() {
        let vault = TempVault::new();
        let note = vault.create_note("Growth", folder("A")).unwrap();
        let id = note.summary.id.clone();
        let source = vault.root().join("figure.png");
        fs::write(&source, b"the figure").unwrap();
        let old_reference = vault.import_attachment(&source, folder("A")).unwrap();
        vault
            .save_note(&id, "Growth", &format!("![f]({old_reference})"))
            .unwrap();

        let name = Path::new(&old_reference).file_name().unwrap();
        let new_reference = format!("B/{ATTACHMENTS}/{}", name.to_str().unwrap());

        // State 1: copied, body not yet rewritten. The note still names the old
        // path, and the old path is still there to be named.
        fs::create_dir_all(vault.root().join("B").join(ATTACHMENTS)).unwrap();
        fs::copy(
            vault.root().join(&old_reference),
            vault.root().join(&new_reference),
        )
        .unwrap();
        assert_eq!(
            vault.read_attachment(&old_reference).unwrap(),
            b"the figure",
            "between the copy and the rewrite, the old reference must still resolve"
        );

        // State 2: body rewritten, old copy not yet removed. The note names the
        // new path, which exists; the stray copy is untidy and harmless.
        vault
            .save_note(&id, "Growth", &format!("![f]({new_reference})"))
            .unwrap();
        for reference in attachments::extract(&vault.read_note(&id).unwrap().body) {
            assert_eq!(
                vault.read_attachment(&reference).unwrap(),
                b"the figure",
                "after the rewrite, {reference} must resolve"
            );
        }
    }

    /// A completed move leaves one copy of the picture, not two.
    ///
    /// The copy-then-delete ordering above is only safe if the delete actually
    /// happens; otherwise every move would quietly double the vault's figures.
    #[test]
    fn a_completed_move_leaves_no_stray_copy_behind() {
        let vault = TempVault::new();
        let note = vault.create_note("Growth", folder("A")).unwrap();
        let id = note.summary.id.clone();
        let source = vault.root().join("figure.png");
        fs::write(&source, b"the figure").unwrap();
        let reference = vault.import_attachment(&source, folder("A")).unwrap();
        vault
            .save_note(&id, "Growth", &format!("![f]({reference})"))
            .unwrap();

        vault.move_note(&id, "B").unwrap();

        assert!(
            !vault.root().join(&reference).exists(),
            "the picture was copied to the new folder but never removed from the old"
        );
        let after = attachments::extract(&vault.read_note(&id).unwrap().body);
        assert_eq!(after.len(), 1);
        assert!(
            after[0].starts_with("B/"),
            "the reference should name the new folder, got {:?}",
            after[0]
        );
        assert_eq!(vault.read_attachment(&after[0]).unwrap(), b"the figure");
    }

    #[test]
    fn deleting_a_note_takes_its_own_attachment_to_the_trash() {
        let vault = TempVault::new();
        let note = vault.create_note("Growth", folder("A")).unwrap();
        let source = vault.root().join("f.png");
        fs::write(&source, b"the figure").unwrap();
        let reference = vault.import_attachment(&source, folder("A")).unwrap();
        vault
            .save_note(&note.summary.id, "Growth", &format!("![p]({reference})"))
            .unwrap();

        vault.delete_note(&note.summary.id).unwrap();

        assert!(
            !vault.root().join(&reference).exists(),
            "the attachment stayed in the vault after its only note was deleted"
        );
        // Trashed, not unlinked: it has to be recoverable by hand.
        let trash = vault.root().join(SUTRA).join(TRASH);
        let recovered: Vec<_> = fs::read_dir(&trash)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".png"))
            .collect();
        assert_eq!(recovered.len(), 1, "the figure is not in the trash");
        assert_eq!(fs::read(recovered[0].path()).unwrap(), b"the figure");
    }

    #[test]
    fn deleting_a_note_leaves_an_attachment_another_note_still_uses() {
        let vault = TempVault::new();
        let first = vault.create_note("First", folder("A")).unwrap();
        let second = vault.create_note("Second", folder("A")).unwrap();
        let source = vault.root().join("shared.png");
        fs::write(&source, b"shared").unwrap();
        let reference = vault.import_attachment(&source, folder("A")).unwrap();
        for note in [&first, &second] {
            vault
                .save_note(
                    &note.summary.id,
                    &note.summary.title,
                    &format!("![s]({reference})"),
                )
                .unwrap();
        }

        vault.delete_note(&first.summary.id).unwrap();

        assert_eq!(
            vault.read_attachment(&reference).unwrap(),
            b"shared",
            "the surviving note's figure was trashed with the other note"
        );
    }

    #[test]
    fn a_note_that_references_nothing_of_ours_is_moved_untouched() {
        // A remote image and a link to another note are not attachments, and a
        // move must not rewrite either.
        let vault = TempVault::new();
        let note = vault.create_note("Reading", folder("A")).unwrap();
        let body = "![web](https://example.com/x.png) and [[01HQ3M8K2P0000000000000001]]";
        vault.save_note(&note.summary.id, "Reading", body).unwrap();

        vault.move_note(&note.summary.id, "B").unwrap();

        assert_eq!(vault.read_note(&note.summary.id).unwrap().body.trim(), body);
    }

    #[test]
    fn a_dangling_reference_is_left_saying_what_it_says() {
        // The picture is already gone. Moving the note must not invent a new
        // target for it, which would hide the loss.
        let vault = TempVault::new();
        let note = vault.create_note("Growth", folder("A")).unwrap();
        let body = "![p](A/.attachments/01HQ3M8K2P0000000000000001_gone.png)";
        vault.save_note(&note.summary.id, "Growth", body).unwrap();

        vault.move_note(&note.summary.id, "B").unwrap();

        assert_eq!(vault.read_note(&note.summary.id).unwrap().body.trim(), body);
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

    /// A note that claims no parent is not the migration's business.
    ///
    /// This is the bug that made an interrupted migration dangerous. The plan
    /// derived every note's target folder from its chain of `parent` claims —
    /// including notes that had no claim at all, whose chain is empty and whose
    /// target therefore came out as the vault root. So a single note still
    /// claiming a parent was enough to make the plan propose flattening every
    /// organised note in the vault into the root.
    ///
    /// And that is exactly the state a half-finished migration leaves behind:
    /// files already in their new folders, claims cleared one at a time. The
    /// second run would have undone the first.
    #[test]
    fn a_note_with_no_claim_is_left_in_the_folder_it_is_in() {
        let vault = TempVault::new();
        // One legacy note, so the vault does need migrating at all.
        legacy(&vault, A, "Research", None);

        // And one note already where it belongs, claiming nothing.
        let organised = vault
            .create_note("Growth log", folder("Research/Sb2Se3"))
            .unwrap();

        let plan = vault.migration_plan().unwrap();
        for (from, to) in &plan.moves {
            assert!(
                !from.starts_with("Research/Sb2Se3/"),
                "the plan wants to move an already-organised note to {to}"
            );
        }

        vault.migrate().unwrap();
        assert_eq!(
            vault
                .read_note(&organised.summary.id)
                .unwrap()
                .summary
                .folder,
            "Research/Sb2Se3",
            "migrating flattened a note that was already in the right place"
        );
    }

    /// Running the migration twice must change nothing the second time.
    ///
    /// An interrupted run is indistinguishable from a completed one that is
    /// asked to run again, so idempotence is the property that makes
    /// interruption survivable. It has to hold at depth: the resumed run sees
    /// some claims cleared and some not, and must still compute the same
    /// destination for a note whose parent's claim has already gone.
    #[test]
    fn migrating_a_second_time_moves_nothing() {
        let vault = TempVault::new();
        legacy(&vault, A, "Research", None);
        legacy(&vault, B, "Sb2Se3", Some(A));
        legacy(&vault, C, "Cp", Some(B));

        vault.migrate().unwrap();
        let after_first: Vec<String> = vault
            .list_notes()
            .unwrap()
            .into_iter()
            .map(|n| format!("{}/{}", n.folder, n.title))
            .collect();

        // Nothing claims a parent any more, so there is nothing to do — but the
        // plan must say so rather than proposing to move everything to the root.
        assert!(vault.migration_plan().unwrap().moves.is_empty());
        vault.migrate().unwrap();

        let after_second: Vec<String> = vault
            .list_notes()
            .unwrap()
            .into_iter()
            .map(|n| format!("{}/{}", n.folder, n.title))
            .collect();
        assert_eq!(after_first, after_second);
    }

    /// A migration interrupted between clearing one claim and the next.
    ///
    /// Built by hand, because the point is not that Sutra reaches this state but
    /// that a vault found in it can be finished. The files are in their new
    /// folders; the parent notes' claims are cleared; the deepest note's is not.
    /// Finishing must leave it where it already is.
    #[test]
    fn a_migration_interrupted_half_way_finishes_where_it_left_off() {
        let vault = TempVault::new();
        legacy(&vault, A, "Research", None);
        legacy(&vault, B, "Sb2Se3", Some(A));
        legacy(&vault, C, "Cp", Some(B));
        vault.migrate().unwrap();

        // Put the deepest note's claim back: the run got as far as moving every
        // file and clearing its ancestors, and stopped before this one.
        let path = vault.root().join("Research/Sb2Se3/Cp.md");
        let contents = fs::read_to_string(&path).unwrap();
        let (fm, body) = frontmatter::split(&contents).unwrap();
        let mut fm = fm.unwrap();
        fm.parent = Some(B.to_string());
        let body = body.to_string();
        note::write_atomic(&path, &frontmatter::join(&fm, &body).unwrap()).unwrap();
        vault.list_notes().unwrap();

        assert!(
            vault.needs_migration().unwrap(),
            "a note still claiming a parent means the vault is not finished"
        );

        vault.migrate().unwrap();

        assert_eq!(
            vault.read_note(C).unwrap().summary.folder,
            "Research/Sb2Se3",
            "finishing the migration moved the note out of the folder it was already in"
        );
        assert!(!vault.needs_migration().unwrap());
        assert!(vault.root().join("Research/Sb2Se3/Cp.md").is_file());
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
    fn re_importing_keeps_the_cached_citation_styles() {
        // The whole point of caching how a paper is formatted is that a thesis
        // draft written on a train still shows "(Ko et al., 2024)" with Zotero
        // closed. Re-importing the paper must not throw that away — an import
        // brings fresh *bibliographic* facts, and how those facts were once
        // rendered is not one of them.
        let vault = TempVault::new();
        let source = vault
            .import_source("Zhou 2019", paper("10.1000/old"))
            .unwrap();

        vault
            .cache_style(
                &source.id,
                "american-chemical-society",
                crate::references::StyledCitation {
                    citation: Some("(1)".into()),
                    bib: Some("Zhou, Y.; Wang, L. Nature Energy 2019.".into()),
                },
            )
            .unwrap();
        vault
            .cache_style(
                &source.id,
                "apa",
                crate::references::StyledCitation {
                    citation: Some("(Zhou & Wang, 2019)".into()),
                    bib: Some("Zhou, Y., & Wang, L. (2019).".into()),
                },
            )
            .unwrap();

        // A fresh fetch from the library. `Reference::to_source` builds one of
        // these with an empty `styled` map, because a search response says
        // nothing about formatting.
        vault
            .import_source("Zhou 2019", paper("10.1000/new"))
            .unwrap();

        let after = vault.read_note(&source.id).unwrap().summary.source.unwrap();
        assert_eq!(
            after.doi.as_deref(),
            Some("10.1000/new"),
            "the fresh bibliographic fact must win"
        );
        assert_eq!(
            after.styled.len(),
            2,
            "both cached styles must survive the re-import, got {:?}",
            after.styled.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            after.styled["apa"].citation.as_deref(),
            Some("(Zhou & Wang, 2019)")
        );
    }

    #[test]
    fn re_importing_keeps_collections_and_the_pdf_when_the_fetch_has_neither() {
        // `import_zotero_source` takes the cheap path: one search response,
        // which carries no collections and no attachments. It must not read as
        // "this paper is now in no collections and has no PDF".
        let vault = TempVault::new();
        let mut full = paper("10.1000/x");
        full.collections = vec!["Sb2Se3".into(), "To read".into()];
        full.pdf = Some("Zhou et al. - 2019.pdf".into());
        let source = vault.import_source("Zhou 2019", full).unwrap();

        vault
            .import_source("Zhou 2019", paper("10.1000/x"))
            .unwrap();

        let after = vault.read_note(&source.id).unwrap().summary.source.unwrap();
        assert_eq!(after.collections, vec!["Sb2Se3", "To read"]);
        assert_eq!(after.pdf.as_deref(), Some("Zhou et al. - 2019.pdf"));
    }

    #[test]
    fn a_re_import_that_does_carry_collections_replaces_them() {
        // The other half of the rule: when the library *does* answer, it is the
        // authority. An item moved out of a collection in Zotero must not keep
        // claiming membership here for ever.
        let vault = TempVault::new();
        let mut first = paper("10.1000/x");
        first.collections = vec!["To read".into()];
        let source = vault.import_source("Zhou 2019", first).unwrap();

        let mut second = paper("10.1000/x");
        second.collections = vec!["Read".into()];
        vault.import_source("Zhou 2019", second).unwrap();

        assert_eq!(
            vault
                .read_note(&source.id)
                .unwrap()
                .summary
                .source
                .unwrap()
                .collections,
            vec!["Read"]
        );
    }

    #[test]
    fn a_re_import_never_blanks_a_field_it_has_no_answer_for() {
        // Zotero going quiet on one field is not the same as Zotero saying the
        // field is empty. Only a real value replaces a real value.
        let vault = TempVault::new();
        let mut rich = paper("10.1000/x");
        rich.citation_key = Some("zhou2019".into());
        rich.abstract_text = Some("Sb2Se3 thin films were grown by...".into());
        rich.item_type = Some("journalArticle".into());
        let source = vault.import_source("Zhou 2019", rich).unwrap();

        // A sparse response: title and key only, as a degraded fetch gives.
        let sparse = SourceMeta {
            zotero: Some("ABCD1234".into()),
            ..Default::default()
        };
        vault.import_source("Zhou 2019", sparse).unwrap();

        let after = vault.read_note(&source.id).unwrap().summary.source.unwrap();
        assert_eq!(after.citation_key.as_deref(), Some("zhou2019"));
        assert_eq!(after.item_type.as_deref(), Some("journalArticle"));
        assert!(after.abstract_text.is_some());
        assert_eq!(after.doi.as_deref(), Some("10.1000/x"));
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

    /// The citation migration keeps a copy first, like the other one.
    ///
    /// It rewrites prose in every note in the vault, which makes it a migration
    /// in every sense the invariant means — "every migration detects, plans,
    /// previews, backs up, applies and verifies". For two releases this one
    /// skipped the backup, and nothing outside the code could tell.
    #[test]
    fn migrating_citations_keeps_a_copy_of_every_note_first() {
        let vault = TempVault::new();
        let source = vault.create_source("S", paper("10.1000/x")).unwrap();
        let note = vault.create_note("Citing", folder("Research")).unwrap();
        vault
            .save_note(&note.summary.id, "Citing", "See [@ABCD1234].")
            .unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("ABCD1234".to_string(), source.summary.id.clone());
        vault.migrate_citations(&mapping).unwrap();

        let backups = vault.root().join(SUTRA).join("backups");
        let run = fs::read_dir(&backups)
            .expect("a migration must leave a backup folder")
            .next()
            .expect("and something in it")
            .unwrap();
        let kept = fs::read_to_string(run.path().join("Research").join("Citing.md"))
            .expect("the note it was about to rewrite");
        assert!(
            kept.contains("[@ABCD1234]"),
            "the copy should hold the note as it was before the rewrite: {kept}"
        );
    }

    /// Running the citation migration again changes nothing.
    ///
    /// The keys it could resolve are gone from the prose, so there is nothing
    /// left to find; the ones it could not are still there, still unresolved,
    /// and still not deleted.
    #[test]
    fn migrating_citations_a_second_time_changes_nothing() {
        let vault = TempVault::new();
        let source = vault.create_source("S", paper("10.1000/x")).unwrap();
        let note = vault.create_note("Citing", None).unwrap();
        vault
            .save_note(
                &note.summary.id,
                "Citing",
                "As [@ABCD1234] shows, unlike [@ZZZZ9999].",
            )
            .unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("ABCD1234".to_string(), source.summary.id.clone());
        assert_eq!(vault.migrate_citations(&mapping).unwrap(), 1);
        let once = vault.read_note(&note.summary.id).unwrap().body;

        assert_eq!(
            vault.migrate_citations(&mapping).unwrap(),
            0,
            "the second run found something to rewrite"
        );
        assert_eq!(vault.read_note(&note.summary.id).unwrap().body, once);
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
    // ---- v0.3: recovery -----------------------------------------------------

    #[test]
    fn a_whole_folder_moved_outside_the_app_is_found_again() {
        // Dragging a project folder in Explorer is an ordinary thing to do.
        // Every note in it keeps its id and its content; only the folder
        // changes, because the folder was never anything but where the file
        // is.
        let vault = TempVault::new();
        let a = vault
            .create_note("Growth", folder("Research/Sb2Se3"))
            .unwrap();
        let b = vault
            .create_note("Phonons", folder("Research/Sb2Se3"))
            .unwrap();
        vault
            .save_note(&a.summary.id, "Growth", "Ribbons align.")
            .unwrap();

        fs::create_dir_all(vault.root().join("Archive")).unwrap();
        fs::rename(
            vault.root().join("Research/Sb2Se3"),
            vault.root().join("Archive/Sb2Se3"),
        )
        .unwrap();

        let moved = vault.read_note(&a.summary.id).expect("note must be found");
        assert_eq!(moved.summary.folder, "Archive/Sb2Se3");
        assert_eq!(moved.body.trim(), "Ribbons align.");
        assert_eq!(
            vault.read_note(&b.summary.id).unwrap().summary.folder,
            "Archive/Sb2Se3"
        );
    }

    #[test]
    fn a_note_with_malformed_frontmatter_does_not_take_the_vault_down() {
        // One broken file must not stop the other notes being listed. It is a
        // corrupted note, not a corrupted vault, and the distinction is the
        // difference between losing one file and losing an afternoon.
        let vault = TempVault::new();
        let good = vault.create_note("Fine", None).unwrap();
        vault
            .save_note(&good.summary.id, "Fine", "Readable.")
            .unwrap();

        fs::write(
            vault.root().join("Broken.md"),
            "---\nid: [this is not\n  valid: yaml: at all\n---\n\nBody.\n",
        )
        .unwrap();

        let notes = vault.list_notes().expect("listing must still work");
        assert!(
            notes.iter().any(|n| n.title == "Fine"),
            "a broken neighbour hid a good note"
        );
        // And the broken file is still on disk, untouched.
        assert!(vault.root().join("Broken.md").exists());
    }

    #[test]
    fn a_note_left_half_written_does_not_replace_the_good_one() {
        // `write_atomic` writes a temp file and renames. An interruption
        // leaves the temp file behind and the note as it was — never a note
        // with half its body.
        let vault = TempVault::new();
        let note = vault.create_note("Growth", None).unwrap();
        let id = note.summary.id.clone();
        vault
            .save_note(&id, "Growth", "The complete body.")
            .unwrap();

        // The debris an interrupted write leaves.
        let path = vault.root().join(vault.relative_for(&id).unwrap());
        fs::write(path.with_extension("md.tmp"), "half a bo").unwrap();

        assert_eq!(
            vault.read_note(&id).unwrap().body.trim(),
            "The complete body."
        );
        // And the leftover is not mistaken for a note.
        let notes = vault.list_notes().unwrap();
        assert_eq!(notes.len(), 1, "a .tmp file was listed as a note");
    }

    #[test]
    fn a_source_whose_zotero_item_vanished_keeps_everything_recorded() {
        // Deleting an item in Zotero must not reach back into the vault. What
        // Sutra recorded is the researcher's, and it stays.
        let vault = TempVault::new();
        let mut meta = paper("10.1000/x");
        meta.citation_key = Some("zhou2019".into());
        meta.styled.insert(
            "american-chemical-society".into(),
            crate::references::StyledCitation {
                citation: Some("(1)".into()),
                bib: Some("Zhou, Y. Nature Energy 2019.".into()),
            },
        );
        let source = vault.import_source("Zhou 2019", meta).unwrap();

        // Zotero is now gone; nothing in the vault changes, because nothing in
        // the vault ever asked it at read time.
        let after = vault.read_note(&source.id).unwrap().summary.source.unwrap();
        assert_eq!(after.zotero.as_deref(), Some("ABCD1234"));
        assert_eq!(after.citation_key.as_deref(), Some("zhou2019"));
        assert_eq!(
            after.styled["american-chemical-society"].bib.as_deref(),
            Some("Zhou, Y. Nature Energy 2019.")
        );
    }

    // ---- v0.3: evidence identity ------------------------------------------

    #[test]
    fn every_recorded_piece_of_evidence_gets_its_own_id() {
        // `id` says which paper; `eid` says which reading of it. Without the
        // second, two records of the same source at the same page are the same
        // record as far as anything outside this note can tell.
        let vault = TempVault::new();
        let source = vault
            .import_source("Zhou 2019", paper("10.1000/x"))
            .unwrap();
        let note = vault.create_note("Reading", None).unwrap();

        vault
            .set_citations(
                &note.summary.id,
                vec![
                    Citation {
                        id: source.id.clone(),
                        page: Some("S12".into()),
                        ..Default::default()
                    },
                    Citation {
                        id: source.id.clone(),
                        page: Some("S12".into()),
                        ..Default::default()
                    },
                ],
            )
            .unwrap();

        let recorded = vault.read_note(&note.summary.id).unwrap().summary.sources;
        assert_eq!(recorded.len(), 2);
        assert!(!recorded[0].eid.is_empty(), "no evidence id was minted");
        assert_ne!(
            recorded[0].eid, recorded[1].eid,
            "two readings of one page must be two pieces of evidence"
        );
    }

    #[test]
    fn an_evidence_id_survives_editing_the_record_around_it() {
        // Correcting a page number is not a new observation. If the id moved,
        // nothing could hold a reference to a piece of evidence for longer
        // than one edit.
        let vault = TempVault::new();
        let source = vault
            .import_source("Zhou 2019", paper("10.1000/x"))
            .unwrap();
        let note = vault.create_note("Reading", None).unwrap();
        vault
            .set_citations(
                &note.summary.id,
                vec![Citation {
                    id: source.id.clone(),
                    page: Some("S12".into()),
                    ..Default::default()
                }],
            )
            .unwrap();

        let mut held = vault.read_note(&note.summary.id).unwrap().summary.sources;
        let original = held[0].eid.clone();
        held[0].page = Some("S13".into());
        held[0].quote = Some("thermal conductivity decreases".into());
        vault.set_citations(&note.summary.id, held).unwrap();

        let after = vault.read_note(&note.summary.id).unwrap().summary.sources;
        assert_eq!(after[0].eid, original, "the evidence was re-identified");
        assert_eq!(after[0].page.as_deref(), Some("S13"));
    }

    // ---- v0.5: the id becomes load-bearing ---------------------------------

    #[test]
    fn an_evidence_id_dropped_in_transit_is_restored_rather_than_reminted() {
        // The failure this guards: a caller sends the list back having lost
        // one `eid`, a fresh one is minted into the empty slot, and every
        // interpretation resting on the old id now rests on nothing. Silent,
        // and indistinguishable afterwards from evidence that never existed.
        let vault = TempVault::new();
        let source = vault
            .import_source("Zhou 2019", paper("10.1000/x"))
            .unwrap();
        let note = vault.create_note("Reading", None).unwrap();
        vault
            .set_citations(
                &note.summary.id,
                vec![Citation {
                    id: source.id.clone(),
                    page: Some("S12".into()),
                    quote: Some("ribbons align along c".into()),
                    ..Default::default()
                }],
            )
            .unwrap();
        let minted = vault.read_note(&note.summary.id).unwrap().summary.sources[0]
            .eid
            .clone();

        // The same record, same source and same words, arriving with no id.
        vault
            .set_citations(
                &note.summary.id,
                vec![Citation {
                    id: source.id.clone(),
                    page: Some("S13".into()),
                    quote: Some("ribbons align along c".into()),
                    ..Default::default()
                }],
            )
            .unwrap();

        let after = vault.read_note(&note.summary.id).unwrap().summary.sources;
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].eid, minted, "the evidence was re-identified");
        // And the edit it arrived with still landed.
        assert_eq!(after[0].page.as_deref(), Some("S13"));
    }

    #[test]
    fn two_indistinguishable_orphans_are_not_guessed_between() {
        // Restoring an id requires exactly one candidate. Two records quoting
        // one source at the same words are not something to pick between, and
        // a wrongly re-used id is worse than a duplicate: a duplicate is
        // visible and repairable, a wrong one is neither.
        let vault = TempVault::new();
        let source = vault
            .import_source("Zhou 2019", paper("10.1000/x"))
            .unwrap();
        let note = vault.create_note("Reading", None).unwrap();
        let same = || Citation {
            id: source.id.clone(),
            quote: Some("ribbons align along c".into()),
            ..Default::default()
        };
        vault
            .set_citations(&note.summary.id, vec![same(), same()])
            .unwrap();
        let held: Vec<String> = vault
            .read_note(&note.summary.id)
            .unwrap()
            .summary
            .sources
            .iter()
            .map(|c| c.eid.clone())
            .collect();

        // Both arrive back with their ids gone.
        vault
            .set_citations(&note.summary.id, vec![same(), same()])
            .unwrap();

        let after = vault.read_note(&note.summary.id).unwrap().summary.sources;
        assert_eq!(after.len(), 2);
        assert_ne!(after[0].eid, after[1].eid, "two records, two ids");
        for eid in &held {
            assert!(
                !after.iter().any(|c| &c.eid == eid),
                "an id was re-used on a record that could not be identified"
            );
        }
    }

    #[test]
    fn an_id_is_not_restored_onto_evidence_quoting_something_else() {
        // Same source, different words, so it is different evidence. Reviving
        // the old id here would attach an interpretation to a sentence its
        // author never read.
        let vault = TempVault::new();
        let source = vault
            .import_source("Zhou 2019", paper("10.1000/x"))
            .unwrap();
        let note = vault.create_note("Reading", None).unwrap();
        vault
            .set_citations(
                &note.summary.id,
                vec![Citation {
                    id: source.id.clone(),
                    quote: Some("ribbons align along c".into()),
                    ..Default::default()
                }],
            )
            .unwrap();
        let first = vault.read_note(&note.summary.id).unwrap().summary.sources[0]
            .eid
            .clone();

        vault
            .set_citations(
                &note.summary.id,
                vec![Citation {
                    id: source.id.clone(),
                    quote: Some("conductivity falls above 400 K".into()),
                    ..Default::default()
                }],
            )
            .unwrap();

        let after = vault.read_note(&note.summary.id).unwrap().summary.sources;
        assert_ne!(after[0].eid, first, "a different quotation took over an id");
    }

    #[test]
    fn an_imported_annotation_records_where_it_came_from() {
        // `origin` is written, not derived. The distinction it exists for is
        // between text the app took out of a PDF and text a person typed, and
        // nothing on the record could tell those apart before.
        let vault = TempVault::new();
        let source = vault
            .import_source("Zhou 2019", paper("10.1000/x"))
            .unwrap();
        let note = vault.create_note("Reading", None).unwrap();
        let added = vault
            .capture_annotations(
                &note.summary.id,
                &source.id,
                &[crate::references::Annotation {
                    key: "ZAB12CD3".into(),
                    kind: Some("highlight".into()),
                    text: Some("ribbons align along c".into()),
                    comment: Some("only two samples".into()),
                    colour: Some("#ffd400".into()),
                    page: Some("S12".into()),
                    sort_index: None,
                }],
            )
            .unwrap();
        assert_eq!(added, 1);

        let recorded = &vault.read_note(&note.summary.id).unwrap().summary.sources[0];
        assert_eq!(recorded.origin.as_deref(), Some("annotation"));
        // And the invariant the whole import exists for, still held.
        assert_eq!(recorded.quote.as_deref(), Some("ribbons align along c"));
        assert_eq!(recorded.comment.as_deref(), Some("only two samples"));
    }

    #[test]
    fn a_v0_4_record_gains_no_v0_5_keys_by_being_read_and_written() {
        // The additive rule, tested at the byte level rather than trusted:
        // `zotero`, `page_index` and `origin` must be absent on a record that
        // never had them, not written out as nulls.
        let vault = TempVault::new();
        let source = vault
            .import_source("Zhou 2019", paper("10.1000/x"))
            .unwrap();
        let note = vault.create_note("Reading", None).unwrap();
        vault
            .set_citations(
                &note.summary.id,
                vec![Citation {
                    id: source.id.clone(),
                    page: Some("S12".into()),
                    quote: Some("ribbons align along c".into()),
                    ..Default::default()
                }],
            )
            .unwrap();

        let raw = fs::read_to_string(vault.path_for(&note.summary.id).unwrap()).unwrap();
        for key in ["zotero:", "page_index:", "origin:"] {
            assert!(
                !raw.contains(key),
                "{key} was written onto a record that has no such fact"
            );
        }
    }

    #[test]
    fn a_v0_2_note_without_evidence_ids_is_read_unchanged() {
        // Backward compatibility, at the byte level: opening a v0.2 vault must
        // not rewrite it, and an absent `eid` is absent, not empty-string.
        let vault = TempVault::new();
        let note = vault.create_note("Reading", None).unwrap();
        let path = vault.path_for(&note.summary.id).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let legacy = raw.replace(
            "tags: []",
            "tags: []\nsources:\n  - id: 01HQ3M8K2P0000000000000001\n    page: S12",
        );
        fs::write(&path, &legacy).unwrap();

        let read = vault.read_note(&note.summary.id).unwrap();
        assert_eq!(read.summary.sources.len(), 1);
        assert_eq!(read.summary.sources[0].page.as_deref(), Some("S12"));
        assert!(
            read.summary.sources[0].eid.is_empty(),
            "reading must not invent an id"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            legacy,
            "opening a v0.2 note rewrote it"
        );
    }

    // ---- v0.4: chapters ----------------------------------------------------

    #[test]
    fn a_chapter_is_a_note_that_names_other_notes_in_order() {
        let vault = TempVault::new();
        let a = vault.create_note("Growth", None).unwrap();
        let b = vault.create_note("Optics", None).unwrap();
        let chapter = vault.create_chapter("3. Sb2Se3", None).unwrap();

        assert_eq!(chapter.summary.note_type, NoteType::Chapter);
        vault
            .set_sequence(
                &chapter.summary.id,
                vec![b.summary.id.clone(), a.summary.id.clone()],
            )
            .unwrap();

        let entries = vault.chapter(&chapter.summary.id).unwrap();
        let titles: Vec<&str> = entries
            .iter()
            .map(|e| e.note.as_ref().unwrap().title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec!["Optics", "Growth"],
            "the chapter's order is the order it was given, not the notes' own"
        );
    }

    /// The chapter holds ids, so renaming and moving a note cannot lose its place.
    ///
    /// This is the same property a link has, for the same reason, and it is the
    /// whole argument for `sequence:` being a list of ids rather than a folder's
    /// contents or a list of titles.
    #[test]
    fn renaming_and_moving_a_note_keeps_its_place_in_a_chapter() {
        let vault = TempVault::new();
        let note = vault.create_note("Growth", folder("Drafts")).unwrap();
        let id = note.summary.id.clone();
        let chapter = vault.create_chapter("3. Sb2Se3", None).unwrap();
        vault
            .set_sequence(&chapter.summary.id, vec![id.clone()])
            .unwrap();

        vault
            .save_note(&id, "Growth of the films", "Prose.")
            .unwrap();
        vault.move_note(&id, "Chapter 3").unwrap();

        let entries = vault.chapter(&chapter.summary.id).unwrap();
        assert_eq!(entries.len(), 1);
        let note = entries[0].note.as_ref().expect("still resolves");
        assert_eq!(note.id, id);
        assert_eq!(note.title, "Growth of the films");
        assert_eq!(note.folder, "Chapter 3");
    }

    /// A note deleted out from under a chapter is reported, not dropped.
    ///
    /// The chapter still claims the note belongs there, and only the author can
    /// say whether the fix is to remove the entry or restore the note. A list
    /// that quietly closed the gap would be a list that lies about what the
    /// chapter said — and it would do it at the exact moment somebody is checking
    /// whether their chapter is complete.
    #[test]
    fn a_chapter_reports_a_note_that_is_gone_rather_than_dropping_it() {
        let vault = TempVault::new();
        let a = vault.create_note("Growth", None).unwrap();
        let b = vault.create_note("Optics", None).unwrap();
        let chapter = vault.create_chapter("3. Sb2Se3", None).unwrap();
        vault
            .set_sequence(
                &chapter.summary.id,
                vec![a.summary.id.clone(), b.summary.id.clone()],
            )
            .unwrap();

        vault.delete_note(&a.summary.id).unwrap();

        let entries = vault.chapter(&chapter.summary.id).unwrap();
        assert_eq!(entries.len(), 2, "the position must still be there");
        assert_eq!(entries[0].id, a.summary.id);
        assert!(
            entries[0].note.is_none(),
            "a deleted note must resolve to nothing rather than to another note"
        );
        assert!(entries[1].note.is_some());

        // And the claim survives on disk, so restoring the note restores the
        // chapter without the author having to remember what was in it.
        assert_eq!(
            vault.sequence_of(&chapter.summary.id).unwrap(),
            vec![a.summary.id.clone(), b.summary.id.clone()]
        );
    }

    /// One note may belong to two chapters, and twice to one.
    #[test]
    fn a_note_may_appear_in_two_chapters_and_twice_in_one() {
        let vault = TempVault::new();
        let methods = vault.create_note("Methods", None).unwrap();
        let three = vault.create_chapter("3. Growth", None).unwrap();
        let four = vault.create_chapter("4. Optics", None).unwrap();

        vault
            .set_sequence(
                &three.summary.id,
                vec![methods.summary.id.clone(), methods.summary.id.clone()],
            )
            .unwrap();
        vault
            .set_sequence(&four.summary.id, vec![methods.summary.id.clone()])
            .unwrap();

        assert_eq!(vault.chapter(&three.summary.id).unwrap().len(), 2);
        assert_eq!(vault.chapter(&four.summary.id).unwrap().len(), 1);

        let used = vault.chapters_using(&methods.summary.id).unwrap();
        assert_eq!(used.len(), 2, "both chapters should be reported");
        let titles: Vec<&str> = used.iter().map(|u| u.title.as_str()).collect();
        assert!(titles.contains(&"3. Growth") && titles.contains(&"4. Optics"));
    }

    /// Exporting a chapter is the chapter's own body followed by its notes.
    #[test]
    fn a_chapter_exports_as_its_own_body_then_its_notes_in_order() {
        let vault = TempVault::new();
        let a = vault.create_note("Growth", None).unwrap();
        vault
            .save_note(&a.summary.id, "Growth", "How the films were made.")
            .unwrap();
        let b = vault.create_note("Optics", None).unwrap();
        vault
            .save_note(&b.summary.id, "Optics", "What they absorbed.")
            .unwrap();

        let chapter = vault.create_chapter("3. Sb2Se3", None).unwrap();
        vault
            .save_note(&chapter.summary.id, "3. Sb2Se3", "The opening argument.")
            .unwrap();
        vault
            .set_sequence(
                &chapter.summary.id,
                vec![a.summary.id.clone(), b.summary.id.clone()],
            )
            .unwrap();

        let sections = vault.chapter_sections(&chapter.summary.id).unwrap();
        assert_eq!(sections.len(), 3);

        assert_eq!(sections[0].title, "3. Sb2Se3");
        assert_eq!(sections[0].body.trim(), "The opening argument.");
        assert!(
            !sections[0].heading,
            "the chapter's own title is the document's title, not a heading in it"
        );

        assert_eq!(sections[1].title, "Growth");
        assert!(sections[1].heading);
        assert_eq!(sections[2].title, "Optics");
        assert_eq!(sections[2].body.trim(), "What they absorbed.");
    }

    /// An export cannot contain a hole, so a missing note is skipped there —
    /// which is exactly why `chapter` reports it separately.
    #[test]
    fn exporting_skips_a_missing_note_that_the_chapter_still_reports() {
        let vault = TempVault::new();
        let a = vault.create_note("Growth", None).unwrap();
        let b = vault.create_note("Optics", None).unwrap();
        let chapter = vault.create_chapter("3. Sb2Se3", None).unwrap();
        vault
            .set_sequence(
                &chapter.summary.id,
                vec![a.summary.id.clone(), b.summary.id.clone()],
            )
            .unwrap();
        vault.delete_note(&a.summary.id).unwrap();

        let sections = vault.chapter_sections(&chapter.summary.id).unwrap();
        assert_eq!(
            sections.len(),
            2,
            "the chapter itself plus the one note that is still there"
        );
        assert_eq!(sections[1].title, "Optics");

        assert!(
            vault.chapter(&chapter.summary.id).unwrap()[0]
                .note
                .is_none(),
            "and the missing one is still reported where a person can see it"
        );
    }

    /// A v0.3 note with no `sequence:` is not a chapter and is read unchanged.
    #[test]
    fn a_note_without_a_sequence_is_read_unchanged() {
        let vault = TempVault::new();
        let note = vault.create_note("Ordinary", None).unwrap();
        let path = vault.path_for(&note.summary.id).unwrap();
        let before = fs::read_to_string(&path).unwrap();

        assert!(
            vault.sequence_of(&note.summary.id).unwrap().is_empty(),
            "no sequence is an empty one, not an error"
        );
        assert!(vault.chapter(&note.summary.id).unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            before,
            "reading a chapter's sequence must not rewrite the note"
        );
        assert!(
            !before.contains("sequence"),
            "and nothing writes an empty one"
        );
    }

    /// Setting a sequence on an ordinary note makes it a chapter.
    #[test]
    fn setting_a_sequence_makes_a_note_a_chapter() {
        let vault = TempVault::new();
        let note = vault.create_note("Was ordinary", None).unwrap();
        let member = vault.create_note("A note", None).unwrap();

        let summary = vault
            .set_sequence(&note.summary.id, vec![member.summary.id.clone()])
            .unwrap();
        assert_eq!(summary.note_type, NoteType::Chapter);
        assert_eq!(
            vault.list_chapters().unwrap().len(),
            1,
            "and it is listed as one"
        );
    }

    /// Emptying a chapter's sequence writes no key rather than an empty list.
    #[test]
    fn an_emptied_sequence_leaves_no_key_in_the_file() {
        let vault = TempVault::new();
        let member = vault.create_note("A note", None).unwrap();
        let chapter = vault.create_chapter("3. Sb2Se3", None).unwrap();
        vault
            .set_sequence(&chapter.summary.id, vec![member.summary.id.clone()])
            .unwrap();
        vault.set_sequence(&chapter.summary.id, Vec::new()).unwrap();

        let raw = fs::read_to_string(vault.path_for(&chapter.summary.id).unwrap()).unwrap();
        assert!(
            !raw.contains("sequence"),
            "an empty sequence should be absent, not `sequence: []`: {raw}"
        );
    }

    // ---- v0.3: two files, one id -------------------------------------------

    #[test]
    fn two_files_claiming_one_id_are_reported_not_just_resolved() {
        let vault = TempVault::new();
        let doc = vault.create_note("Growth", None).unwrap();
        let id = doc.summary.id.clone();
        vault.save_note(&id, "Growth", "the version here").unwrap();

        let original = vault.root().join(vault.relative_for(&id).unwrap());
        let copy = vault.root().join("Growth (conflicted copy).md");
        fs::write(&copy, fs::read_to_string(&original).unwrap()).unwrap();

        vault.list_notes().unwrap();
        let clashes = vault.id_clashes();
        assert_eq!(clashes.len(), 1, "the clash was not reported");
        assert_eq!(clashes[0].id, id);
        assert!(clashes[0].opened.ends_with("Growth.md"));
        assert!(clashes[0].shadowed.contains("conflicted copy"));

        // Both files still on disk. Reporting is not deleting.
        assert!(original.exists() && copy.exists());
    }

    #[test]
    fn a_vault_with_no_clashes_reports_none() {
        let vault = TempVault::new();
        vault.create_note("One", None).unwrap();
        vault.create_note("Two", None).unwrap();
        vault.list_notes().unwrap();
        assert!(vault.id_clashes().is_empty());
    }

    // ---- v0.3: filename collisions -----------------------------------------

    #[test]
    fn a_title_differing_only_in_case_gets_its_own_file() {
        // NTFS and APFS treat these as one file; ext4 as two. Whichever this
        // test runs on, the vault must be one that opens on all of them.
        let vault = TempVault::new();
        let first = vault.create_note("Growth", None).unwrap();
        let second = vault.create_note("growth", None).unwrap();

        let a = vault.relative_for(&first.summary.id).unwrap();
        let b = vault.relative_for(&second.summary.id).unwrap();
        assert_ne!(
            a.to_lowercase(),
            b.to_lowercase(),
            "two notes share a filename once case is folded: {a} and {b}"
        );
        // And both still open as themselves.
        assert_eq!(
            vault.read_note(&first.summary.id).unwrap().summary.title,
            "Growth"
        );
        assert_eq!(
            vault.read_note(&second.summary.id).unwrap().summary.title,
            "growth"
        );
    }

    #[test]
    fn a_unicode_title_keeps_its_characters() {
        // A materials vault is full of these. Dropping them would turn
        // "Sb₂Se₃ growth" into "growth" and lose the note in a list.
        let vault = TempVault::new();
        let note = vault.create_note("Sb₂Se₃ growth — α phase", None).unwrap();
        let relative = vault.relative_for(&note.summary.id).unwrap();
        assert!(
            relative.contains("Sb₂Se₃"),
            "characters were stripped: {relative}"
        );
        assert_eq!(
            vault.read_note(&note.summary.id).unwrap().summary.title,
            "Sb₂Se₃ growth — α phase"
        );
    }

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

    #[test]
    fn the_canonical_file_wins_over_every_conflict_convention() {
        use std::cmp::Ordering;

        // Each of these is a real thing a sync client or Explorer writes
        // beside the file Sutra wrote. All of them add characters; none
        // shortens the name.
        for decorated in [
            "Growth (conflicted copy).md",
            "Growth-LAPTOP.md",
            "Growth (1).md",
            "Growth - Copy.md",
            "Growth (DESKTOP-4F2K1 conflicted copy 2026-09-04).md",
        ] {
            assert_eq!(
                canonical_first("Growth.md", decorated),
                Ordering::Less,
                "Growth.md should win over {decorated}"
            );
            // And the answer must not depend on which one was seen first.
            assert_eq!(canonical_first(decorated, "Growth.md"), Ordering::Greater);
        }
    }

    #[test]
    fn a_deep_note_is_not_beaten_by_a_shallow_conflicted_copy() {
        use std::cmp::Ordering;

        // The comparison is on the file name, not the path: a note four
        // folders down is not a worse candidate than a conflicted copy in the
        // vault root.
        assert_eq!(
            canonical_first("a/b/c/d/Growth.md", "Growth (conflicted copy).md"),
            Ordering::Less
        );
    }

    #[test]
    fn two_equally_named_files_are_decided_the_same_way_every_time() {
        use std::cmp::Ordering;

        // Same length, so the filesystem's order could otherwise decide.
        // Lexicographic is arbitrary but stable, which is the whole point.
        assert_eq!(
            canonical_first("b/Growth.md", "a/Growth.md"),
            Ordering::Greater
        );
        assert_eq!(
            canonical_first("a/Growth.md", "b/Growth.md"),
            Ordering::Less
        );
        assert_eq!(
            canonical_first("a/Growth.md", "a/Growth.md"),
            Ordering::Equal
        );
    }
}
