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

/// What Zotero says about one attachment, as far as finding its file needs.
///
/// A copy of the four fields that matter, rather than a borrow of the whole
/// record, so that resolution is a pure function of data and can be tested
/// without a library, a network or a provider.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttachmentRecord {
    pub key: String,
    /// `imported_file`, `imported_url`, `linked_file`, `linked_url` — verbatim.
    pub link_mode: Option<String>,
    pub filename: Option<String>,
    pub path: Option<String>,
}

/// A PDF Zotero owns.
///
/// **Verified for `imported_file`** against a real library: the response
/// carried `linkMode: imported_file`, a `filename`, and **no `path` at all** —
/// which is why `path` is not required for that mode and its absence is not
/// treated as missing metadata.
///
/// `linked_file` is implemented to Zotero's documented semantics and covered by
/// tests, and is **not verified against a real library**. See
/// `docs/architecture/verification.md`; the distinction is kept because a
/// passing fixture says the code matches what we believe, not that what we
/// believe is what Zotero sends.
pub struct ZoteroPdf {
    /// Zotero's data directory. Where `storage/` lives.
    data_dir: PathBuf,
    attachment: AttachmentRecord,
}

impl ZoteroPdf {
    pub fn new(data_dir: impl Into<PathBuf>, attachment: AttachmentRecord) -> Self {
        Self {
            data_dir: data_dir.into(),
            attachment,
        }
    }
}

/// Whether a name Zotero gave is a plain file name and not a path.
///
/// **This is not normalisation.** A filename is used exactly as given — never
/// slugged, re-cased or re-encoded — because it names a file another program
/// owns. What this refuses is a value that is not a filename at all: one
/// carrying a separator, or `..`, which would send a read outside the storage
/// folder. Refusing is the only safe response; rewriting it into something
/// acceptable would be guessing at what the library meant.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

impl PdfLocator for ZoteroPdf {
    fn locate(&self, attachment_key: &str) -> Result<Option<PathBuf>> {
        let record = &self.attachment;
        if record.key != attachment_key {
            return Err(SutraError::Pdf(format!(
                "asked for attachment {attachment_key} but holding {}",
                record.key
            )));
        }

        let Some(mode) = record.link_mode.as_deref() else {
            // Zotero always says how it holds a file. Nothing to resolve, and
            // nothing worth assuming about which mode was meant.
            return Err(SutraError::Pdf(format!(
                "Zotero did not say how it stores attachment {attachment_key}, \
                 so Sutra cannot tell where the file is."
            )));
        };

        match mode {
            // Verified. The file sits in a folder named for the attachment, in
            // the library's own storage. `path` is absent for these and its
            // absence is correct, not missing metadata.
            //
            // `imported_url` — a file Zotero downloaded rather than one that was
            // added from disk — is documented as using the same layout and is
            // resolved the same way. That half is not verified; see
            // docs/architecture/verification.md.
            "imported_file" | "imported_url" => {
                let Some(filename) = record.filename.as_deref() else {
                    return Err(SutraError::Pdf(format!(
                        "Zotero gave no file name for attachment {attachment_key}, \
                         so Sutra cannot tell which file in its storage folder is \
                         the paper."
                    )));
                };
                if !is_plain_name(filename) {
                    return Err(SutraError::Pdf(format!(
                        "Zotero gave {filename:?} as a file name for attachment \
                         {attachment_key}, which is a path rather than a name. \
                         Sutra will not read outside the library's storage folder."
                    )));
                }
                if !is_plain_name(attachment_key) {
                    return Err(SutraError::Pdf(format!(
                        "{attachment_key:?} is not an attachment key."
                    )));
                }
                let path = self
                    .data_dir
                    .join("storage")
                    .join(attachment_key)
                    .join(filename);
                Ok(path.is_file().then_some(path))
            }

            // Documented, tested, and NOT verified against a real library.
            "linked_file" => {
                let Some(raw) = record.path.as_deref() else {
                    return Err(SutraError::Pdf(format!(
                        "Zotero says attachment {attachment_key} is a linked file \
                         but gave no path for it."
                    )));
                };
                // Zotero writes `attachments:name.pdf` when the library has a
                // base directory for linked files, and the real path is that
                // base plus the rest. The base directory is a preference Sutra
                // has not read and has not verified, so this is reported rather
                // than guessed — resolving it against the data directory would
                // be inventing a location.
                if let Some(relative) = raw.strip_prefix("attachments:") {
                    return Err(SutraError::Pdf(format!(
                        "This PDF is a linked file stored relative to Zotero's \
                         linked-attachments base directory ({relative}), which \
                         Sutra does not know. Open it in Zotero."
                    )));
                }
                let path = PathBuf::from(raw);
                if !path.is_absolute() {
                    return Err(SutraError::Pdf(format!(
                        "Zotero gave a relative path ({raw}) for linked attachment \
                         {attachment_key}, and Sutra has nothing to resolve it \
                         against."
                    )));
                }
                Ok(path.is_file().then_some(path))
            }

            // A bookmark. There is no file, and that is not a failure — it is
            // the same answer as a source with no PDF.
            "linked_url" => Ok(None),

            other => Err(SutraError::Pdf(format!(
                "Zotero stores this attachment as {other:?}, which Sutra does not \
                 know how to read. Open it in Zotero."
            ))),
        }
    }

