//! Extracted text, kept so it is not extracted twice, and disposable.
//!
//! Three constraints from `docs/architecture/invariants.md` and the v0.4
//! roadmap decide everything here, and they decide it jointly:
//!
//! **It is not research, so it is not in the vault.** Extracted text is derived
//! from a file Sutra does not own. Writing it into the vault would put bytes
//! the researcher did not write, and cannot correct, in the folder that is
//! supposed to be theirs — and it would put a copy of a Zotero PDF's contents
//! inside Sutra's boundary, which `docs/decisions/0003-pdf-ownership-and-access.md`
//! forbids in spirit even where the file itself is not copied.
//!
//! **It is not "already in a note", so it is not in the notes index.** That
//! index is rebuildable from the markdown by definition. Text from a PDF is
//! not in any markdown, so putting it there would make the index hold something
//! that could not be reconstructed — and the one architectural rule this app
//! has is that it can always be thrown away.
//!
//! **So: its own directory, in app data, outside both.** Deleting the whole
//! thing costs one re-extraction and loses nothing. That is the test of whether
//! a cache is really disposable, and this one passes it.
//!
//! Invalidation is by fingerprint rather than by age. A cache entry records
//! what the PDF was when it was read; if the file on disk no longer matches,
//! the entry is ignored. Re-annotating a paper in Zotero rewrites the file, so
//! this is a case that happens in ordinary use rather than a theoretical one.

use crate::error::Result;
use crate::pdfread::Fingerprint;
use crate::pdftext::Extraction;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// One cached extraction, with what the file was when it was made.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    /// Bumped when the shape below changes, so an old entry is discarded
    /// rather than misread. Cheaper than a migration, and correct, because
    /// nothing here is worth migrating.
    version: u32,
    fingerprint: Fingerprint,
    extraction: Extraction,
}

const VERSION: u32 = 1;

/// Where extracted text lives.
///
/// Given the app-data directory rather than finding it, so a test can point it
/// at a temporary folder and so this module has no opinion about the platform.
pub struct TextCache {
    root: PathBuf,
}

impl TextCache {
    pub fn new(app_data: impl Into<PathBuf>) -> Self {
        Self {
            root: app_data.into().join("pdf-text"),
        }
    }

    /// The file an extraction of this path would be cached in.
    ///
    /// Named by a hash of the path rather than by the path itself: a filename
    /// derived from `C:\Users\...\Some Paper (2024) v2.pdf` would be
    /// unwritable on one platform or another, and the name of a cache file is
    /// not information anybody needs. The hash need not be stable across
    /// releases — if it changes, every entry is missed once and rebuilt.
    fn entry_path(&self, pdf: &Path) -> PathBuf {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        pdf.hash(&mut hasher);
        self.root.join(format!("{:016x}.json", hasher.finish()))
    }

    /// The cached extraction of this PDF, if there is one and it is still true.
    ///
    /// Never an error. Every way this can fail — no entry, unreadable entry,
    /// an entry from an older shape, a file that has changed since — has the
    /// same correct answer, which is "extract it again". A cache that can
    /// return an error is a cache that can break the thing it was meant to
    /// make faster.
    pub fn get(&self, pdf: &Path, now: &Fingerprint) -> Option<Extraction> {
        let raw = std::fs::read(self.entry_path(pdf)).ok()?;
        let entry: Entry = serde_json::from_slice(&raw).ok()?;
        if entry.version != VERSION {
            return None;
        }
        if &entry.fingerprint != now {
            return None;
        }
        Some(entry.extraction)
    }

    /// Remember an extraction against what the file was when it was made.
    ///
    /// Returns `Ok(())` having done nothing if the write fails for a reason
    /// that is not the caller's business — a full disk, a locked directory. A
    /// cache that cannot be written is a slower app, not a broken one, and
    /// failing the extraction the user asked for because the *cache* failed
    /// would be exactly backwards.
    pub fn put(&self, pdf: &Path, fingerprint: Fingerprint, extraction: &Extraction) -> Result<()> {
        let entry = Entry {
            version: VERSION,
            fingerprint,
            extraction: extraction.clone(),
        };
        let Ok(json) = serde_json::to_vec(&entry) else {
            return Ok(());
        };
        if std::fs::create_dir_all(&self.root).is_err() {
            return Ok(());
        }
        // Written through a temporary file in the same directory and renamed,
        // the same way notes are: an interrupted write leaves the old entry or
        // no entry, never half of one that parses as far as the fingerprint and
        // then stops.
        let final_path = self.entry_path(pdf);
        let temp = final_path.with_extension("json.part");
        if std::fs::write(&temp, &json).is_err() {
            return Ok(());
        }
        if std::fs::rename(&temp, &final_path).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        Ok(())
    }

