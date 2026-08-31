//! Finding and rewriting `[@ref]` citations in a note's markdown.
//!
//! Two shapes live in the same syntax, and length tells them apart. Eight
//! characters is a Zotero item key, which is how citations were written before
//! a source became a note in the vault; twenty-six is a source note's ULID,
//! which is how they are written now.
//!
//! The old shape only means something while Zotero is installed and running —
//! open the vault on another machine and every citation resolves to nothing.
//! That is why they are migrated, and why both shapes have to be understood in
//! the meantime.

/// Zotero item keys are eight characters of uppercase base32-ish.
fn looks_like_zotero_key(candidate: &str) -> bool {
    candidate.len() == 8
        && candidate
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
}

/// Every distinct legacy `[@KEY]` in `body`, in order of first appearance.
///
/// A scan over raw text rather than a markdown parse, for the same reason
/// `links::extract` is: Rust does not interpret note bodies, and a scan cannot
/// disagree with the editor about document structure because it never forms an
/// opinion about it.
pub fn legacy_keys(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = body;

    while let Some(start) = rest.find("[@") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(']') else { break };
        let candidate = &after[..end];

        if looks_like_zotero_key(candidate) && !found.iter().any(|f| f == candidate) {
            found.push(candidate.to_string());
        }
        rest = &after[end + 1..];
    }

    found
}

/// Every distinct `[@ref]` in `body`, whatever shape, in order.
///
/// Unlike [`legacy_keys`] this does not judge what a reference looks like — it
/// reports what is written. Used to check generated text against the vault,
/// where the question is not "is this a plausible key" but "does this note
/// exist here", and a made-up reference that happens to look well-formed is
/// exactly the thing being caught.
pub fn all_refs(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = body;

    while let Some(start) = rest.find("[@") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(']') else { break };
        let candidate = &after[..end];

        if !candidate.is_empty() && !found.iter().any(|f| f == candidate) {
            found.push(candidate.to_string());
        }
        rest = &after[end + 1..];
    }

    found
}

/// Remove every `[@ref]` naming something not in `known`.
///
/// The whole reference goes, brackets included, rather than being left as
/// broken text: a citation that resolves to nothing is worse than no citation,
/// because it looks like provenance and is not.
pub fn drop_unknown(body: &str, known: &[String]) -> (String, Vec<String>) {
    let mut removed = Vec::new();
    let mut out = body.to_string();
    for reference in all_refs(body) {
        if known.contains(&reference) {
            continue;
        }
        out = out.replace(&format!("[@{reference}]"), "");
        removed.push(reference);
    }
    (out, removed)
}

/// Replace every `[@from]` with `[@to]`.
///
/// The whole reference is matched, brackets included, so a key appearing as
/// ordinary prose somewhere in the note is left alone. Returns the body
/// unchanged when there is nothing to do, which is what lets the caller skip
/// writing the file at all.
pub fn rewrite(body: &str, from: &str, to: &str) -> String {
    body.replace(&format!("[@{from}]"), &format!("[@{to}]"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_found_in_order_and_once_each() {
        let body = "As [@ABCD1234] shows, and again [@ABCD1234], unlike [@ZZZZ9999].";
        assert_eq!(legacy_keys(body), vec!["ABCD1234", "ZZZZ9999"]);
    }

    #[test]
    fn a_source_note_citation_is_not_a_legacy_one() {
        // Twenty-six characters is a ULID: already migrated, leave it be.
        let body = "See [@01HQ3M8K2P00000000000000A1] for the ribbon data.";
        assert!(legacy_keys(body).is_empty());
    }

    #[test]
    fn things_that_only_look_like_citations_are_ignored() {
        for body in [
            "an email like a@b.com",
            "[@lowercase]",
            "[@TOO_LONG_FOR_A_KEY]",
            "[@SHORT]",
            "[@ABCD1234 unclosed",
            "[@]",
        ] {
            assert!(legacy_keys(body).is_empty(), "{body:?}");
        }
    }

    #[test]
    fn an_unclosed_bracket_does_not_hang_the_scan() {
        assert!(legacy_keys("[@[@[@").is_empty());
    }

    #[test]
    fn rewriting_replaces_every_occurrence_and_nothing_else() {
        let body = "See [@ABCD1234] and [@ABCD1234]. The word ABCD1234 stays.";
        let out = rewrite(body, "ABCD1234", "01HQ3M8K2P00000000000000A1");
        assert_eq!(
            out,
            "See [@01HQ3M8K2P00000000000000A1] and [@01HQ3M8K2P00000000000000A1]. \
             The word ABCD1234 stays."
        );
    }

    #[test]
    fn rewriting_a_key_that_is_absent_changes_nothing() {
        let body = "See [@ZZZZ9999].";
        assert_eq!(rewrite(body, "ABCD1234", "X"), body);
    }
}
