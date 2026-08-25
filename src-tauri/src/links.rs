//! Finding `[[id]]` references in a note's markdown.

/// A ULID is 26 characters of Crockford base32: digits and uppercase letters,
/// excluding I, L, O and U. We accept the looser digit/uppercase set and let
/// the index decide whether the target actually exists — a link to a note that
/// was deleted is still a link, and telling the user it dangles is more useful
/// than silently dropping it.
fn looks_like_ulid(candidate: &str) -> bool {
    candidate.len() == 26
        && candidate
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
}

/// Every distinct `[[id]]` in `body`, in order of first appearance.
///
/// This is deliberately a scan over raw text rather than a markdown parse.
/// Rust does not interpret note bodies — that belongs to the editor — and for
/// the index we only need to know which ids are mentioned. A scan cannot
/// disagree with the editor about document structure because it never forms an
/// opinion about it.
///
/// A link inside a fenced code block is still reported. That is a knowing
/// trade: recognising fences here would mean parsing markdown after all, and
/// an extra backlink is a far smaller problem than a missing one.
pub fn extract(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = body;

    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let candidate = &after[..end];

        if looks_like_ulid(candidate) && !found.iter().any(|f| f == candidate) {
            found.push(candidate.to_string());
        }
        rest = &after[end + 2..];
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "01HQ3M8K2P0000000000000001";
    const B: &str = "01HQ3M8K2P0000000000000002";

    #[test]
    fn finds_a_single_link() {
        assert_eq!(extract(&format!("See [[{A}]] for context.")), vec![A]);
    }

    #[test]
    fn finds_several_in_order() {
        let body = format!("[[{B}]] then [[{A}]]");
        assert_eq!(extract(&body), vec![B, A]);
    }

    #[test]
    fn reports_each_target_once() {
        let body = format!("[[{A}]] and again [[{A}]]");
        assert_eq!(extract(&body), vec![A]);
    }

    #[test]
    fn ignores_things_that_are_not_ids() {
        assert!(extract("[[not an id]]").is_empty());
        assert!(extract("[[]]").is_empty());
        assert!(extract("[[short]]").is_empty());
        // Lowercase is not Crockford base32.
        assert!(extract("[[01hq3m8k2p0000000000000001]]").is_empty());
    }

    #[test]
    fn ignores_unclosed_brackets() {
        assert!(extract(&format!("[[{A}")).is_empty());
    }

    #[test]
    fn a_single_bracket_is_not_a_link() {
        // Miller indices must not be mistaken for links.
        assert!(extract("Ribbons along [001] and [hk0].").is_empty());
    }

    #[test]
    fn survives_adjacent_links() {
        let body = format!("[[{A}]][[{B}]]");
        assert_eq!(extract(&body), vec![A, B]);
    }
}
