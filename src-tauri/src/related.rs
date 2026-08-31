//! Why one note is near another, and how near.
//!
//! The panel this feeds has one job the brief is explicit about: not to list
//! neighbours but to say *why* each is a neighbour. A ranked list with no
//! reasons is a list you learn to distrust, because the one time it is wrong
//! you have no way to tell — and a suggestion you cannot check is worse than
//! none, since it costs attention on every note.
//!
//! So relatedness here is never a single opaque number. It is a set of
//! [`Reason`]s, each of which is a fact about the two notes that a person can
//! verify at a glance, and the score is just their sum. The sentence under a
//! result is the same data the ranking used, not a separate explanation
//! written afterwards to sound plausible.
//!
//! Scoring and wording live here, away from SQL, so both can be tested without
//! a database and neither can quietly depend on the shape of a query.

use serde::Serialize;

/// One fact linking two notes.
///
/// Every variant carries what it needs to explain itself. Nothing is a bare
/// weight: if a signal cannot be put into a sentence, it does not belong in a
/// panel whose purpose is the sentence.
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    /// Both carry this tag. `idf` is how much that says — a tag on three notes
    /// out of five hundred is a real statement about both; one on half the
    /// vault is barely a fact.
    Tag { tag: String, idf: f64 },
    /// Both cite this source. The strongest ordinary signal there is: two
    /// notes drawing on the same paper are nearly always about the same thing,
    /// and unlike a shared word it cannot happen by accident.
    Source { title: String },
    /// Both link to this note, and it is a project. Kept apart from a plain
    /// co-link because "both in PhD Thesis" is a different sentence, and a
    /// truer one, than "both link to PhD Thesis".
    Project { title: String },
    /// Both link to this note.
    CoLink { title: String },
    /// They share this many distinctive words. `idf` is those words' combined
    /// weight, so six rare terms outrank six ordinary ones.
    Terms { count: usize, idf: f64 },
    /// Same folder. Never a reason on its own — a folder of forty notes would
    /// make forty neighbours — but a fair tiebreak between two that are
    /// otherwise equal.
    Folder,
}

/// How much a shared source is worth.
///
/// Higher than any single tag can reach. A shared citation is a deliberate act
/// by the same person about the same paper; a shared tag is a filing decision,
/// and a shared word can be a coincidence.
const SOURCE_WEIGHT: f64 = 3.0;
/// Brings raw inverse document frequency onto the same scale as the other
/// signals. Unscaled, a tag carried by a tenth of the vault scores 2.3, so two
/// such tags would outrank a shared citation — which gets the evidence exactly
/// backwards. At 0.7 a tag on a tenth of the vault is worth about half a
/// source, a tag on a hundredth is worth slightly more than one, and it takes
/// a genuinely rare tag to beat one outright.
const TAG_SCALE: f64 = 0.7;
const PROJECT_WEIGHT: f64 = 2.0;
const COLINK_WEIGHT: f64 = 1.2;
const FOLDER_WEIGHT: f64 = 0.3;
/// Shared prose is the weakest signal and the noisiest, so it is scaled down
/// and capped: a long note shares many words with everything, and without a
/// ceiling length alone would decide the ranking.
const TERM_SCALE: f64 = 0.35;
const TERM_CEILING: f64 = 2.5;

/// What a note must score to be shown at all.
///
/// A floor, not a limit — the panel would rather be short than padded. Two
/// notes sharing one ordinary tag are not related in any sense worth a line of
/// screen, and listing them is how a reader learns to stop looking at the
/// panel.
///
/// Read as a statement about tags, since one shared tag is the commonest way
/// to land near it: at 1.3, a single tag earns a row only if fewer than about
/// one note in six carries it. `#method/dsc` on four notes of thirty-five
/// clears it; `#sb2se3` on six does not, and needs a second signal.
///
/// Calibrated against the realistic vault in `index.rs`'s tests, which is one
/// vault and not a proof. If the panel ever reads as padded, this is the
/// number to raise, and `judge_the_panel_on_a_realistic_vault` is how to see
/// the effect.
pub const FLOOR: f64 = 1.3;

