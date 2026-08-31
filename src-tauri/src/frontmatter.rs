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
/// What kind of note this is.
///
/// Never asked for up front. Everything starts as `Standard` and can be
/// changed later, because deciding what a thought is before writing it down is
/// exactly the friction capture is supposed to avoid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteType {
    #[default]
    Standard,
    Literature,
    Idea,
    Question,
    Experiment,
    Project,
    Meeting,
    Task,
    Daily,
    /// A paper, book or dataset. Its own kind because a source is cited rather
    /// than written, and mixing them into the note list would bury the notes.
    Source,
}

impl NoteType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Literature => "literature",
            Self::Idea => "idea",
            Self::Question => "question",
            Self::Experiment => "experiment",
            Self::Project => "project",
            Self::Meeting => "meeting",
            Self::Task => "task",
            Self::Daily => "daily",
            Self::Source => "source",
        }
    }

    /// Every kind, in the order the picker offers them.
    ///
    /// Exists only so a test can pin the list. The frontend declares the same union
    /// by hand, and the two drifting apart is silent — a note saved as a kind
    /// the UI has never heard of just renders as the default.
    #[cfg(test)]
    pub fn all() -> [Self; 10] {
        [
            Self::Standard,
            Self::Literature,
            Self::Idea,
            Self::Question,
            Self::Experiment,
            Self::Project,
            Self::Meeting,
            Self::Task,
            Self::Daily,
            Self::Source,
        ]
    }

    /// Infallible on purpose. A hand-edited `type: litrature` should leave the
    /// note perfectly usable as a standard one, not make the file unreadable.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "literature" => Self::Literature,
            "idea" => Self::Idea,
            "question" => Self::Question,
            "experiment" => Self::Experiment,
            "project" => Self::Project,
            "meeting" => Self::Meeting,
            "task" => Self::Task,
            "daily" => Self::Daily,
            "source" => Self::Source,
            _ => Self::Standard,
        }
    }
}

impl<'de> Deserialize<'de> for NoteType {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// What a source note records about the thing it stands for.
///
/// Grouped under one `source:` key rather than spread across the top level, so
/// a glance at the file says which half is Sutra's bookkeeping and which half
/// is the paper.
///
/// Every field is optional. A source captured from a scribbled reference with
/// only a title is still a source, and refusing it would push people back to
/// writing citations by hand in prose — which is exactly the loss of
/// provenance this exists to prevent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceMeta {
    /// "Zhou, Y.; Wang, L." — as written, not parsed. Author name parsing is a
    /// famously bad idea and citation style is the exporter's problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    /// Journal, book, conference — whatever the thing appeared in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The Zotero item key this was imported from, so a re-import updates the
    /// same note instead of making a second one. Absent for a source typed in
    /// by hand, which must remain a perfectly ordinary thing to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zotero: Option<String>,
}

/// One note citing one source, at one place in it.
///
/// This is the provenance record section 5 asks for, and it lives in the
/// note's own frontmatter rather than in the index — so it survives being
/// copied to another machine, opened in another editor, or read in ten years
/// with none of this software installed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Citation {
    /// The source note's ULID. Not a Zotero key: a source is a note in the
    /// vault, so a citation keeps working whether or not Zotero ever exists
    /// again on this machine.
    pub id: String,
    /// A string, not a number: "S12", "6-8" and "iv" are all real page
    /// references and none of them is an integer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// What the source actually says, in its own words. The heart of keeping
    /// the author's claim separate from the reader's reading of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "time::serde::rfc3339::option"
    )]
    pub captured: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    /// ULID. Stable and permanent — the note's real identity.
    pub id: String,
    /// Missing means `Standard`, which is what every note written before this
    /// existed should be read as.
    #[serde(rename = "type", default)]
    pub note_type: NoteType,
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
    /// Present on a note of `type: source`, and meaningless on any other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceMeta>,
    /// The sources this note draws on, with where in them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Citation>,
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
            note_type: NoteType::default(),
            title,
            parent: None,
            position: 0,
            created: now,
            updated: now,
            tags: Vec::new(),
            icon: None,
            cover: None,
            source: None,
            sources: Vec::new(),
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
            note_type: NoteType::Literature,
            title: "CVT runs".into(),
            parent: Some("01HQ3M8K1A".into()),
            position: 3,
            created: datetime!(2026-08-21 10:14:00 UTC),
            updated: datetime!(2026-08-21 11:02:00 UTC),
            tags: vec!["sb2se3".into(), "cvt".into()],
            icon: None,
            cover: None,
            source: None,
            sources: Vec::new(),
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

    #[test]
    fn a_note_type_round_trips_through_yaml() {
        let text = join(&sample(), "body").unwrap();
        assert!(text.contains("type: literature"), "{text}");
        let (parsed, _) = split(&text).unwrap();
        assert_eq!(parsed.unwrap().note_type, NoteType::Literature);
    }

    #[test]
    fn a_note_written_before_types_existed_reads_as_standard() {
        let text = "---\nid: x\ntitle: Old\ncreated: 2026-08-21T10:14:00Z\nupdated: 2026-08-21T10:14:00Z\n---\n\nbody\n";
        let (parsed, _) = split(text).unwrap();
        assert_eq!(parsed.unwrap().note_type, NoteType::Standard);
    }

    #[test]
    fn a_misspelled_type_reads_as_standard_rather_than_breaking_the_note() {
        // These files are hand-edited. `type: litrature` should cost the note
        // its category, not its readability.
        let text = "---\nid: x\ntype: litrature\ntitle: T\ncreated: 2026-08-21T10:14:00Z\nupdated: 2026-08-21T10:14:00Z\n---\n\nb\n";
        let (parsed, _) = split(text).unwrap();
        assert_eq!(parsed.unwrap().note_type, NoteType::Standard);
    }

    #[test]
    fn the_note_types_match_the_ones_the_frontend_declares() {
        // src/vault/api.ts declares this union by hand. If you add a kind here
        // and not there, a note saved as it renders as a plain note with no
        // error anywhere — so the list is pinned in both places on purpose.
        let names: Vec<&str> = NoteType::all().iter().map(|t| t.as_str()).collect();
        assert_eq!(
            names,
            [
                "standard",
                "literature",
                "idea",
                "question",
                "experiment",
                "project",
                "meeting",
                "task",
                "daily",
                "source",
            ]
        );
        // And every one of them survives being written and read back.
        for kind in NoteType::all() {
            assert_eq!(NoteType::parse(kind.as_str()), kind);
        }
    }
}