    /// Throw the whole cache away.
    ///
    /// Exists because the claim "this is disposable" should be something the
    /// app can actually do, not only something the documentation says.
    pub fn clear(&self) -> Result<()> {
        match std::fs::remove_dir_all(&self.root) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdftext::Page;

    struct Scratch {
        dir: PathBuf,
    }

    impl Scratch {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("sutra-cache-{}", ulid::Ulid::generate()));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }
        fn cache(&self) -> TextCache {
            TextCache::new(&self.dir)
        }
        /// A PDF on disk to fingerprint. Its contents do not matter here — the
        /// cache never reads them, only their length and time.
        fn pdf(&self, contents: &[u8]) -> PathBuf {
            let path = self.dir.join("paper.pdf");
            std::fs::write(&path, contents).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn extraction(text: &str) -> Extraction {
        Extraction {
            pages: vec![Page {
                number: 1,
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn what_went_in_comes_back_out() {
        let scratch = Scratch::new();
        let cache = scratch.cache();
        let pdf = scratch.pdf(b"one");
        let print = crate::pdfread::fingerprint(&pdf).unwrap();

        assert!(cache.get(&pdf, &print).is_none(), "nothing cached yet");
        cache
            .put(&pdf, print.clone(), &extraction("Sb2Se3"))
            .unwrap();
        assert_eq!(cache.get(&pdf, &print), Some(extraction("Sb2Se3")));
    }

    /// The case that makes fingerprinting worth having rather than caching by
    /// path alone: annotating a paper in Zotero rewrites the file, and the old
    /// text is then wrong about a document somebody is quoting.
    #[test]
    fn a_changed_pdf_invalidates_what_was_cached_for_it() {
        let scratch = Scratch::new();
        let cache = scratch.cache();
        let pdf = scratch.pdf(b"one");
        let before = crate::pdfread::fingerprint(&pdf).unwrap();
        cache
            .put(&pdf, before.clone(), &extraction("old text"))
            .unwrap();

        // Rewritten longer, so the length differs whatever the clock did.
        std::fs::write(&pdf, b"one, revised and extended").unwrap();
        let after = crate::pdfread::fingerprint(&pdf).unwrap();

        assert!(
            cache.get(&pdf, &after).is_none(),
            "stale text was served for a file that has changed"
        );
        // And the old fingerprint still matches its own entry, so the miss is
        // the file having moved on rather than the entry having been lost.
        assert_eq!(cache.get(&pdf, &before), Some(extraction("old text")));
    }

    #[test]
    fn two_pdfs_do_not_share_an_entry() {
        let scratch = Scratch::new();
        let cache = scratch.cache();
        let first = scratch.dir.join("a.pdf");
        let second = scratch.dir.join("b.pdf");
        std::fs::write(&first, b"aaa").unwrap();
        std::fs::write(&second, b"bbb").unwrap();
        let fa = crate::pdfread::fingerprint(&first).unwrap();
        let fb = crate::pdfread::fingerprint(&second).unwrap();

        cache
            .put(&first, fa.clone(), &extraction("first paper"))
            .unwrap();
        assert!(cache.get(&second, &fb).is_none());
        cache
            .put(&second, fb.clone(), &extraction("second paper"))
            .unwrap();
        assert_eq!(cache.get(&first, &fa), Some(extraction("first paper")));
        assert_eq!(cache.get(&second, &fb), Some(extraction("second paper")));
    }

    /// The disposability claim, as something the code can do rather than
    /// something the module comment asserts.
    #[test]
    fn the_whole_cache_can_be_thrown_away() {
        let scratch = Scratch::new();
        let cache = scratch.cache();
        let pdf = scratch.pdf(b"one");
        let print = crate::pdfread::fingerprint(&pdf).unwrap();
        cache.put(&pdf, print.clone(), &extraction("text")).unwrap();
        assert!(cache.get(&pdf, &print).is_some());

        cache.clear().unwrap();
        assert!(cache.get(&pdf, &print).is_none());
        // And clearing an already-empty cache is not an error, because "make
        // sure there is nothing there" is a reasonable thing to ask twice.
        cache.clear().unwrap();
    }

    /// A cache that can fail the operation it was meant to speed up is worse
    /// than no cache. Damaged entries are misses.
    #[test]
    fn a_corrupt_entry_is_a_miss_rather_than_an_error() {
        let scratch = Scratch::new();
        let cache = scratch.cache();
        let pdf = scratch.pdf(b"one");
        let print = crate::pdfread::fingerprint(&pdf).unwrap();
        cache.put(&pdf, print.clone(), &extraction("text")).unwrap();

        // Half-written, truncated by a full disk, corrupted by a crash.
        let entry = cache.entry_path(&pdf);
        std::fs::write(&entry, b"{\"version\":1,\"fingerp").unwrap();
        assert!(cache.get(&pdf, &print).is_none());

        // And it recovers: writing over it works.
        cache
            .put(&pdf, print.clone(), &extraction("again"))
            .unwrap();
        assert_eq!(cache.get(&pdf, &print), Some(extraction("again")));
    }

    /// Entries from an older shape are discarded rather than misread. Nothing
    /// here is worth a migration.
    #[test]
    fn an_entry_from_an_older_version_is_ignored() {
        let scratch = Scratch::new();
        let cache = scratch.cache();
        let pdf = scratch.pdf(b"one");
        let print = crate::pdfread::fingerprint(&pdf).unwrap();

        let stale = Entry {
            version: VERSION - 1,
            fingerprint: print.clone(),
            extraction: extraction("from an older build"),
        };
        std::fs::create_dir_all(&cache.root).unwrap();
        std::fs::write(cache.entry_path(&pdf), serde_json::to_vec(&stale).unwrap()).unwrap();

        assert!(cache.get(&pdf, &print).is_none());
    }

    /// The invariant that decides where this lives: nothing the cache writes
    /// may land in the vault.
    #[test]
    fn nothing_is_written_inside_the_vault() {
        let scratch = Scratch::new();
        let vault = scratch.dir.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let pdf = vault.join(".attachments").join("paper.pdf");
        std::fs::create_dir_all(pdf.parent().unwrap()).unwrap();
        std::fs::write(&pdf, b"a paper inside the vault").unwrap();
        let print = crate::pdfread::fingerprint(&pdf).unwrap();

        // App data deliberately elsewhere, as it is in the real app.
        let cache = TextCache::new(scratch.dir.join("appdata"));
        cache.put(&pdf, print, &extraction("extracted")).unwrap();

        let inside: Vec<_> = walk(&vault).into_iter().collect();
        assert_eq!(
            inside,
            vec![pdf],
            "the cache put something in the vault: {inside:?}"
        );
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return found;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path);
            }
        }
        found.sort();
        found
    }
}