/// Below this a reason is true but empty, and goes unsaid.
const NOT_WORTH_SAYING: f64 = 0.25;

/// A note near the one being read, and why.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub reasons: Vec<Reason>,
}

/// What the frontend receives: the note, the sentence, and the score behind it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Related {
    pub id: String,
    pub title: String,
    pub folder: String,
    /// One line saying why this is here. Never empty.
    pub reason: String,
    /// The sum of the reasons. Sent so the panel could show it while tuning,
    /// and so a test can assert an ordering rather than a rendering.
    pub score: f64,
}

impl Reason {
    /// What this reason contributes to the score.
    fn weight(&self) -> f64 {
        match self {
            Self::Tag { idf, .. } => idf * TAG_SCALE,
            Self::Source { .. } => SOURCE_WEIGHT,
            Self::Project { .. } => PROJECT_WEIGHT,
            Self::CoLink { .. } => COLINK_WEIGHT,
            Self::Terms { idf, .. } => (idf * TERM_SCALE).min(TERM_CEILING),
            Self::Folder => FOLDER_WEIGHT,
        }
    }
}

impl Candidate {
    pub fn score(&self) -> f64 {
        self.reasons.iter().map(Reason::weight).sum()
    }

    /// The reasons, strongest first.
    fn ranked(&self) -> Vec<&Reason> {
        let mut out: Vec<&Reason> = self.reasons.iter().collect();
        out.sort_by(|a, b| {
            b.weight()
                .partial_cmp(&a.weight())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }

    /// The one-line explanation.
    ///
    /// At most two reasons, strongest first. A third wraps the line, and by
    /// the third the reader has already decided whether to click — this is
    /// there to be checked at a glance, not to be complete.
    ///
    /// Lowercase, because it is a fragment sitting under a title rather than a
    /// sentence of its own.
    pub fn explain(&self) -> String {
        let ranked = self.ranked();
        // The folder is a tiebreak, not something to say out loud beside a
        // real reason: "same folder" next to a shared source reads as if the
        // folder were the point. On its own it is all there is to say.
        let worth_saying: Vec<&Reason> = if ranked.len() == 1 {
            ranked
        } else {
            ranked
                .into_iter()
                // A tag on nearly every note scores about nothing, and saying
                // "shares #note" beside a real reason is worse than silence:
                // it offers the reader a fact to check that turns out to be
                // empty.
                .filter(|r| r.weight() > NOT_WORTH_SAYING)
                .filter(|r| !matches!(r, Reason::Folder))
                .collect()
        };
        if worth_saying.is_empty() {
            // Unreachable through `rank`, which drops anything below the
            // floor, but a total function is worth more than an assertion.
            return "near this note".into();
        }
        let said = worth_saying.iter().take(2).copied().collect::<Vec<_>>();
        // Two tags read as one clause: "shares #sb2se3 and #cvt", not
        // "shares #sb2se3 and shares #cvt".
        if let [
            Reason::Tag { tag: first, .. },
            Reason::Tag { tag: second, .. },
        ] = said[..]
        {
            return format!("shares #{first} and #{second}");
        }
        said.iter()
            .map(|r| phrase(r))
            .collect::<Vec<_>>()
            .join(" and ")
    }

    pub fn finish(&self) -> Related {
        Related {
            id: self.id.clone(),
            title: self.title.clone(),
            folder: self.folder.clone(),
            reason: self.explain(),
            score: self.score(),
        }
    }
}

/// How much of a title a reason may spend.
///
/// A source note's title is the paper's, and papers have long titles. The line
/// is read at a glance beside a result, so it is cut to the part that
/// identifies the thing — the full title is one click away on the note itself.
const TITLE_BUDGET: usize = 28;

fn short(title: &str) -> String {
    if title.chars().count() <= TITLE_BUDGET {
        return title.to_string();
    }
    // Cut at a word boundary where there is one nearby, so the result reads as
    // a shortened title rather than a truncated string.
    let cut: String = title.chars().take(TITLE_BUDGET).collect();
    let trimmed = match cut.rsplit_once(' ') {
        Some((head, _)) if head.chars().count() >= TITLE_BUDGET / 2 => head,
        _ => cut.as_str(),
    };
    format!("{}…", trimmed.trim_end_matches([' ', ',', '—', '-']))
}

fn phrase(reason: &Reason) -> String {
    match reason {
        Reason::Tag { tag, .. } => format!("shares #{tag}"),
        Reason::Source { title } => format!("cites {} too", short(title)),
        Reason::Project { title } => format!("both in {}", short(title)),
        Reason::CoLink { title } => format!("both link to {}", short(title)),
        Reason::Terms { count: 1, .. } => "shares a distinctive word".into(),
        Reason::Terms { count, .. } => format!("shares {count} distinctive words"),
        Reason::Folder => "same folder".into(),
    }
}

/// How much a tag says, from how many notes carry it.
///
/// Plain inverse document frequency. A tag on 3 of 500 notes scores about 5;
/// one on 250 scores 0.7. That difference is the whole reason tags are
/// weighted at all: without it, `#note` on everything would outrank `#Sb2Se3`
/// on four, purely by being common.
pub fn idf(total: usize, carrying: usize) -> f64 {
    if total == 0 || carrying == 0 {
        return 0.0;
    }
    ((total as f64) / (carrying as f64)).ln().max(0.0)
}

/// Rank candidates and drop the ones not worth a line.
///
/// Ties break on title so the order is stable between runs — a panel whose
/// rows shuffle on every keystroke is one nobody can use.
pub fn rank(mut candidates: Vec<Candidate>, limit: usize) -> Vec<Related> {
    candidates.retain(|c| c.score() >= FLOOR);
    candidates.sort_by(|a, b| {
        b.score()
            .partial_cmp(&a.score())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.title.cmp(&b.title))
    });
    candidates.truncate(limit);
    candidates.iter().map(Candidate::finish).collect()
}

/// The words in a note, lowercased, in the order they appear, de-duplicated.
///
/// Approximates FTS5's `unicode61` tokeniser closely enough to *propose*
/// terms; FTS5 itself does the matching, so a disagreement here costs a
/// candidate term rather than a wrong result.
pub fn terms(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        // Two characters is not a distinctive word, and a bare number is not
        // one either: "300" appears in every temperature in the vault.
        if raw.chars().count() < 3 || raw.chars().all(|c| c.is_numeric()) {
            continue;
        }
        let word = raw.to_lowercase();
        if STOPWORDS.contains(&word.as_str()) {
            continue;
        }
        if seen.insert(word.clone()) {
            out.push(word);
        }
    }
    out
}

