//! Finding the attachments a note's markdown points at.
//!
//! An attachment reference is an ordinary markdown link target — the same
//! string [`crate::vault::Vault::read_attachment`] resolves. Two shapes count:
//! the hidden `.attachments` folder beside a note, and the flat top-level
//! `attachments/` an older vault used.
//!
//! This is a scan over raw text rather than a markdown parse, for the same
//! reason [`crate::links::extract`] is one: Rust does not interpret note
//! bodies, and a scan cannot disagree with the editor about document structure
//! because it never forms an opinion about it. The cost is that a reference
//! inside a fenced code block counts as a reference. That is the safe
//! direction to be wrong in — it can only ever make the app treat a file as
//! *more* referenced than it is, which leaves the file alone.

/// The hidden folder beside a note. Matches [`crate::vault`]'s own constant;
/// the two are pinned together by a test there.
const ATTACHMENTS: &str = ".attachments";
const LEGACY_ATTACHMENTS: &str = "attachments";

/// Whether a link target names a file in an attachments folder.
///
/// Deliberately narrow. An absolute URL, a `[[wikilink]]`, a path climbing out
/// with `..` — none of these are attachments this vault owns, and treating one
/// as ours is how a move or a delete would touch a file it has no business
/// touching.
pub fn is_attachment(target: &str) -> bool {
    if target.is_empty() || target.contains('\\') {
        return false;
    }
    // A scheme, a protocol-relative URL, or an absolute path is somebody
    // else's file.
    if target.starts_with('/') || target.starts_with("//") {
        return false;
    }
    if target.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty() && scheme.chars().all(|c| c.is_alphanumeric())
    }) {
        return false;
    }

    let parts: Vec<&str> = target.split('/').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return false;
    }
    match parts.as_slice() {
        // `attachments/x.png` — and only at the root, which is where the old
        // layout put them.
        [LEGACY_ATTACHMENTS, _] => true,
        // `.../.attachments/x.png`, at any depth including none.
        [.., folder, _] => *folder == ATTACHMENTS,
        _ => false,
    }
}

/// Every distinct attachment reference in `body`, in order of first appearance.
pub fn extract(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = body;

    while let Some(open) = rest.find("](") {
        let after = &rest[open + 2..];
        let Some(close) = after.find(')') else { break };
        let raw = &after[..close];
        rest = &after[close + 1..];

        // `](<path>)` is how markdown writes a target containing spaces, so
        // inside the brackets the whole thing is the path. Outside them a
        // title may follow — `](path "caption")` — and the space ends the
        // target. Getting this the wrong way round truncates a folder whose
        // name has a space in it.
        let target = match raw.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
            Some(bracketed) => bracketed.trim(),
            None => raw.split_whitespace().next().unwrap_or(""),
        };

        if is_attachment(target) && !found.iter().any(|f| f == target) {
            found.push(target.to_string());
        }
    }
    found
}

/// Replace every occurrence of one attachment reference with another.
///
/// Only the exact target is rewritten, and only where it sits in a link. The
/// prose is never touched — a note that happens to *mention* the path in a
/// sentence keeps saying what it said.
pub fn retarget(body: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;

    while let Some(open) = rest.find("](") {
        let (before, after) = rest.split_at(open + 2);
        out.push_str(before);
        let Some(close) = after.find(')') else {
            out.push_str(after);
            return out;
        };
        let raw = &after[..close];
        let angled = raw.starts_with('<') && raw.ends_with('>');
        let inner = if angled { &raw[1..raw.len() - 1] } else { raw };

        if inner.trim() == from {
            if angled {
                out.push('<');
                out.push_str(to);
                out.push('>');
            } else {
                out.push_str(to);
            }
        } else {
            out.push_str(raw);
        }
        out.push(')');
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_an_image_beside_its_note() {
        assert_eq!(
            extract("![plot](Research/Sb2Se3/.attachments/01H_x.png)"),
            vec!["Research/Sb2Se3/.attachments/01H_x.png"]
        );
    }

    #[test]
    fn finds_one_at_the_vault_root() {
        assert_eq!(
            extract("![x](.attachments/01H_x.png)"),
            vec![".attachments/01H_x.png"]
        );
    }

    #[test]
    fn finds_the_old_flat_layout() {
        assert_eq!(
            extract("![x](attachments/old.png)"),
            vec!["attachments/old.png"]
        );
    }

    #[test]
    fn reports_each_reference_once_in_order() {
        let body =
            "![b](R/.attachments/b.png) ![a](R/.attachments/a.png) ![b](R/.attachments/b.png)";
        assert_eq!(
            extract(body),
            vec!["R/.attachments/b.png", "R/.attachments/a.png"]
        );
    }

    #[test]
    fn ignores_everything_that_is_not_ours() {
        for target in [
            "https://example.com/x.png",
            "//example.com/x.png",
            "/etc/passwd",
            "R/.attachments/../../secret.md",
            "../secret.md",
            "Research/Notes.md",
            "deep/attachments/x.png",
            "C:/Windows/x.png",
        ] {
            assert!(
                extract(&format!("![x]({target})")).is_empty(),
                "{target} was treated as an attachment"
            );
        }
    }

    #[test]
    fn reads_an_angle_bracketed_target() {
        assert_eq!(
            extract("![x](<My Notes/.attachments/01H_x.png>)"),
            vec!["My Notes/.attachments/01H_x.png"]
        );
    }

    #[test]
    fn ignores_a_title_after_the_target() {
        assert_eq!(
            extract("![x](R/.attachments/x.png \"a caption\")"),
            vec!["R/.attachments/x.png"]
        );
    }

    #[test]
    fn retargets_only_the_matching_link() {
        let body = "![a](A/.attachments/x.png) and ![b](B/.attachments/y.png)";
        assert_eq!(
            retarget(body, "A/.attachments/x.png", "C/.attachments/x.png"),
            "![a](C/.attachments/x.png) and ![b](B/.attachments/y.png)"
        );
    }

    #[test]
    fn retargets_every_occurrence() {
        let body = "![a](A/.attachments/x.png) then ![a](A/.attachments/x.png)";
        assert_eq!(
            retarget(body, "A/.attachments/x.png", "B/.attachments/x.png"),
            "![a](B/.attachments/x.png) then ![a](B/.attachments/x.png)"
        );
    }

    #[test]
    fn retargeting_leaves_the_prose_alone() {
        // The path in a sentence is not a link, so it stays as written.
        let body = "The file A/.attachments/x.png holds ![it](A/.attachments/x.png).";
        assert_eq!(
            retarget(body, "A/.attachments/x.png", "B/.attachments/x.png"),
            "The file A/.attachments/x.png holds ![it](B/.attachments/x.png)."
        );
    }

    #[test]
    fn retargeting_keeps_angle_brackets() {
        assert_eq!(
            retarget(
                "![x](<A B/.attachments/x.png>)",
                "A B/.attachments/x.png",
                "C/.attachments/x.png"
            ),
            "![x](<C/.attachments/x.png>)"
        );
    }

    #[test]
    fn an_unclosed_link_does_not_lose_the_rest_of_the_note() {
        let body = "![x](A/.attachments/x.png but never closed";
        assert_eq!(retarget(body, "a", "b"), body);
    }
}
