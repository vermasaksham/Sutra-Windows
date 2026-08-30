//! The YAML block at the top of every note.

use crate::error::{Result, SutraError};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// The delimiter line. A frontmatter block opens and closes with exactly this.
const FENCE: &str = "---";

/// Page-level metadata. Block-level things (maths, callouts) never appear here
/// — they have a position in the document, and frontmatter cannot express one.
///
/// `#[serde(default)]` on a field means "if the key is missing, use
/// `Default::default()`". That matters because these files are hand-editable:
/// someone will delete a line, and a missing `tags:` should give an empty list,
/// not an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    /// ULID. Stable and permanent — the note's real identity.
    pub id: String,
    pub title: String,
    /// Dead: hierarchy is the folder a note sits in, not a claim the note
    /// makes about itself. Kept on the struct so an unmigrated vault's
    /// `parent:` key survives being read and written back — the migration is
    /// the only thing that reads it, and the only thing that clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Sort order among the notes in a folder. Ties fall back to title.
    #[serde(default)]
    pub position: i64,
    /// `time::serde::rfc3339` tells serde to read and write these as
    /// `2026-08-21T10:14:00Z` rather than some internal representation.
    #[serde(with = "time::serde::rfc3339")]
    pub created: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated: OffsetDateTime,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
}

/// The current time, truncated to whole seconds.
///
/// These files are read and edited by hand, and `2026-08-21T11:53:29Z` is
/// something a person can parse at a glance where
/// `2026-08-21T11:53:29.129750608Z` is not. It also keeps the diff on every
/// save down to the characters that actually changed.
pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
}

impl Frontmatter {
    /// A brand new note's metadata.
    pub fn new(id: String, title: String) -> Self {
        let now = now();
        Self {
            id,
            title,
            parent: None,
            position: 0,
            created: now,
            updated: now,
            tags: Vec::new(),
            icon: None,
            cover: None,
        }
    }
}

/// Split a file into its frontmatter block and its body.
///
/// Returns `None` for the frontmatter when the file has no block at all, which
/// is not an error: someone may have dropped a plain `.md` file into the vault,
/// and we would rather adopt it than reject it. A block that opens but is
/// malformed *is* an error — that is a corrupted note, not a plain one.
///
/// The `&str` return values borrow from `contents`; nothing is copied here. The
/// lifetime elision means the outputs cannot outlive the input, which is
/// exactly right.
pub fn split(contents: &str) -> Result<(Option<Frontmatter>, &str)> {
    // A frontmatter block must be the very first thing in the file. Strip a
    // UTF-8 BOM first — Windows editors like Notepad add one.
    let text = contents.strip_prefix('\u{feff}').unwrap_or(contents);

    let Some(rest) = strip_fence_line(text) else {
        return Ok((None, text));
    };

    // Find the closing fence: a line that is exactly `---`.
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == FENCE {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            let frontmatter: Frontmatter = serde_yaml_ng::from_str(yaml)
                .map_err(|e| SutraError::Frontmatter(e.to_string()))?;
            // A body conventionally starts after one blank line; drop it so the
            // body we hand out is the prose itself.
            return Ok((Some(frontmatter), body.strip_prefix('\n').unwrap_or(body)));
        }
        offset += line.len();
    }

    Err(SutraError::Frontmatter(
        "opening --- has no matching closing ---".into(),
    ))
}

/// Consume a leading `---` line, returning what follows it.
fn strip_fence_line(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(FENCE)?;
    match rest.strip_prefix("\r\n") {
        Some(r) => Some(r),
        None => rest.strip_prefix('\n'),
    }
}

/// Render metadata and body back into the file format.
///
/// Always writes `\n` endings. Git, every editor worth using, and our own
/// parser handle them on Windows, and picking one keeps saves byte-stable
/// instead of flip-flopping with whatever last touched the file.
pub fn join(frontmatter: &Frontmatter, body: &str) -> Result<String> {
    let yaml = serde_yaml_ng::to_string(frontmatter)?;
    let body = body.trim_end_matches('\n');
    Ok(format!("{FENCE}\n{yaml}{FENCE}\n\n{body}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn sample() -> Frontmatter {
        Frontmatter {
            id: "01HQ3M8K2P".into(),
            title: "CVT runs".into(),
            parent: Some("01HQ3M8K1A".into()),
            position: 3,
            created: datetime!(2026-08-21 10:14:00 UTC),
            updated: datetime!(2026-08-21 11:02:00 UTC),
            tags: vec!["sb2se3".into(), "cvt".into()],
            icon: None,
            cover: None,
        }
    }

    #[test]
    fn timestamps_are_whole_seconds() {
        // Sub-second precision is noise in a file a person reads and edits,
        // and it makes every save a bigger diff than it needs to be.
        assert_eq!(now().nanosecond(), 0);
    }

    #[test]
    fn round_trips() {
        let original = sample();
        let file = join(&original, "Body text.").unwrap();
        let (parsed, body) = split(&file).unwrap();
        assert_eq!(parsed.unwrap(), original);
        assert_eq!(body, "Body text.\n");
    }

    #[test]
    fn saving_twice_is_byte_stable() {
        let file = join(&sample(), "Body text.").unwrap();
        let (fm, body) = split(&file).unwrap();
        assert_eq!(join(&fm.unwrap(), body).unwrap(), file);
    }

    #[test]
    fn adopts_a_file_with_no_frontmatter() {
        let (fm, body) = split("Just prose.\n").unwrap();
        assert!(fm.is_none());
        assert_eq!(body, "Just prose.\n");
    }

    #[test]
    fn tolerates_a_bom() {
        let file = format!("\u{feff}{}", join(&sample(), "Body.").unwrap());
        assert!(split(&file).unwrap().0.is_some());
    }

    #[test]
    fn missing_optional_keys_are_defaults() {
        let file = "---\nid: abc\ntitle: T\ncreated: 2026-08-21T10:14:00Z\nupdated: 2026-08-21T10:14:00Z\n---\n\nBody\n";
        let (fm, _) = split(file).unwrap();
        let fm = fm.unwrap();
        assert!(fm.tags.is_empty());
        assert_eq!(fm.parent, None);
        assert_eq!(fm.position, 0);
    }

    #[test]
    fn an_unclosed_block_is_an_error() {
        assert!(split("---\nid: abc\n\nBody without a closing fence\n").is_err());
    }

    #[test]
    fn a_body_containing_a_fence_survives() {
        // A horizontal rule in the prose must not be mistaken for the closing
        // fence — the closing fence is found before the body is ever scanned.
        let file = join(&sample(), "Above\n\n---\n\nBelow").unwrap();
        let (_, body) = split(&file).unwrap();
        assert_eq!(body, "Above\n\n---\n\nBelow\n");
    }
}
