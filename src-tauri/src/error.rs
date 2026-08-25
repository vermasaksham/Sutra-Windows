//! One error type for the whole storage layer.

use serde::{Serialize, Serializer};

/// Everything that can go wrong while talking to the vault.
///
/// `thiserror` generates the boilerplate: the `Display` implementation comes
/// from the `#[error("...")]` strings, and each `#[from]` generates a `From`
/// impl. That `From` is what makes the `?` operator work — when a function
/// returns `Result<_, SutraError>` and you call something returning
/// `Result<_, std::io::Error>`, `?` converts the error for you on the way out.
///
/// `{0}` in the message interpolates the wrapped value.
#[derive(Debug, thiserror::Error)]
pub enum SutraError {
    #[error("no vault is open")]
    NoVault,

    #[error("no note with id {0}")]
    NoteNotFound(String),

    #[error("{0} is not a directory")]
    NotADirectory(String),

    /// The frontmatter block was present but malformed.
    #[error("could not read frontmatter: {0}")]
    Frontmatter(String),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),

    /// Index failures are recoverable by definition — the index is derived, so
    /// the worst case is rebuilding it. Reported rather than swallowed so a
    /// broken index does not quietly degrade search into "no results".
    #[error("index error: {0}")]
    Index(#[from] rusqlite::Error),
}

/// Tauri sends a command's `Err` to the frontend as JSON, so the error has to
/// be serialisable. We deliberately send only the human-readable message and
/// never the underlying path: the frontend has no business knowing where the
/// vault sits on disk.
impl Serialize for SutraError {
    // Note the fully-qualified `std::result::Result` here. The `Result<T>`
    // alias at the bottom of this file takes one type parameter and pins the
    // error to SutraError, so writing a bare `Result<S::Ok, S::Error>` inside
    // this module resolves to the alias and fails with "expected 1 generic
    // argument, found 2". Aliases shadow the prelude.
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Shorthand so functions can be written `-> Result<Note>`.
pub type Result<T> = std::result::Result<T, SutraError>;
