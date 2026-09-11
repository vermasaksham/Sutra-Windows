//! The one place Sutra touches a PDF it does not own.
//!
//! `docs/decisions/0003-pdf-ownership-and-access.md` draws the line this module
//! is: **ownership is not access.** A Zotero-managed PDF stays Zotero's while
//! Sutra reads it and stays Zotero's afterwards. Reading bytes creates no second
//! copy and answers no question about which file is authoritative.
//!
//! So this module reads. It does not write, rename, move, remove, copy or
//! create anything, anywhere, ever — and that is enforced rather than intended:
//! `the_read_boundary_contains_no_write` reads this file's own source and fails
//! on any call that could modify a file. The ADR asks for exactly that, for the
//! reason it gives: the edit that breaks this will be made in good faith by
//! someone who did not know, and a person cannot be relied on to notice.
//!
//! Cache writes live in `pdfcache`, and vault writes in `vault`. Neither
//! belongs here.

use crate::error::{Result, SutraError};
use std::path::{Path, PathBuf};

/// Where a PDF might be found, without saying how to find it.
///
/// Two kinds of PDF reach the same extraction code: one the researcher attached
/// into their own vault, which Sutra owns, and one Zotero owns and Sutra may
/// only read. The roadmap asks for a single extraction path whose only
/// difference is who owns the file — so ownership is the thing this trait
/// abstracts, and everything downstream of `locate` is identical for both.
pub trait PdfLocator {
    /// The path to read, or `None` when there is no such PDF to read.
    ///
    /// `None` is not a failure: a source with no PDF attached is an ordinary,
    /// common state. A failure is when a PDF should be there and something
    /// went wrong finding it, and that is an `Err`.
    fn locate(&self, id: &str) -> Result<Option<PathBuf>>;

    /// Who owns what this locator finds. Recorded with extracted text so a
    /// later reader can tell derived-from-ours from derived-from-theirs.
    fn ownership(&self) -> Ownership;
}

/// Who owns a PDF. Not a permission check — a statement of fact used in
/// provenance and in what the interface is allowed to offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ownership {
    /// Under the vault's `.attachments/`. Sutra's, and the researcher's.
    Vault,
    /// Zotero's, or any other program's. Readable, never writable.
    External,
}

/// A PDF the researcher attached into their own vault.
///
/// The implemented half. `id` is the attachment's path relative to the vault
/// root, exactly as `attachments` records it.
pub struct VaultPdf {
    root: PathBuf,
}

impl VaultPdf {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl PdfLocator for VaultPdf {
    fn locate(&self, id: &str) -> Result<Option<PathBuf>> {
        // A relative path from the vault's own records, so it must stay inside
        // the vault. `..` in it would be a bug elsewhere, but the consequence
        // here would be Sutra reading an arbitrary file off the disk, so it is
        // refused rather than trusted.
        if id
            .split(['/', '\\'])
            .any(|part| part == ".." || part.is_empty())
        {
            return Err(SutraError::Pdf(format!(
                "{id} is not a path inside the vault"
            )));
        }
        let path = self.root.join(id);
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(path))
    }

    fn ownership(&self) -> Ownership {
        Ownership::Vault
    }
}

/// A PDF Zotero owns.
///
/// **PENDING REAL-ZOTERO VERIFICATION.** Deliberately unimplemented, and this
/// is the whole of what is blocked.
///
/// Resolving one means turning an attachment record into a path, and the
/// fields that would do it — `filename`, `linkMode`, `path` — have not been
/// observed in a response from a real Zotero. They are documented, and the
/// documentation is probably right; "probably right" is not a basis for code
/// that reads files off somebody's disk. An `imported_file` attachment lives at
/// `<dataDir>/storage/<key>/<filename>` and needs `filename`; a `linked_file`
/// is anywhere at all and needs `path`. Guessing wrong does not fail loudly —
/// it silently reads the wrong file, or none, and the researcher is told their
/// paper has no text layer.
///
/// So this returns a failure that says exactly that, and every caller already
/// handles it, because "the PDF is not available and here is why" is a state
/// the whole feature is built to degrade into. When the shape is verified, the
/// resolution goes here and nothing else in this file changes.
pub struct ZoteroPdf {
    /// Zotero's data directory, when it is known. Unused until resolution is
    /// written; carried now so the constructor does not change shape later.
    #[allow(dead_code)]
    data_dir: Option<PathBuf>,
}

