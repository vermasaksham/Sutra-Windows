//! The YAML block at the top of every note.

use crate::error::{Result, SutraError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// A saved query. Its own kind because opening one runs it rather than
    /// showing its body — the one note in the vault that is read by asking a
    /// question instead of by reading it.
    View,
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
            Self::View => "view",
        }
    }

    /// Every kind, in the order the picker offers them.
    ///
    /// Exists only so a test can pin the list. The frontend declares the same union
    /// by hand, and the two drifting apart is silent — a note saved as a kind
    /// the UI has never heard of just renders as the default.
    #[cfg(test)]
    pub fn all() -> [Self; 11] {
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
            Self::View,
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
            "view" => Self::View,
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
    /// The citation key, when the reference manager has one.
    ///
    /// Zotero only has these with Better BibTeX installed. `None` means the
    /// library has none, and is never replaced by a plausible-looking guess:
    /// an invented `@Ko2024` reads correctly in a draft and fails silently at
    /// the bibliography, which is the worst of both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation_key: Option<String>,
    /// The abstract as published. Never a generated summary — a reader must be
    /// able to trust that everything in this struct came from the publisher or
    /// from the person typing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstract_text: Option<String>,
    /// "journalArticle", "book", "thesis" — the reference manager's own word.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    /// When it entered the library, which is often the only clue to why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
    /// The reference manager's collections this item sits in, by name.
    ///
    /// Recorded, never mirrored into folders: an item can be in three
    /// collections while the notes about it live in one folder elsewhere, and
    /// making either follow the other destroys that independence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<String>,
    /// The title of the PDF attachment, when the library has one. Recorded so
    /// the note can say a PDF exists while Zotero is closed; the file itself
    /// is never copied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf: Option<String>,
    /// This paper as the reference manager rendered it, keyed by CSL style id.
    ///
    /// A cache, and a deliberate one. Formatting is Zotero's job and the app
    /// asks Zotero for it — but a thesis draft is written on trains, and a
    /// citation that reads "(Ko et al., 2024)" only while a program is running
    /// is not a citation you can rely on. Keeping every style ever fetched,
    /// rather than only the current one, means switching back to a style used
    /// before needs no network at all.
    ///
    /// A `BTreeMap` so the YAML comes out in a stable order and a note is not
    /// rewritten by a re-serialisation that only changed the order of a map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub styled: BTreeMap<String, crate::references::StyledCitation>,
}

impl SourceMeta {
    /// Fold a freshly-fetched record onto the one the vault already holds.
    ///
    /// `self` is what the reference manager just said; `existing` is what the
    /// source note has recorded. The result is what should be written back.
    ///
    /// The rule is one sentence: **the library is the authority on every fact
    /// it actually answered for, and silent about the rest.** A field the
    /// fetch left empty is not the library saying "this is empty" — it is the
    /// library not having been asked. `import_zotero_source` takes the cheap
    /// path (one search response), which carries no collections and no
    /// attachments at all; before this existed, re-importing a paper through
    /// it read as "no collections, no PDF" and wrote that over the real
    /// answers a fuller fetch had recorded earlier.
    ///
    /// `styled` is the case that mattered most. Formatting is Zotero's job,
    /// but a cached rendering is what makes a citation readable with Zotero
    /// closed — the entire reason the cache exists. A fetch never carries one,
    /// so replacing the map wholesale silently threw away every style ever
    /// rendered. Here the two maps are merged per style id, and an incoming
    /// entry only wins if it says something ([`StyledCitation::is_empty`]
    /// marks the ones that do not, which are failures rather than answers).
    ///
    /// Nothing here invents a value. Every field in the result came either
    /// from this fetch or from a previous one — never from a guess.
    ///
    /// The knowing trade is `citation_key`: keeping the previously fetched one
    /// when a fetch has none means a library that lost Better BibTeX keeps
    /// showing the key it used to have. That is the better failure. The key
    /// was real when it was recorded, and a bibliography that silently loses
    /// its keys breaks a draft in a way nobody notices until submission.
    pub fn merged_over(mut self, existing: &SourceMeta) -> Self {
        fn answered(incoming: Option<String>, held: &Option<String>) -> Option<String> {
            match incoming {
                Some(value) if !value.trim().is_empty() => Some(value),
                _ => held.clone(),
            }
        }

        self.authors = answered(self.authors, &existing.authors);
        self.year = answered(self.year, &existing.year);
        self.container = answered(self.container, &existing.container);
        self.doi = answered(self.doi, &existing.doi);
        self.url = answered(self.url, &existing.url);
        self.zotero = answered(self.zotero, &existing.zotero);
        self.citation_key = answered(self.citation_key, &existing.citation_key);
        self.abstract_text = answered(self.abstract_text, &existing.abstract_text);
        self.item_type = answered(self.item_type, &existing.item_type);
        self.added = answered(self.added, &existing.added);
        self.pdf = answered(self.pdf, &existing.pdf);

        // A fetch that *did* bring collections is authoritative, including
        // when it brings fewer than before: an item taken out of a collection
        // in Zotero must stop claiming membership here. Only an empty list —
        // "not asked" — falls back.
        if self.collections.is_empty() {
            self.collections = existing.collections.clone();
        }

        let mut styled = existing.styled.clone();
        for (style, rendered) in std::mem::take(&mut self.styled) {
            if !rendered.is_empty() {
                styled.insert(style, rendered);
            }
        }
        self.styled = styled;

        self
    }
}

/// One note citing one source, at one place in it.
///
/// This is the provenance record section 5 asks for, and it lives in the
/// note's own frontmatter rather than in the index — so it survives being
/// copied to another machine, opened in another editor, or read in ten years
/// with none of this software installed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Citation {
    /// This piece of evidence's own ULID — its identity, not the source's.
    ///
    /// The distinction v0.3 freezes. `id` says *which paper*; `eid` says
    /// *which reading of it*. Without one, two records of the same source at
    /// the same page are indistinguishable, nothing outside the note can point
    /// at a particular quote, and a citation cannot be told apart from a
    /// second citation of the same work — so evidence could never become a
    /// thing in its own right without changing the file format again.
    ///
    /// `#[serde(default)]` and skipped when empty, so a v0.2 note that has
    /// none stays valid and unchanged on disk. One is minted when a citation
    /// is written through the app; nothing rewrites a vault to add them.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub eid: String,
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
    /// What kind of evidence this is: "experimental", "computational",
    /// "theoretical", "review", "observation".
    ///
    /// A string rather than an enum, for the same reason a view's unknown term
    /// is kept verbatim: a kind written by a newer build must survive being
    /// read and written back by an older one rather than being quietly
    /// dropped. The UI offers the known list and accepts what it finds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
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
    /// Notes a person has said are *not* duplicates of this one.
    ///
    /// In the file rather than the index, because the index is disposable and
    /// deleting it must not resurrect a suggestion someone has already looked
    /// at and dismissed. Written on both notes of a pair, so either can filter
    /// without consulting the other.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_duplicates: Vec<String>,
    /// Present on a note of `type: view`: the query it stands for.
    ///
    /// In the note's own frontmatter rather than in a settings file, so a view
    /// is backed up, synced, versioned and readable as plain text along with
    /// everything else — and so deleting the index cannot lose one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<crate::views::Query>,
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
            not_duplicates: Vec::new(),
            view: None,
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
            not_duplicates: Vec::new(),
            view: None,
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
                "view",
            ]
        );
        // And every one of them survives being written and read back.
        for kind in NoteType::all() {
            assert_eq!(NoteType::parse(kind.as_str()), kind);
        }
    }
}
