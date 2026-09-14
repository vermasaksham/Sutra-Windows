//! Every piece of evidence in the vault, assembled so it can be browsed apart
//! from the notes that use it.
//!
//! A retrieval surface, not a database. The vault files stay authoritative and
//! nothing here writes: this reads what is already indexed and joins it into
//! the shape the question "what have I actually got on this?" needs.
//!
//! The join it does is the one a note cannot do for itself. A note knows what
//! it cites; a paper knows what has been quoted from it; neither knows which
//! *other* notes rest on the same sentence. That is the thing researchers lose
//! track of, and it is why this exists rather than a filter over the note list.

use crate::frontmatter::{Citation, SharedEvidence};
use crate::vault::NoteSummary;
use serde::Serialize;

/// One piece of evidence, with everything about it in one place.
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceItem {
    pub eid: String,
    /// The Source note it was taken from.
    pub source: String,
    /// That note's title, so a list reads as papers rather than as ULIDs.
    pub source_title: String,
    pub page: Option<String>,
    pub page_index: Option<u32>,
    pub quote: Option<String>,
    pub kind: Option<String>,
    pub origin: Option<String>,
    pub annotation: Option<String>,
    pub colour: Option<String>,
    /// True when the record lives on the Source note, so more than one note can
    /// rest on it. False means it is inline, in the single note using it.
    pub shared: bool,
    /// Which notes draw on this, and what each of them made of it.
    pub used_by: Vec<Use>,
}

/// One note's use of one piece of evidence.
#[derive(Debug, Clone, Serialize)]
pub struct Use {
    pub note: String,
    pub title: String,
    /// This reader's own remark. Never the paper's words — see `SharedEvidence`.
    pub comment: Option<String>,
}

/// Assemble every piece of evidence from a vault listing.
///
/// Two passes, because the two halves live in different notes: shared records
/// are on Source notes, and who uses them is on everything else.
///
/// **An orphaned reference is kept, not dropped.** A citation pointing at a
/// record that is not there becomes an item with no quotation and its `used_by`
/// intact, so the browser can show that something was recorded here and is now
/// missing. Silently omitting it would hide exactly the breakage the
/// completeness checks exist to surface.
pub fn gather(notes: &[NoteSummary]) -> Vec<EvidenceItem> {
    let mut items: Vec<EvidenceItem> = Vec::new();
    let mut by_eid: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    // Shared records first, so a reference found below joins an existing item
    // rather than creating a second one for the same `eid`.
    for note in notes {
        for record in &note.evidence {
            by_eid.insert(record.eid.clone(), items.len());
            items.push(from_shared(record, note));
        }
    }

    for note in notes {
        for citation in &note.sources {
            let used = Use {
                note: note.id.clone(),
                title: note.title.clone(),
                comment: citation.comment.clone(),
            };
            match by_eid.get(&citation.eid) {
                Some(&at) => items[at].used_by.push(used),
                None => {
                    // Inline, or a reference whose record is gone. Both are
                    // items in their own right; `shared` and an empty `quote`
                    // tell them apart.
                    by_eid.insert(citation.eid.clone(), items.len());
                    let mut item = from_inline(citation, notes);
                    item.used_by.push(used);
                    items.push(item);
                }
            }
        }
    }

    items
}

fn from_shared(record: &SharedEvidence, source: &NoteSummary) -> EvidenceItem {
    EvidenceItem {
        eid: record.eid.clone(),
        source: source.id.clone(),
        source_title: source.title.clone(),
        page: record.page.clone(),
        page_index: record.page_index,
        quote: record.quote.clone(),
        kind: record.kind.clone(),
        origin: record.origin.clone(),
        annotation: record.annotation.clone(),
        colour: record.colour.clone(),
        shared: true,
        used_by: Vec::new(),
    }
}

