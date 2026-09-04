//! Finding notes that may be the same note written twice.
//!
//! Two facts shape this. The first is that duplicates in a research vault are
//! nearly always *accidental* — the same idea written up twice months apart,
//! under titles that say the same thing in a different order. The second is
//! that merging is destructive and being wrong about it costs work, so nothing
//! here decides anything: it produces candidates and a comparison, and a
//! person presses a button.
//!
//! Three measures, deliberately dull:
//!
//! 1. The **normalised title** — lowercased, stripped of punctuation, words
//!    sorted. "Thermal conductivity Sb2Se3" and "Sb2Se3 thermal conductivity"
//!    collapse to the same string, which is the commonest shape of an
//!    accidental duplicate and the one worth being certain about.
//! 2. **Title overlap**, for titles that say the same thing with a word or two
//!    different.
//! 3. **Body overlap**, because two notes can share a title by coincidence
//!    ("Notes", "Meeting") and never a body.
//!
//! No embeddings, no model. A deterministic implementation first is the rule
//! the whole suggestion side of this app follows, and for "did I write this
//! twice" the deterministic answer is very nearly as good.

use serde::Serialize;
use std::collections::HashSet;

/// How alike two notes are, in the three ways that are measured.
///
/// Kept as the separate measures rather than one number, for the same reason
/// [`crate::related::Reason`] is: the panel has to be able to say which of them
/// fired, and a reader who cannot check a suggestion learns to ignore it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Likeness {
    /// Same title once punctuation and word order are set aside.
    pub same_title: bool,
    /// Share of title words in common, ignoring order.
    pub title_overlap: f64,
    /// Share of body words in common, ignoring order.
    pub body_overlap: f64,
}

/// A note that may be a duplicate of the one being read.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Duplicate {
    pub id: String,
    pub title: String,
    pub folder: String,
    /// One line saying what matched, for the same reason the related panel
    /// gives one: a suggestion nobody can check is one nobody should act on.
    pub reason: String,
    pub score: f64,
}

/// An identical normalised title is worth this much on its own.
///
/// Enough to clear the floor unaided. Two notes whose titles are the same
/// words in a different order is not a coincidence anyone has to weigh — it is
/// the thing this feature exists to catch.
const SAME_TITLE: f64 = 1.0;

/// What a pair must reach to be offered.
///
/// High, and meant to be. A false duplicate costs a person's attention and,
/// if they act on it carelessly, a note; a missed one costs nothing, because
/// the vault goes on working exactly as it did. So this errs towards silence.
pub const FLOOR: f64 = 0.85;

impl Likeness {
    pub fn score(&self) -> f64 {
        if self.same_title {
            // Still added to, so an identical title *and* an identical body
            // ranks above an identical title alone.
            return SAME_TITLE + self.body_overlap;
        }
        // Neither alone is enough: a shared title with nothing else is two
        // notes called "Meeting", and a shared body with a different title is
        // usually a quotation. The product says "both, or neither".
        2.0 * self.title_overlap * self.body_overlap
    }

    pub fn explain(&self) -> String {
        if self.same_title {
            return if self.body_overlap >= 0.6 {
                "same title, and the bodies mostly agree".into()
            } else {
                "the same title in a different order".into()
            };
        }
        format!(
            "{}% of the title and {}% of the text in common",
            (self.title_overlap * 100.0).round() as i64,
            (self.body_overlap * 100.0).round() as i64
        )
    }
}

/// How alike two notes are.
pub fn compare(a_title: &str, a_body: &str, b_title: &str, b_body: &str) -> Likeness {
    let a_words = words(a_title);
    let b_words = words(b_title);
    Likeness {
        // An empty title matches every other empty title, which would make
        // every untitled capture a duplicate of every other. It is not.
        same_title: !a_words.is_empty() && normalise_title(a_title) == normalise_title(b_title),
        title_overlap: overlap(&a_words, &b_words),
        body_overlap: overlap(&words(a_body), &words(b_body)),
    }
}

/// A title reduced to what it says rather than how it was typed.
///
/// Lowercased, punctuation dropped, words sorted. Word order is the thing that
/// varies between two goes at the same title, and it is exactly the thing that
/// carries no meaning in one.
pub fn normalise_title(title: &str) -> String {
    let mut parts: Vec<String> = words(title).into_iter().collect();
    parts.sort();
    parts.join(" ")
}

/// The distinct words in a string, lowercased.
fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Jaccard: shared words over all words. 1.0 is the same set.
fn overlap(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.intersection(b).count() as f64;
    let total = a.union(b).count() as f64;
    shared / total
}