impl ZoteroPdf {
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        Self { data_dir }
    }
}

impl PdfLocator for ZoteroPdf {
    fn locate(&self, _attachment_key: &str) -> Result<Option<PathBuf>> {
        Err(SutraError::Pdf(
            "Sutra cannot yet find Zotero's copy of this PDF on disk. Reading a \
             Zotero-managed file needs the attachment's filename and link mode, \
             and that part of Zotero's response has not been verified against a \
             real library yet — so it is not guessed at. Everything else works: \
             annotations import, and a PDF attached to the note itself can be \
             read."
                .to_string(),
        ))
    }

    fn ownership(&self) -> Ownership {
        Ownership::External
    }
}

/// What a PDF was when it was read, for telling whether a cache entry is stale.
///
/// Size and modification time rather than a hash of the contents: a thesis's
/// worth of PDFs is gigabytes, hashing all of it on every launch would be the
/// slowest thing the app does, and the question being answered is only "has
/// this file changed since we extracted it". A file edited in place without its
/// length or mtime moving would defeat it; that is a trade made knowingly, and
/// the cost of being wrong is one stale extraction, never a lost note, because
/// extracted text is disposable by construction.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fingerprint {
    pub bytes: u64,
    /// Seconds since the Unix epoch. `None` when the filesystem would not say,
    /// which is a real answer on some network shares — and a fingerprint that
    /// cannot see the time still has the length, so it is weaker rather than
    /// useless.
    pub modified: Option<i64>,
}

/// Read a file's identity without reading the file.
pub fn fingerprint(path: &Path) -> Result<Fingerprint> {
    let meta = std::fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    Ok(Fingerprint {
        bytes: meta.len(),
        modified,
    })
}

