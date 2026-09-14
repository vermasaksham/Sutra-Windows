//! Turning a PDF into text that knows which page it came from.
//!
//! **Why a child process.** Extraction is the one place Sutra runs a large
//! third-party parser over a file it did not produce, whose structure it cannot
//! validate first, and which frequently was not produced correctly either —
//! publishers' PDFs are full of malformed cross-reference tables and fonts that
//! claim encodings they do not have. `pdf-extract` panics on some of them. The
//! release profile sets `panic = "abort"`, so a panic anywhere in this process
//! takes the whole application down, and a researcher loses the paragraph they
//! were in the middle of writing because they clicked on a bad paper.
//!
//! `catch_unwind` does not help: `abort` means there is no unwinding to catch.
//! Moving extraction into a child process does, completely. The child is this
//! same binary re-invoked with a hidden argument — no second executable to
//! ship, sign or keep in step — and if it dies, it dies alone, and the parent
//! reports a failure with a name.
//!
//! **Why page-aware.** A quotation without a page is not evidence. The library
//! offers whole-document and per-page extraction; only the second can answer
//! "which page did this sentence come from", so only the second is used, even
//! though joining pages afterwards would be cheaper.

use crate::error::{Result, SutraError};
use crate::pdfread;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};

/// What the child's exit code means.
///
/// The parent cannot see the child's error value, only its status, so the
/// status is the vocabulary. Everything except `ENCRYPTED` collapses into one
/// report — "could not read it, here is what it said" — because the researcher's
/// next step is the same for all of them. A password-protected file is separated
/// out because its next step is different and specific: the file is fine, Sutra
/// is not being given the password, and Zotero's reader can open it.
mod exit {
    pub const NO_PATH: i32 = 2;
    pub const UNREADABLE: i32 = 3;
    pub const PARSER: i32 = 4;
    pub const OUTPUT: i32 = 5;
    /// Encrypted, and the empty password did not open it.
    pub const ENCRYPTED: i32 = 6;
}

/// Whether a parser failure was "this file is locked".
///
/// Matched on the error's debug form rather than its type: `lopdf::Error` is not
/// a direct dependency, and taking one on so a message can be more specific
/// would be the tail wagging the dog. The match is narrow, and **the fallback is
/// safe** — an encrypted file this fails to recognise is reported as an ordinary
/// parser failure, which is exactly what it was reported as before this existed.
/// No file is ever wrongly called locked, because nothing else in the parser
/// produces a decryption error.
fn is_encrypted(error: &pdf_extract::OutputError) -> bool {
    let debug = format!("{error:?}");
    debug.contains("Decryption") || debug.contains("IncorrectPassword")
}

/// The hidden first argument that means "you are the extraction child".
///
/// Not a documented command-line interface and not intended as one: it exists
/// so the parent can re-invoke itself. It is prefixed and unlikely to collide
/// with a path a file association might hand the app on Windows.
pub const CHILD_FLAG: &str = "--sutra-extract-pdf";

/// One page's text, with the page it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// 1-based, as a reader counts pages and as `annotationPageLabel` reports
    /// them. The library returns them in order; this is the index in that
    /// order, which is the page's position in the file and not necessarily the
    /// number printed on it — a paper starting at page 431 of a volume has a
    /// first page of 1 here. The printed number, where a page carries one, is
    /// Zotero's business and arrives with an annotation.
    pub number: usize,
    pub text: String,
}

/// Everything extraction produced from one PDF.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extraction {
    pub pages: Vec<Page>,
}

impl Extraction {
    /// Whether the file had any text at all.
    ///
    /// A scanned paper is a sequence of pictures: extraction succeeds, every
    /// page is empty, and there is nothing to quote. That is a real and common
    /// state, distinct from a failure, and it is the one OCR would address —
    /// which v0.4 does not do. Naming it is what stops it reading as a bug.
    pub fn has_text(&self) -> bool {
        self.pages.iter().any(|p| !p.text.trim().is_empty())
    }
}