    fn ownership(&self) -> Ownership {
        Ownership::External
    }
}

/// Where Zotero keeps its library on this machine.
///
/// `extensions.zotero.dataDir` in a profile's `prefs.js` when it is set, and
/// `~/Zotero` when it is not — which is Zotero's own default and the case for
/// most installations.
///
/// Reading `prefs.js` is a read of a file Zotero owns, which is why it lives in
/// this module: everything that touches Zotero's directory is here, and
/// `the_read_boundary_contains_no_write` covers it.
pub fn data_dir(home: &Path, app_data: Option<&Path>) -> PathBuf {
    if let Some(app_data) = app_data {
        let profiles = app_data.join("Zotero").join("Zotero").join("profiles");
        if let Ok(entries) = std::fs::read_dir(&profiles) {
            for entry in entries.flatten() {
                let prefs = entry.path().join("prefs.js");
                let Ok(text) = std::fs::read_to_string(&prefs) else {
                    continue;
                };
                if let Some(dir) = data_dir_from_prefs(&text) {
                    return PathBuf::from(dir);
                }
            }
        }
    }
    home.join("Zotero")
}

/// Pull `extensions.zotero.dataDir` out of a `prefs.js`.
///
/// A line-wise scan rather than a JavaScript parse: the file is generated, one
/// `user_pref(...)` per line, and a parser would be a dependency and a second
/// thing to be wrong.
///
/// The value is a JavaScript string literal, so `\\` means one backslash — which
/// matters on the platform this ships to, where every such path has several.
fn data_dir_from_prefs(text: &str) -> Option<String> {
    for line in text.lines() {
        // `continue`, not `?`. A `prefs.js` has hundreds of lines and this one
        // is not the first; returning on the first line that is not it would
        // mean never finding the preference at all.
        let Some(rest) = line
            .trim()
            .strip_prefix("user_pref(\"extensions.zotero.dataDir\",")
        else {
            continue;
        };
        let rest = rest
            .trim()
            .trim_end_matches(';')
            .trim_end_matches(')')
            .trim();
        let Some(quoted) = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')) else {
            continue;
        };
        let unescaped = quoted.replace("\\\\", "\\");
        if !unescaped.trim().is_empty() {
            return Some(unescaped);
        }
    }
    None
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

    // ---- the Zotero resolver ------------------------------------------------
    //
    // `imported_file` is verified against a real library: the response carried
    // linkMode `imported_file`, a filename, and no `path` at all. Everything
    // else below is implemented to Zotero's documented semantics and covered
    // here, and is NOT verified against real data — see
    // docs/architecture/verification.md, which is the record of that
    // distinction and must be updated with it rather than instead of it.

    struct Library {
        root: std::path::PathBuf,
    }

    impl Library {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("sutra-zot-{}", ulid::Ulid::generate()));
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        /// Put a file where an imported attachment's file really sits.
        fn imported(&self, key: &str, filename: &str) -> std::path::PathBuf {
            let folder = self.root.join("storage").join(key);
            std::fs::create_dir_all(&folder).unwrap();
            let path = folder.join(filename);
            std::fs::write(&path, b"%PDF-1.4\n").unwrap();
            path
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    fn imported_record(key: &str, filename: &str) -> AttachmentRecord {
        AttachmentRecord {
            key: key.to_string(),
            link_mode: Some("imported_file".to_string()),
            filename: Some(filename.to_string()),
            // The verified response carried no `path`, and its absence is
            // correct rather than missing metadata.
            path: None,
        }
    }

    /// The shape verified against the real library, resolved.
    #[test]
    fn an_imported_file_resolves_to_storage_key_filename() {
        let library = Library::new();
        let expected = library.imported("J938YE6Z", "Zhou et al. - 2019 - Sb2Se3.pdf");

        let locator = ZoteroPdf::new(
            &library.root,
            imported_record("J938YE6Z", "Zhou et al. - 2019 - Sb2Se3.pdf"),
        );
        assert_eq!(locator.locate("J938YE6Z").unwrap(), Some(expected));
        assert_eq!(locator.ownership(), Ownership::External);
    }

    /// The filename is used exactly as Zotero gave it. A console that renders
    /// α as `Î±` is a code page, not a file name, and nothing here may "fix"
    /// one into the other.
    #[test]
    fn a_filename_is_used_exactly_as_given() {
        let library = Library::new();
        let name = "Sb₂Se₃ α-phase β-transition (2019).pdf";
        let expected = library.imported("ABCD1234", name);

        let found = ZoteroPdf::new(&library.root, imported_record("ABCD1234", name))
            .locate("ABCD1234")
            .unwrap()
            .expect("the file is there");

        assert_eq!(found, expected);
        assert_eq!(
            found.file_name().unwrap().to_string_lossy(),
            name,
            "the name was altered on the way through"
        );
    }

    /// Documented, tested, NOT verified against the real library.
    #[test]
    fn a_linked_file_resolves_to_its_absolute_path() {
        let library = Library::new();
        let elsewhere = library.root.join("somewhere-else.pdf");
        std::fs::write(&elsewhere, b"%PDF-1.4\n").unwrap();

        let locator = ZoteroPdf::new(
            &library.root,
            AttachmentRecord {
                key: "LINK0001".to_string(),
                link_mode: Some("linked_file".to_string()),
                filename: None,
                path: Some(elsewhere.to_string_lossy().to_string()),
            },
        );
        assert_eq!(locator.locate("LINK0001").unwrap(), Some(elsewhere));
    }

    /// A linked file kept relative to Zotero's base directory. Sutra has not
    /// read that preference and does not guess at it — it says so instead.
    #[test]
    fn a_linked_file_under_the_base_directory_is_reported_not_guessed() {
        let library = Library::new();
        let locator = ZoteroPdf::new(
            &library.root,
            AttachmentRecord {
                key: "LINK0002".to_string(),
                link_mode: Some("linked_file".to_string()),
                filename: None,
                path: Some("attachments:papers/zhou.pdf".to_string()),
            },
        );
        let said = locator.locate("LINK0002").unwrap_err().to_string();
        assert!(said.contains("base directory"), "{said}");
        // And it must not have invented a location under the data directory.
        assert!(!said.contains("storage"), "{said}");
    }

    /// The file is gone, moved, or never synced. `None`, not an error: the same
    /// answer as a source with no PDF, which the caller turns into a named
    /// state.
    #[test]
    fn a_missing_file_is_absent_rather_than_an_error() {
        let library = Library::new();
        let locator = ZoteroPdf::new(
            &library.root,
            imported_record("GONE0001", "never-downloaded.pdf"),
        );
        assert_eq!(locator.locate("GONE0001").unwrap(), None);
    }

    #[test]
    fn a_link_mode_sutra_cannot_read_is_named() {
        let library = Library::new();
        for mode in ["imported_directory", "something_new_in_zotero_8"] {
            let locator = ZoteroPdf::new(
                &library.root,
                AttachmentRecord {
                    key: "MODE0001".to_string(),
                    link_mode: Some(mode.to_string()),
                    filename: Some("x.pdf".to_string()),
                    path: None,
                },
            );
            let said = locator.locate("MODE0001").unwrap_err().to_string();
            assert!(said.contains(mode), "the mode is not named: {said}");
            assert!(said.contains("Open it in Zotero"), "{said}");
        }
    }

    /// A bookmark has no file. Not a failure — the same answer as no PDF.
    #[test]
    fn a_linked_url_has_no_file_and_that_is_not_a_failure() {
        let library = Library::new();
        let locator = ZoteroPdf::new(
            &library.root,
            AttachmentRecord {
                key: "URL00001".to_string(),
                link_mode: Some("linked_url".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(locator.locate("URL00001").unwrap(), None);
    }

    #[test]
    fn attachment_metadata_that_cannot_be_used_is_named() {
        let library = Library::new();

        // No link mode at all.
        let bare = ZoteroPdf::new(
            &library.root,
            AttachmentRecord {
                key: "BARE0001".to_string(),
                ..Default::default()
            },
        );
        assert!(
            bare.locate("BARE0001")
                .unwrap_err()
                .to_string()
                .contains("did not say how it stores")
        );

        // Imported, but no filename — so there is no way to tell which file in
        // the folder is the paper.
        let nameless = ZoteroPdf::new(
            &library.root,
            AttachmentRecord {
                key: "NAME0001".to_string(),
                link_mode: Some("imported_file".to_string()),
                filename: None,
                path: None,
            },
        );
        assert!(
            nameless
                .locate("NAME0001")
                .unwrap_err()
                .to_string()
                .contains("no file name")
        );

        // Linked, but no path.
        let pathless = ZoteroPdf::new(
            &library.root,
            AttachmentRecord {
                key: "PATH0001".to_string(),
                link_mode: Some("linked_file".to_string()),
                filename: None,
                path: None,
            },
        );
        assert!(
            pathless
                .locate("PATH0001")
                .unwrap_err()
                .to_string()
                .contains("gave no path")
        );
    }

    /// A filename carrying a separator is not a filename. Refused rather than
    /// rewritten: rewriting would be guessing at what the library meant, and
    /// the consequence of being wrong is reading a file outside the library.
    #[test]
    fn a_filename_that_is_really_a_path_is_refused() {
        let library = Library::new();
        for bad in ["../../secrets.pdf", "sub/dir/paper.pdf", "..", ""] {
            let locator = ZoteroPdf::new(&library.root, imported_record("ESC00001", bad));
            assert!(
                locator.locate("ESC00001").is_err(),
                "{bad:?} was not refused"
            );
        }
    }

    #[test]
    fn the_data_directory_comes_from_the_profile_when_it_is_set() {
        let library = Library::new();
        let app_data = library.root.join("AppData");
        let profile = app_data
            .join("Zotero")
            .join("Zotero")
            .join("profiles")
            .join("abc123.default");
        std::fs::create_dir_all(&profile).unwrap();
        // As Zotero writes it: a JavaScript string, so every backslash is
        // doubled — which is every path on the platform this ships to.
        std::fs::write(
            profile.join("prefs.js"),
            "// Zotero prefs\n\
             user_pref(\"extensions.zotero.automaticScreenshot\", false);\n\
             user_pref(\"extensions.zotero.dataDir\", \"D:\\\\Research\\\\Zotero\");\n\
             user_pref(\"extensions.zotero.firstRun2\", false);\n",
        )
        .unwrap();

        assert_eq!(
            data_dir(&library.root, Some(&app_data)),
            std::path::PathBuf::from("D:\\Research\\Zotero"),
            "the preference was not read, or the escaping was not undone"
        );
    }

    #[test]
    fn the_data_directory_falls_back_to_zoteros_own_default() {
        let library = Library::new();
        // No profile at all, and a profile that does not set the preference.
        assert_eq!(data_dir(&library.root, None), library.root.join("Zotero"));

        let app_data = library.root.join("Empty");
        let profile = app_data
            .join("Zotero")
            .join("Zotero")
            .join("profiles")
            .join("x.default");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("prefs.js"), "user_pref(\"other.thing\", 1);\n").unwrap();
        assert_eq!(
            data_dir(&library.root, Some(&app_data)),
            library.root.join("Zotero")
        );
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