/// The largest PDF that will be read into memory.
///
/// Extraction needs the bytes, and a malformed or hostile file claiming to be a
/// paper should not be able to exhaust memory. Papers are single-digit
/// megabytes; a scanned thesis can be a hundred. Beyond this it is refused by
/// name rather than attempted.
pub const MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Read a PDF's bytes, and nothing else.
///
/// The only filesystem operation in this module that touches the file itself.
/// It opens for reading — `std::fs::read` cannot create, truncate or modify —
/// and the size is checked before the read rather than after, so an enormous
/// file is declined instead of loaded and then rejected.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(SutraError::Pdf(format!("{} is not a file", path.display())));
    }
    if meta.len() > MAX_BYTES {
        return Err(SutraError::Pdf(format!(
            "{} is {} MB, larger than the {} MB Sutra will read",
            path.display(),
            meta.len() / (1024 * 1024),
            MAX_BYTES / (1024 * 1024)
        )));
    }
    Ok(std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard `docs/decisions/0003-pdf-ownership-and-access.md` asks for, in
    /// the terms it asks for them:
    ///
    /// > Read access lives in one module whose only filesystem operation
    /// > against Zotero's directory is a read, and a test reads that module's
    /// > own source and fails if it contains a write, rename, or remove call.
    ///
    /// This is that test, and it is not decoration. The rule it protects —
    /// Sutra never modifies a file it does not own — cannot be checked by
    /// running the app, because the failure is a thing that does not happen
    /// until the day it does, to somebody's library. The realistic way it
    /// breaks is a future edit made in good faith: someone adds a cache write
    /// here because this is where the file is already open, and the ownership
    /// boundary is gone with no test failing. So the source itself is the
    /// thing asserted on.
    ///
    /// If you are reading this because it just failed: the fix is not to add
    /// your call to the list. It is to put the write somewhere else —
    /// `pdfcache` for derived text, `vault` for anything of the researcher's.
    #[test]
    fn the_read_boundary_contains_no_write() {
        // The production half only. The tests below legitimately create and
        // truncate files — that is how the size limit is exercised — and they
        // are not the boundary; scanning them would make this guard fail on
        // itself and teach the next person to weaken it.
        let whole = include_str!("pdfread.rs");
        let source = whole
            .split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .unwrap_or(whole);

        // Everything in `std::fs` that can create, change or destroy a file,
        // plus the obvious ways to open one for writing. Matched with their
        // `fs::`/`File::` prefixes so a bare `write` in prose or a doc comment
        // is not a false alarm.
        const FORBIDDEN: &[&str] = &[
            "fs::write",
            "fs::create",
            "fs::remove",
            "fs::rename",
            "fs::copy",
            "fs::hard_link",
            "fs::soft_link",
            "fs::set_permissions",
            "fs::OpenOptions",
            "File::create",
            "OpenOptions::new",
            "set_len",
            "write_all",
        ];

        let offending: Vec<&str> = FORBIDDEN
            .iter()
            .copied()
            .filter(|needle| source.contains(needle))
            .collect();

        assert!(
            offending.is_empty(),
            "pdfread.rs is the module that may only read. It now contains: {offending:?}. \
             Put the write in pdfcache (derived text) or vault (the researcher's own files)."
        );
    }

    #[test]
    fn a_vault_pdf_resolves_under_the_vault() {
        let root = std::env::temp_dir().join(format!("sutra-pdf-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(root.join(".attachments")).unwrap();
        let pdf = root.join(".attachments").join("paper.pdf");
        std::fs::write(&pdf, b"%PDF-1.4\n").unwrap();

        let locator = VaultPdf::new(&root);
        assert_eq!(
            locator.locate(".attachments/paper.pdf").unwrap(),
            Some(pdf.clone())
        );
        assert_eq!(locator.ownership(), Ownership::Vault);

        // Absent is `None`, not an error: a source with no PDF is ordinary.
        assert_eq!(locator.locate(".attachments/missing.pdf").unwrap(), None);

        std::fs::remove_dir_all(&root).ok();
    }

    /// A relative path is supposed to come from the vault's own records, but
    /// the consequence of that assumption being wrong is Sutra reading an
    /// arbitrary file off the disk, so it is checked rather than trusted.
    #[test]
    fn a_vault_pdf_cannot_climb_out_of_the_vault() {
        let locator = VaultPdf::new("/some/vault");
        for escape in [
            "../../../etc/passwd",
            ".attachments/../../secrets.pdf",
            "..\\..\\windows\\system32\\config",
        ] {
            assert!(locator.locate(escape).is_err(), "{escape} was not refused");
        }
    }

    /// The pending half, asserted as pending. This test exists so that
    /// implementing resolution *has* to come back here and say so, rather than
    /// the interface quietly changing behaviour with nothing recording that the
    /// verification the ADR demanded ever happened.
    #[test]
    fn a_zotero_pdf_is_not_resolved_yet_and_says_why() {
        let locator = ZoteroPdf::new(None);
        let err = locator.locate("ABCD1234").unwrap_err();
        let said = err.to_string();
        assert!(
            said.contains("not been verified"),
            "the failure must say the shape is unverified, not just fail: {said}"
        );
        // And it must not read as "you have no PDF", which would send someone
        // looking for a problem in their Zotero library.
        assert!(!said.contains("no PDF attached"), "misleading: {said}");
        assert_eq!(locator.ownership(), Ownership::External);
    }

    #[test]
    fn a_fingerprint_changes_when_the_file_does() {
        let path = std::env::temp_dir().join(format!("sutra-fp-{}.pdf", ulid::Ulid::generate()));
        std::fs::write(&path, b"one").unwrap();
        let first = fingerprint(&path).unwrap();

        std::fs::write(&path, b"one and a half").unwrap();
        let second = fingerprint(&path).unwrap();

        assert_ne!(first, second, "a longer file must not fingerprint the same");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_enormous_file_is_declined_by_name_rather_than_read() {
        // Not by writing half a gigabyte: the check reads metadata, so a sparse
        // file of the right length exercises it at no cost.
        let path = std::env::temp_dir().join(format!("sutra-big-{}.pdf", ulid::Ulid::generate()));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        drop(file);

        let err = read_bytes(&path).unwrap_err().to_string();
        assert!(err.contains("larger than"), "{err}");
        std::fs::remove_file(&path).ok();
    }
}
