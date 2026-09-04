//! Tags: what a note is about, as opposed to where it lives.
//!
//! Two jobs live here. Normalising, so that the same tag typed two ways is one
//! tag — this is the only defence against the tag explosion section 11 warns
//! about, and it has to happen before a tag reaches disk. And spotting tags
//! that look like they were meant to be the same, which is offered to the user
//! and never applied.

use serde::Serialize;
use std::collections::HashMap;

/// Beyond this many distinct tags, only the exact-after-punctuation check runs.
///
/// Pairwise comparison is quadratic. A vault with a few hundred tags costs
/// nothing; one with tens of thousands would stall the UI, and a vault in that
/// state has problems this feature cannot fix anyway.
const PAIRWISE_LIMIT: usize = 2_000;

/// Shortest tag worth checking for a one-character typo.
///
/// Below this, single-character differences are usually two real tags: `xrd`
/// and `xrf` are different techniques, not a slip.
const TYPO_FLOOR: usize = 5;

/// Put a tag in the one form it is stored in.
///
/// Returns `None` for anything that normalises to nothing, so `#`, `///` and a
/// stray space cannot become tags.
///
/// Hierarchy is slashes: `#Research/Materials/Sb2Se3` is three levels, and each
/// level is normalised on its own. Nothing here invents structure — a tag with
/// no slash stays a tag with no slash.
pub fn normalise(raw: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    for segment in raw.split('/') {
        let mut out = String::with_capacity(segment.len());
        let mut pending_dash = false;

        for ch in segment.chars() {
            if ch.is_whitespace() || ch == '-' || ch == '_' {
                // A run of spacing becomes one dash, and only once real text
                // follows — which trims a trailing dash without a second pass.
                pending_dash = !out.is_empty();
            } else if ch.is_control() || matches!(ch, '#' | ',' | '"' | '\'' | '`') {
                // Dropped rather than treated as a break: `#tag` is `tag`, and
                // someone's `don't` should not become `don-t`.
                continue;
            } else {
                if pending_dash {
                    out.push('-');
                    pending_dash = false;
                }
                out.extend(ch.to_lowercase());
            }
        }

        if !out.is_empty() {
            parts.push(out);
        }
    }

    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Normalise a list, dropping what normalises to nothing and de-duplicating
/// while keeping the order the user typed.
pub fn normalise_all<I: IntoIterator<Item = S>, S: AsRef<str>>(tags: I) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let Some(tag) = normalise(tag.as_ref()) else {
            continue;
        };
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// A tag and how many notes carry it.
type Tagged<'a> = (&'a str, usize);

/// Two tags that look like they were meant to be one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// The rarer of the two — the one worth merging away.
    pub from: String,
    pub from_count: usize,
    /// The commoner one.
    pub into: String,
    pub into_count: usize,
    /// Shown to the user verbatim, so they can judge rather than trust.
    pub reason: String,
}

/// Find pairs of tags that differ only in ways that are usually accidents.
///
/// Deliberately two narrow, explainable rules rather than a similarity score.
/// A researcher deciding whether to merge two tags needs to know *why* they
/// were put in front of each other, and "0.87" does not tell them.
///
/// Nothing here changes anything. Every pair is a question.
pub fn similar(counts: &HashMap<String, usize>) -> Vec<Suggestion> {
    let mut tags: Vec<Tagged<'_>> = counts.iter().map(|(t, c)| (t.as_str(), *c)).collect();
    // Sorted so the output is stable rather than dependent on hash order.
    tags.sort_by(|a, b| a.0.cmp(b.0));

    let mut found: Vec<Suggestion> = Vec::new();

    // Rule one: identical once punctuation is removed. `thermal-conductivity`
    // and `thermalconductivity` are the same tag typed two ways.
    let mut by_squashed: HashMap<String, Vec<Tagged<'_>>> = HashMap::new();
    for (tag, count) in &tags {
        by_squashed
            .entry(squash(tag))
            .or_default()
            .push((tag, *count));
    }
    let mut groups: Vec<_> = by_squashed
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .collect();
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, group) in groups {
        for pair in pairs(&group) {
            found.push(suggest(pair, "the same tag with different punctuation"));
        }
    }

    // Rule two: one character apart. Only for tags long enough that a single
    // difference is more likely a slip than a distinction.
    if tags.len() <= PAIRWISE_LIMIT {
        for i in 0..tags.len() {
            for j in (i + 1)..tags.len() {
                let (a, b) = (squash(tags[i].0), squash(tags[j].0));
                if a == b {
                    continue; // Already reported by rule one.
                }
                if a.chars().count() < TYPO_FLOOR || b.chars().count() < TYPO_FLOOR {
                    continue;
                }
                if transposed(&a, &b) {
                    found.push(suggest((tags[i], tags[j]), "two letters swapped"));
                } else if one_edit_apart(&a, &b) {
                    found.push(suggest((tags[i], tags[j]), "one character apart"));
                }
            }
        }
    }

    found
}