fn from_inline(citation: &Citation, notes: &[NoteSummary]) -> EvidenceItem {
    let title = notes
        .iter()
        .find(|n| n.id == citation.id)
        .map(|n| n.title.clone())
        // The source note is not in this vault. Named as a state rather than
        // left blank, and never guessed at from the citation itself.
        .unwrap_or_else(|| "Source note missing".into());
    EvidenceItem {
        eid: citation.eid.clone(),
        source: citation.id.clone(),
        source_title: title,
        page: citation.page.clone(),
        page_index: citation.page_index,
        quote: citation.quote.clone(),
        kind: citation.kind.clone(),
        origin: citation.origin.clone(),
        annotation: citation.annotation.clone(),
        colour: citation.colour.clone(),
        // A reference whose record is missing is not shared evidence — there is
        // no shared record. It is a reference to nothing, and reads as one.
        shared: false,
        used_by: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter::NoteType;

    fn note(id: &str, title: &str, note_type: NoteType) -> NoteSummary {
        NoteSummary {
            id: id.into(),
            note_type,
            title: title.into(),
            folder: String::new(),
            position: 0,
            tags: Vec::new(),
            icon: None,
            cover: None,
            source: None,
            evidence: Vec::new(),
            sources: Vec::new(),
            excerpt: String::new(),
            updated: time::OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn record(eid: &str, quote: &str) -> SharedEvidence {
        SharedEvidence {
            eid: eid.into(),
            quote: Some(quote.into()),
            page: Some("431".into()),
            kind: Some("measurement".into()),
            ..Default::default()
        }
    }

    fn reference(eid: &str, source: &str, comment: &str) -> Citation {
        Citation {
            eid: eid.into(),
            id: source.into(),
            at: Some("source".into()),
            comment: Some(comment.into()),
            ..Default::default()
        }
    }

    /// The join no single note can do for itself, and the reason this module
    /// exists: which *other* notes rest on the same sentence.
    #[test]
    fn one_record_used_twice_is_one_item_with_two_uses() {
        let mut paper = note("S1", "Zhou 2019", NoteType::Source);
        paper.evidence.push(record("E1", "ribbons align along c"));

        let mut first = note("N1", "Reading Zhou", NoteType::Literature);
        first
            .sources
            .push(reference("E1", "S1", "only two samples"));
        let mut second = note("N2", "Chapter 3", NoteType::Standard);
        second
            .sources
            .push(reference("E1", "S1", "supports the growth argument"));

        let items = gather(&[paper, first, second]);
        assert_eq!(items.len(), 1, "one quotation, one item");
        assert_eq!(items[0].quote.as_deref(), Some("ribbons align along c"));
        assert!(items[0].shared);
        assert_eq!(items[0].source_title, "Zhou 2019");

        let uses: Vec<&str> = items[0].used_by.iter().map(|u| u.title.as_str()).collect();
        assert_eq!(uses, ["Reading Zhou", "Chapter 3"]);
        // Each reader's own remark stays theirs.
        assert_eq!(
            items[0].used_by[0].comment.as_deref(),
            Some("only two samples")
        );
        assert_eq!(
            items[0].used_by[1].comment.as_deref(),
            Some("supports the growth argument")
        );
    }

    #[test]
    fn an_inline_record_is_an_item_of_its_own() {
        let paper = note("S1", "Zhou 2019", NoteType::Source);
        let mut reading = note("N1", "Reading Zhou", NoteType::Literature);
        reading.sources.push(Citation {
            eid: "E2".into(),
            id: "S1".into(),
            quote: Some("conductivity falls above 400 K".into()),
            ..Default::default()
        });

        let items = gather(&[paper, reading]);
        assert_eq!(items.len(), 1);
        assert!(!items[0].shared, "an inline record is not shared");
        assert_eq!(items[0].source_title, "Zhou 2019");
        assert_eq!(items[0].used_by.len(), 1);
    }

    /// A reference whose record is gone stays visible. Dropping it would hide
    /// the one thing worth showing: that something was recorded here.
    #[test]
    fn a_reference_to_nothing_is_kept_and_reads_as_empty() {
        let paper = note("S1", "Zhou 2019", NoteType::Source);
        let mut reading = note("N1", "Reading Zhou", NoteType::Literature);
        reading
            .sources
            .push(reference("GONE", "S1", "the key number"));

        let items = gather(&[paper, reading]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].eid, "GONE");
        assert_eq!(items[0].quote, None, "a quotation was invented");
        assert!(!items[0].shared);
        // And what the reader wrote about it survives, which is all that is
        // left of the evidence.
        assert_eq!(
            items[0].used_by[0].comment.as_deref(),
            Some("the key number")
        );
    }

    #[test]
    fn a_shared_record_nothing_cites_is_still_listed() {
        // Captured from a paper and not yet used. Absent from every note's
        // `sources:`, and still evidence the researcher collected.
        let mut paper = note("S1", "Zhou 2019", NoteType::Source);
        paper.evidence.push(record("E1", "ribbons align along c"));

        let items = gather(&[paper]);
        assert_eq!(items.len(), 1);
        assert!(items[0].used_by.is_empty());
    }

    #[test]
    fn evidence_whose_source_left_the_vault_is_named_as_such() {
        let mut reading = note("N1", "Reading Zhou", NoteType::Literature);
        reading.sources.push(Citation {
            eid: "E3".into(),
            id: "S-GONE".into(),
            quote: Some("ribbons align along c".into()),
            ..Default::default()
        });

        let items = gather(&[reading]);
        assert_eq!(items[0].source_title, "Source note missing");
        // The quotation is the researcher's and does not evaporate with the
        // note that named the paper.
        assert_eq!(items[0].quote.as_deref(), Some("ribbons align along c"));
    }
}
