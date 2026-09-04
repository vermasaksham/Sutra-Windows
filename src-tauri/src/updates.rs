//! Telling you that a newer Sutra exists — only when you ask.
//!
//! Sutra cannot update itself yet; that needs a signing key its author has to
//! generate and hold, and `docs/releasing.md` says how. Until then the gap this
//! closes is smaller but real: without it, a fix only reaches you if you happen
//! to visit the releases page, which means every fix is optional by accident.
//!
//! Nothing here runs on its own. The app is local-first and says so in those
//! words, so a background check phoning a server would be a promise broken
//! quietly. This runs when a person presses a button, and not otherwise.

use crate::error::{Result, SutraError};
use serde::{Deserialize, Serialize};

const RELEASES: &str = "https://api.github.com/repos/vermasaksham/Sutra-Windows/releases/latest";

/// What the check found.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// The version running now.
    pub current: String,
    /// The newest version published, if the check reached GitHub.
    pub latest: Option<String>,
    /// True only when `latest` is genuinely newer than `current`.
    pub newer: bool,
    /// Where to get it.
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

/// Compare two versions the way a person would read them.
///
/// Split on dots, compare numerically, and treat a missing part as zero so
/// "0.2" and "0.2.0" are the same version. Anything non-numeric — a "0.2.0-rc1"
/// — compares as the numbers before it and is otherwise ignored, because the
/// only decision being made here is "should this person be told about a new
/// release", and a release candidate is not something to push at someone.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim()
            .trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// The version this binary was built as.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Ask GitHub what the newest release is.
///
/// Every failure returns an error rather than a quiet "you are up to date":
/// being told nothing is wrong when the check never happened is worse than
/// being told the check failed.
pub fn check() -> Result<UpdateStatus> {
    let current = current_version().to_string();
    let fallback = "https://github.com/vermasaksham/Sutra-Windows/releases/latest";

    let response = ureq::get(RELEASES)
        // GitHub rejects requests with no user agent, and a request that
        // identifies itself is the polite thing to send anyway.
        .header("User-Agent", &format!("Sutra/{current}"))
        .header("Accept", "application/vnd.github+json")
        .call();

    let mut response = match response {
        Ok(r) => r,
        Err(e) => {
            return Err(SutraError::Zotero(format!(
                "could not reach GitHub to check for updates: {e}"
            )));
        }
    };

    let release: Release = response
        .body_mut()
        .read_json()
        .map_err(|e| SutraError::Zotero(format!("GitHub sent something unreadable: {e}")))?;

    let latest = release.tag_name.trim_start_matches('v').to_string();
    Ok(UpdateStatus {
        newer: is_newer(&latest, &current),
        url: if release.html_url.is_empty() {
            fallback.to_string()
        } else {
            release.html_url
        },
        latest: Some(latest),
        current,
    })
}

/// Hand a URL to whatever the desktop uses for links.
///
/// Same shape as opening a Zotero item: no dependency, and the caller has
/// already checked the URL is one of ours.
pub fn open(url: &str) -> Result<()> {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
        // The empty string is the window title `start` expects first; omitting
        // it makes `start` treat the URL as the title.
        ("cmd", vec!["/C", "start", "", url])
    } else if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else {
        ("xdg-open", vec![url])
    };

    std::process::Command::new(program)
        .args(&args)
        .spawn()
        .map(|_| ())
        .map_err(|e| SutraError::Zotero(format!("could not open the browser: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_are_recognised() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.0"), "ten is after nine, not before");
    }

    #[test]
    fn the_same_version_is_not_an_update() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        // A tag may or may not carry its v, and "0.2" and "0.2.0" are one
        // version written two ways.
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.2", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2"));
    }

    #[test]
    fn older_versions_are_never_offered() {
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn a_release_candidate_is_not_pushed_at_anyone() {
        // "0.2.0-rc1" reads as 0.2.0, so it is offered to someone on 0.1.0 and
        // not to someone already on 0.2.0. Nobody is nagged to move sideways.
        assert!(is_newer("0.2.0-rc1", "0.1.0"));
        assert!(!is_newer("0.2.0-rc1", "0.2.0"));
    }

    #[test]
    fn nonsense_never_claims_to_be_newer() {
        // A tag that is not a version at all must not read as an update; the
        // parse yields zeroes, and zero is not newer than anything.
        assert!(!is_newer("nightly", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
    }
}