/// How long the child gets before it is assumed hung.
///
/// A pathological PDF can send the parser into something that does not finish.
/// Two minutes is far past any real paper on any real machine, and a bounded
/// wait is what makes "extraction failed" a state the app reaches rather than
/// one it hangs in.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// How often to look in on the child while waiting.
const POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// Extract text from a PDF, in a child process, page by page.
///
/// Every outcome is one of: text, "no text layer" (`Extraction::has_text`), or
/// an `Err` naming what happened. There is no path that returns silence.
pub fn extract(path: &Path) -> Result<Extraction> {
    // Checked here rather than only in the child so the common failure — the
    // file is not there — is reported without spawning anything, and reads the
    // same whether or not a process could be started.
    if !path.is_file() {
        return Err(SutraError::Pdf(format!(
            "{} is not there to read",
            path.display()
        )));
    }

    let exe = std::env::current_exe()
        .map_err(|e| SutraError::Pdf(format!("could not find Sutra's own program to run: {e}")))?;

    let mut child = Command::new(exe)
        .arg(CHILD_FLAG)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| SutraError::Pdf(format!("could not start text extraction: {e}")))?;

    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if started.elapsed() > TIMEOUT {
                    // Best effort: if the kill fails the child is already gone,
                    // or is unkillable and the report is the same either way.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(SutraError::Pdf(format!(
                        "reading {} took longer than {} seconds and was stopped",
                        path.display(),
                        TIMEOUT.as_secs()
                    )));
                }
                std::thread::sleep(POLL);
            }
            Err(e) => return Err(SutraError::Pdf(format!("lost track of extraction: {e}"))),
        }
    }

    let out = child
        .wait_with_output()
        .map_err(|e| SutraError::Pdf(format!("could not read the result of extraction: {e}")))?;

    if out.status.code() == Some(exit::ENCRYPTED) {
        // Not a fault in the file and not something Sutra can fix by trying
        // again. Saying "could not read this PDF" here would send someone
        // looking for a corrupt download.
        return Err(SutraError::Pdf(format!(
            "{} is password-protected, so its text cannot be read. Zotero's own \
             reader can still open it.",
            path.display()
        )));
    }

    if !out.status.success() {
        // This is the case the child process exists for: the parser hit
        // something it could not handle and took its process down with it. The
        // application is still running, which is the whole point, and the
        // researcher gets a sentence instead of a closed window.
        let said = String::from_utf8_lossy(&out.stderr);
        let said = said.trim();
        let detail = if said.is_empty() {
            "it stopped without saying why, which usually means the file is malformed".to_string()
        } else {
            // Bounded: a parser can be talkative, and this ends up in a toast.
            said.chars().take(300).collect::<String>()
        };
        return Err(SutraError::Pdf(format!(
            "could not read the text of {}: {detail}",
            path.display()
        )));
    }

    serde_json::from_slice::<Extraction>(&out.stdout).map_err(|e| {
        SutraError::Pdf(format!(
            "extraction returned something unreadable for {}: {e}",
            path.display()
        ))
    })
}