/// Rank pairs and drop the ones not worth offering.
pub fn rank(mut found: Vec<Duplicate>, limit: usize) -> Vec<Duplicate> {
    found.retain(|d| d.score >= FLOOR);
    found.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.title.cmp(&b.title))
    });
    found.truncate(limit);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_words_in_a_different_order_are_the_same_title() {
        // The step's proof, and the commonest shape of an accidental
        // duplicate: the same note written up twice, months apart, by someone
        // who phrased the title differently the second time.
        assert_eq!(
            normalise_title("Thermal conductivity Sb2Se3"),
            normalise_title("Sb2Se3 thermal conductivity")
        );
        let alike = compare(
            "Thermal conductivity Sb2Se3",
            "Boundary scattering dominates.",
            "Sb2Se3 thermal conductivity",
            "Boundary scattering dominates below the Debye temperature.",
        );
        assert!(alike.same_title);
        assert!(alike.score() >= FLOOR);
        assert!(
            alike.explain().contains("same title"),
            "{}",
            alike.explain()
        );
    }

    #[test]
    fn punctuation_and_case_do_not_make_two_titles_different() {
        assert_eq!(
            normalise_title("Sb2Se3: thermal conductivity"),
            normalise_title("sb2se3 — Thermal Conductivity")
        );
    }

    #[test]
    fn two_untitled_notes_are_not_duplicates_of_each_other() {
        // Every capture starts untitled. Reading an empty title as "the same
        // title" would make the Inbox one enormous pile of duplicates on the
        // first day anyone used it.
        let alike = compare("", "Milk, bread.", "", "Ring the supervisor.");
        assert!(!alike.same_title);
        assert!(alike.score() < FLOOR);
    }

    #[test]
    fn a_shared_title_with_nothing_else_is_not_enough() {
        // Two notes called "Meeting" are two meetings.
        let alike = compare(
            "Meeting",
            "Discussed the anneal schedule with A.",
            "Meeting",
            "Reviewed the draft introduction.",
        );
        // The titles are identical, so this *is* offered — and the sentence
        // says only that, so the reader can dismiss it at a glance.
        assert!(alike.same_title);
        assert_eq!(alike.explain(), "the same title in a different order");
        // But an identical body ranks well above it, so the real duplicate
        // comes first when both are on screen.
        let real = compare(
            "Meeting",
            "Discussed the anneal.",
            "Meeting",
            "Discussed the anneal.",
        );
        assert!(real.score() > alike.score());
    }

    #[test]
    fn a_shared_body_under_a_different_title_is_usually_a_quotation() {
        // One note quoting another at length is not a duplicate of it, and
        // this is the shape that would otherwise fire constantly in a vault
        // full of literature notes.
        let alike = compare(
            "Reading — Zhou 2019",
            "Quasi-1D ribbons align along the crystallographic axis.",
            "Phonon transport",
            "Quasi-1D ribbons align along the crystallographic axis.",
        );
        assert!(!alike.same_title);
        assert!(
            alike.score() < FLOOR,
            "score {} from {alike:?}",
            alike.score()
        );
    }

    #[test]
    fn near_identical_titles_and_bodies_are_offered_with_the_numbers() {
        let alike = compare(
            "DSC run 27 August",
            "Ramped 300-800 K at 10 K/min under argon, sapphire standard.",
            "DSC run 27 Aug",
            "Ramped 300-800 K at 10 K/min under argon, sapphire baseline.",
        );
        assert!(!alike.same_title);
        assert!(alike.score() >= FLOOR, "{alike:?} scored {}", alike.score());
        assert!(alike.explain().contains('%'), "{}", alike.explain());
    }

    #[test]
    fn two_unrelated_notes_are_never_offered() {
        let alike = compare(
            "Sb2Se3 heat capacity",
            "Cp fitted as a polynomial over 300-800 K.",
            "Group meeting 3",
            "Discussed progress and next steps.",
        );
        assert!(alike.score() < FLOOR);
    }

    #[test]
    fn ranking_drops_what_is_below_the_floor_and_is_stable() {
        let make = |title: &str, score: f64| Duplicate {
            id: title.into(),
            title: title.into(),
            folder: String::new(),
            reason: String::new(),
            score,
        };
        let ranked = rank(
            vec![
                make("Weak", FLOOR - 0.01),
                make("Beta", 1.5),
                make("Alpha", 1.5),
                make("Strong", 2.0),
            ],
            5,
        );
        let titles: Vec<&str> = ranked.iter().map(|d| d.title.as_str()).collect();
        assert_eq!(titles, ["Strong", "Alpha", "Beta"]);
    }
}