/// Words too common to say anything, dropped before the index is even asked.
///
/// Short and English-only on purpose. The real filtering is done by document
/// frequency, which needs no list and adapts to the vault — this only saves
/// asking about words no vault will ever find distinctive.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "that", "this", "with", "from", "are", "was", "were", "but", "not", "all",
    "any", "can", "has", "have", "had", "its", "one", "two", "out", "which", "when", "where",
    "there", "here", "then", "than", "they", "them", "their", "these", "those", "what", "how",
    "why", "who", "will", "would", "could", "should", "been", "being", "into", "over", "under",
    "more", "most", "some", "such", "only", "also", "each", "other", "about", "after", "before",
    "between", "both", "does", "did", "doing", "you", "your", "our", "his", "her", "she", "him",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(title: &str, reasons: Vec<Reason>) -> Candidate {
        Candidate {
            id: title.to_string(),
            title: title.to_string(),
            folder: "Research".into(),
            reasons,
        }
    }

    fn tag(name: &str, idf: f64) -> Reason {
        Reason::Tag {
            tag: name.into(),
            idf,
        }
    }

    #[test]
    fn a_rare_tag_says_more_than_a_common_one() {
        // The whole reason tags are weighted rather than counted. Without
        // this, `#note` on every note in the vault would outrank `#Sb2Se3` on
        // four of them, purely by being common.
        let rare = idf(500, 3);
        let common = idf(500, 250);
        assert!(rare > common * 5.0, "rare {rare}, common {common}");
        // A tag on every note says nothing at all.
        assert_eq!(idf(500, 500), 0.0);
    }

    #[test]
    fn a_shared_source_outranks_an_ordinary_shared_tag() {
        // Two notes citing the same paper is a deliberate act by one person
        // about one paper. A shared tag is a filing decision, and a tag on a
        // tenth of the vault is barely that. The scale exists so the ranking
        // reflects the difference: unscaled, two ordinary tags would outrank a
        // shared citation, which gets the evidence backwards.
        let source = candidate(
            "cites the same paper",
            vec![Reason::Source {
                title: "Zhou 2019".into(),
            }],
        );
        let ordinary = candidate("shares #method", vec![tag("method", idf(500, 50))]);
        assert!(source.score() > ordinary.score());
        assert!(
            source.score() > ordinary.score() * 1.5,
            "source {}, tag {}",
            source.score(),
            ordinary.score()
        );

        // And two of them still do not add up to a shared citation.
        let twice = candidate(
            "shares two ordinary tags",
            vec![tag("method", idf(500, 50)), tag("open", idf(500, 50))],
        );
        assert!(twice.score() < source.score() * 1.2);
    }

    #[test]
    fn a_nearly_unique_tag_may_outrank_a_shared_source() {
        // Not a contradiction of the test above — the correct ordering. A tag
        // two notes out of five thousand share is a stronger statement about
        // both than a citation they have in common with a dozen others.
        let source = candidate(
            "cites the same paper",
            vec![Reason::Source {
                title: "Zhou 2019".into(),
            }],
        );
        let rare = candidate("shares #sbsei", vec![tag("sbsei", idf(5_000, 2))]);
        assert!(rare.score() > source.score());
    }

    #[test]
    fn shared_prose_cannot_outrank_everything_by_being_long() {
        // A long note shares words with everything. Without the ceiling,
        // length alone would decide the ranking and the panel would show the
        // same four sprawling notes next to every note in the vault.
        let sprawling = candidate(
            "a very long note",
            vec![Reason::Terms {
                count: 200,
                idf: 900.0,
            }],
        );
        let focused = candidate(
            "one shared source",
            vec![Reason::Source {
                title: "Zhou 2019".into(),
            }],
        );
        assert!(sprawling.score() < focused.score());
        assert!(sprawling.score() <= TERM_CEILING);
    }

    #[test]
    fn the_floor_keeps_a_single_common_tag_out_of_the_panel() {
        // A panel padded with weak rows is one the reader learns to ignore,
        // which costs more than an empty panel ever could.
        let weak = candidate("barely related", vec![tag("note", idf(500, 300))]);
        assert!(weak.score() < FLOOR);
        assert!(rank(vec![weak], 5).is_empty());
    }

    #[test]
    fn the_folder_alone_is_not_enough_to_be_related() {
        // Otherwise every note in a folder is "related" to every other, which
        // is a fact about filing that the folder tree already shows.
        let sibling = candidate("a sibling", vec![Reason::Folder]);
        assert!(sibling.score() < FLOOR);
        assert!(rank(vec![sibling], 5).is_empty());
    }

    #[test]
    fn the_folder_breaks_a_tie_but_is_never_the_reason_given() {
        // "same folder" beside a shared source reads as if the folder were the
        // point. It decides the order; it does not get the line.
        let source = Reason::Source {
            title: "Zhou 2019".into(),
        };
        let near = candidate("in this folder", vec![source.clone(), Reason::Folder]);
        let far = candidate("elsewhere", vec![source]);
        assert!(near.score() > far.score());
        assert_eq!(near.explain(), "cites Zhou 2019 too");

        let ranked = rank(vec![far, near], 5);
        assert_eq!(ranked[0].title, "in this folder");
    }

    #[test]
    fn a_reason_is_always_given_and_reads_as_a_fragment() {
        // Every row in the panel has to say why it is there. Lowercase,
        // because it sits under a title rather than opening a sentence.
        for reasons in [
            vec![Reason::Source {
                title: "Zhou 2019".into(),
            }],
            vec![Reason::Project {
                title: "PhD Thesis".into(),
            }],
            vec![Reason::CoLink {
                title: "Phonon transport".into(),
            }],
            vec![Reason::Terms { count: 1, idf: 4.0 }],
            vec![Reason::Terms { count: 6, idf: 9.0 }],
            vec![tag("sb2se3", 4.0)],
            vec![Reason::Folder],
        ] {
            let line = candidate("x", reasons).explain();
            assert!(!line.is_empty());
            assert!(
                line.chars().next().is_some_and(|c| !c.is_uppercase()),
                "{line}"
            );
        }
    }

    #[test]
    fn the_explanation_gives_the_two_strongest_reasons_and_stops() {
        // A third reason wraps the line, and by then the reader has already
        // decided whether to click.
        let many = candidate(
            "many reasons",
            vec![
                tag("sb2se3", 4.0),
                Reason::Terms { count: 4, idf: 6.0 },
                Reason::CoLink {
                    title: "Phonon transport".into(),
                },
                Reason::Folder,
            ],
        );
        let line = many.explain();
        assert_eq!(line, "shares #sb2se3 and shares 4 distinctive words");
        assert!(!line.contains("Phonon"), "{line}");
        assert!(!line.contains("folder"), "{line}");
    }

    #[test]
    fn one_shared_word_is_singular() {
        let one = candidate("x", vec![Reason::Terms { count: 1, idf: 5.0 }]);
        assert_eq!(one.explain(), "shares a distinctive word");
        let two = candidate("x", vec![Reason::Terms { count: 2, idf: 5.0 }]);
        assert_eq!(two.explain(), "shares 2 distinctive words");
    }

    #[test]
    fn ranking_is_stable_between_runs() {
        // A panel whose rows shuffle when nothing changed is one nobody can
        // use — the eye learns positions, and moving them costs the reader
        // every time.
        let make = || {
            vec![
                candidate("Beta", vec![tag("a", 2.0)]),
                candidate("Alpha", vec![tag("b", 2.0)]),
                candidate("Gamma", vec![tag("c", 3.0)]),
            ]
        };
        let first: Vec<String> = rank(make(), 5).into_iter().map(|r| r.title).collect();
        let again: Vec<String> = rank(make(), 5).into_iter().map(|r| r.title).collect();
        assert_eq!(first, again);
        // Equal scores fall back to the title, not to hash order.
        assert_eq!(first, ["Gamma", "Alpha", "Beta"]);
    }

    #[test]
    fn terms_drops_what_could_never_be_distinctive() {
        let found = terms("The DSC run ramped 300–800 K at 10 K/min, with Sb2Se3 and a 2nd sample");
        // Stopwords, bare numbers and anything under three characters.
        assert!(!found.contains(&"the".to_string()));
        assert!(!found.contains(&"with".to_string()));
        assert!(!found.contains(&"300".to_string()));
        assert!(!found.contains(&"10".to_string()));
        assert!(!found.contains(&"at".to_string()));
        // But the words that matter survive, lowercased.
        assert!(found.contains(&"dsc".to_string()));
        assert!(found.contains(&"ramped".to_string()));
        assert!(found.contains(&"sb2se3".to_string()));
        // And "2nd" is a word, not a number.
        assert!(found.contains(&"2nd".to_string()));
    }

    #[test]
    fn terms_are_distinct_and_keep_the_order_they_appear_in() {
        assert_eq!(
            terms("selenide ribbons and selenide chains"),
            ["selenide", "ribbons", "chains"]
        );
    }
}