/// The child half: extract, print JSON, exit.
///
/// Called from `main` before anything else starts — before Tauri, before any
/// window, before a vault is opened — because this process is not an
/// application run, it is one function call with a process around it.
///
/// Returns the exit code. Failure goes to stderr and a non-zero status rather
/// than into the JSON, so that a panic (which produces neither) is handled by
/// the parent identically to a clean failure. One path, not two.
pub fn run_as_child(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("no path given");
        return exit::NO_PATH;
    };
    let path = Path::new(path);

    let bytes = match pdfread::read_bytes(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("{e}");
            return exit::UNREADABLE;
        }
    };

    // The call that can panic. Nothing is caught, deliberately: a panic aborts
    // this process, the parent sees the status, and the report is the same as
    // for any other failure.
    let pages = match pdf_extract::extract_text_from_mem_by_pages(&bytes) {
        Ok(pages) => pages,
        Err(e) => {
            eprintln!("{e}");
            return if is_encrypted(&e) {
                exit::ENCRYPTED
            } else {
                exit::PARSER
            };
        }
    };

    let extraction = Extraction {
        pages: pages
            .into_iter()
            .enumerate()
            .map(|(i, text)| Page {
                number: i + 1,
                text,
            })
            .collect(),
    };

    match serde_json::to_string(&extraction) {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            exit::OUTPUT
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest PDF that really contains text, built by hand.
    ///
    /// A fixture file would be easier, but a binary blob in the repository that
    /// nobody can read or regenerate is how test data rots. This is a complete
    /// PDF — catalogue, page tree, one page, a font, and a content stream that
    /// draws `words` with the standard text operators — and every byte of it is
    /// here to be read. The cross-reference table is built from the real byte
    /// offsets, which is the part that has to be right for any parser to accept
    /// the file at all.
    fn tiny_pdf(words: &str) -> Vec<u8> {
        let content = format!("BT /F1 12 Tf 72 720 Td ({words}) Tj ET");
        let objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
              /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
                .to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ];

        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }

        let xref_at = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("sutra-{name}-{}.pdf", ulid::Ulid::generate()))
    }

    /// Extraction is a child process, and a child process needs a binary to be.
    /// Under `cargo test` the running executable is the test harness, which
    /// does not understand `CHILD_FLAG` — so these tests exercise the child
    /// half directly and the parent half's failure handling separately, and the
    /// two meeting is what the end-to-end suite covers.
    #[test]
    fn the_child_reads_text_and_says_which_page_it_was_on() {
        let path = scratch("child");
        std::fs::write(&path, tiny_pdf("Hello from page one")).unwrap();

        let bytes = crate::pdfread::read_bytes(&path).unwrap();
        let pages = pdf_extract::extract_text_from_mem_by_pages(&bytes)
            .expect("the hand-built PDF must be readable, or the fixture is wrong");

        assert_eq!(pages.len(), 1, "one page in, one page out");
        assert!(
            pages[0].contains("Hello from page one"),
            "the text did not survive extraction: {:?}",
            pages[0]
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_missing_file_is_named_rather_than_spawned_for() {
        let err = extract(std::path::Path::new("/no/such/paper.pdf"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("not there to read"), "{err}");
    }

    /// The state OCR would address and v0.4 does not: extraction worked, the
    /// pages are blank, there is nothing to quote. It must be distinguishable
    /// from a failure, because telling someone their file is broken when it is
    /// merely scanned sends them to fix the wrong thing.
    #[test]
    fn a_pdf_with_no_text_layer_is_not_a_failure() {
        let empty = Extraction {
            pages: vec![
                Page {
                    number: 1,
                    text: String::new(),
                },
                Page {
                    number: 2,
                    text: "   \n  ".to_string(),
                },
            ],
        };
        assert!(!empty.has_text());

        let real = Extraction {
            pages: vec![Page {
                number: 1,
                text: "Sb2Se3 ribbons".to_string(),
            }],
        };
        assert!(real.has_text());
    }

    #[test]
    fn pages_are_numbered_from_one_as_a_reader_counts_them() {
        let path = scratch("numbering");
        std::fs::write(&path, tiny_pdf("Only page")).unwrap();
        let bytes = crate::pdfread::read_bytes(&path).unwrap();
        let pages = pdf_extract::extract_text_from_mem_by_pages(&bytes).unwrap();

        let extraction = Extraction {
            pages: pages
                .into_iter()
                .enumerate()
                .map(|(i, text)| Page {
                    number: i + 1,
                    text,
                })
                .collect(),
        };
        assert_eq!(extraction.pages[0].number, 1, "not zero-based");
        std::fs::remove_file(&path).ok();
    }

    /// The child half, driven exactly as `main` drives it, on a file that is
    /// not a PDF at all. It must return a non-zero code and say something,
    /// rather than panicking out of the function or printing JSON nobody can
    /// use.
    #[test]
    fn the_child_reports_a_file_it_cannot_read() {
        let path = scratch("garbage");
        std::fs::write(&path, b"this is not a PDF, it is a sentence").unwrap();

        let code = run_as_child(&[path.to_string_lossy().to_string()]);
        assert_ne!(code, 0, "unreadable input must not exit successfully");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn the_child_refuses_to_run_with_no_path() {
        assert_ne!(run_as_child(&[]), 0);
    }
}