fn suggest(((a, ac), (b, bc)): (Tagged<'_>, Tagged<'_>), reason: &str) -> Suggestion {
    // The rarer tag is the one to merge away, so the pair is swapped when `a`
    // is the commoner of the two. Ties break on the name, so a suggestion does
    // not flip between runs.
    let swap = ac > bc || (ac == bc && a < b);
    let ((from, from_count), (into, into_count)) = if swap {
        ((b, bc), (a, ac))
    } else {
        ((a, ac), (b, bc))
    };
    Suggestion {
        from: from.to_string(),
        from_count,
        into: into.to_string(),
        into_count,
        reason: reason.to_string(),
    }
}

fn pairs<'a>(group: &'a [Tagged<'a>]) -> Vec<(Tagged<'a>, Tagged<'a>)> {
    let mut out = Vec::new();
    for i in 0..group.len() {
        for j in (i + 1)..group.len() {
            out.push((group[i], group[j]));
        }
    }
    out
}

/// Only the letters and digits, so punctuation and hierarchy fall away.
fn squash(tag: &str) -> String {
    tag.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// True when one insertion, deletion or substitution turns `a` into `b`.
///
/// Not a full edit distance — the answer is only ever needed as "is it one?",
/// and stopping at one is both faster and easier to be sure of.
fn one_edit_apart(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (longer, shorter) = if a.len() >= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    if longer.len() - shorter.len() > 1 {
        return false;
    }

    let mut i = 0;
    let mut j = 0;
    let mut edited = false;
    while i < longer.len() && j < shorter.len() {
        if longer[i] == shorter[j] {
            i += 1;
            j += 1;
            continue;
        }
        if edited {
            return false;
        }
        edited = true;
        if longer.len() == shorter.len() {
            // A substitution: step over both.
            i += 1;
            j += 1;
        } else {
            // A deletion from the longer one: step over it alone.
            i += 1;
        }
    }
    // Reaching here means at most one edit was consumed, and whatever is left
    // over is at most the single trailing character of an insertion.
    true
}

/// True when `a` and `b` differ only by two adjacent letters being swapped.
///
/// Its own check because a transposition is two substitutions by the walk
/// above, and "thermodynamcis" is one of the commonest slips there is — the
/// exact case section 11 is worried about.
fn transposed(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len() != b.len() {
        return false;
    }
    let differing: Vec<usize> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
    matches!(differing[..], [i, j] if j == i + 1 && a[i] == b[j] && a[j] == b[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_lowercased_and_stripped_of_its_hash() {
        assert_eq!(normalise("#Sb2Se3").as_deref(), Some("sb2se3"));
        assert_eq!(normalise("  CVT  ").as_deref(), Some("cvt"));
    }

    #[test]
    fn spacing_becomes_a_single_dash() {
        assert_eq!(
            normalise("thermal   conductivity").as_deref(),
            Some("thermal-conductivity")
        );
        assert_eq!(
            normalise("thermal_conductivity").as_deref(),
            Some("thermal-conductivity")
        );
        assert_eq!(normalise("  trailing  ").as_deref(), Some("trailing"));
    }

    #[test]
    fn slashes_are_hierarchy_and_each_level_is_normalised() {
        assert_eq!(
            normalise("#Research / Materials / Sb2Se3").as_deref(),
            Some("research/materials/sb2se3")
        );
        assert_eq!(normalise("a//b").as_deref(), Some("a/b"));
        assert_eq!(normalise("/leading/").as_deref(), Some("leading"));
    }

    #[test]
    fn a_tag_that_normalises_to_nothing_is_not_a_tag() {
        for raw in ["", "   ", "#", "///", "#/#/#", "\"'`"] {
            assert_eq!(normalise(raw), None, "{raw:?} should not be a tag");
        }
    }

    #[test]
    fn an_apostrophe_does_not_split_a_word() {
        assert_eq!(normalise("don't").as_deref(), Some("dont"));
    }

    #[test]
    fn non_ascii_letters_survive() {
        assert_eq!(
            normalise("Kristallögraphie").as_deref(),
            Some("kristallögraphie")
        );
    }

    #[test]
    fn normalising_a_list_dedupes_and_keeps_order() {
        let out = normalise_all([" CVT ", "cvt", "#Sb2Se3", "", "cvt"]);
        assert_eq!(out, vec!["cvt", "sb2se3"]);
    }

    fn counts(pairs: &[(&str, usize)]) -> HashMap<String, usize> {
        pairs.iter().map(|(t, c)| (t.to_string(), *c)).collect()
    }

    #[test]
    fn punctuation_only_differences_are_offered() {
        let found = similar(&counts(&[
            ("thermal-conductivity", 12),
            ("thermalconductivity", 1),
            ("cvt", 5),
        ]));
        assert_eq!(found.len(), 1, "{found:?}");
        // The rarer one is the one to merge away.
        assert_eq!(found[0].from, "thermalconductivity");
        assert_eq!(found[0].into, "thermal-conductivity");
        assert!(found[0].reason.contains("punctuation"));
    }

    #[test]
    fn a_one_character_typo_is_offered() {
        let found = similar(&counts(&[("thermodynamics", 20), ("thermodynamcis", 1)]));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].from, "thermodynamcis");
        assert!(found[0].reason.contains("swapped"), "{:?}", found[0].reason);
    }

    #[test]
    fn short_tags_one_letter_apart_are_left_alone() {
        // xrd and xrf are different techniques, not a slip.
        assert!(similar(&counts(&[("xrd", 9), ("xrf", 4)])).is_empty());
    }

    #[test]
    fn unrelated_tags_are_not_offered() {
        let found = similar(&counts(&[
            ("thermodynamics", 3),
            ("crystallography", 3),
            ("sb2se3", 3),
        ]));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_pair_is_offered_once_not_twice() {
        let found = similar(&counts(&[("sb2-se3", 2), ("sb2se3", 9)]));
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_dropped_letter_is_offered() {
        let found = similar(&counts(&[("thermodynamics", 20), ("thermodynamic", 1)]));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].reason.contains("one character"),
            "{:?}",
            found[0].reason
        );
    }

    #[test]
    fn a_transposition_is_recognised() {
        assert!(transposed("thermodynamics", "thermodynamcis"));
        assert!(
            !transposed("thermodynamics", "thermodynamics"),
            "identical is not a swap"
        );
        assert!(!transposed("abcd", "badc"), "two separate swaps is not one");
        assert!(!transposed("abc", "abcd"), "different lengths");
    }

    #[test]
    fn one_edit_recognises_each_kind_of_slip() {
        assert!(one_edit_apart("thermodynamics", "thermodynamic")); // deletion
        assert!(one_edit_apart("thermodynamic", "thermodynamics")); // insertion
        assert!(one_edit_apart("thermodynamics", "thermodynamacs")); // substitution
        assert!(one_edit_apart("crystal", "crystal")); // nothing at all
        assert!(!one_edit_apart("thermodynamics", "crystallography"));
        assert!(!one_edit_apart("abcdef", "abcdefgh")); // two apart
    }
}
