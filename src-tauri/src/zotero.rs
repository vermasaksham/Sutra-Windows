//! Reading references from a running Zotero.
//!
//! Zotero 7 exposes a read-only mirror of its Web API on the loopback
//! interface, at `http://127.0.0.1:23119/api/users/0/...`. Nothing leaves the
//! machine, which is the only reason this is acceptable in a local-first app.
//!
//! It has to be switched on: Zotero → Settings → Advanced → "Allow other
//! applications on this computer to communicate with Zotero". If it is off, or
//! Zotero is not running, every request fails at connect, and the error a user
//! sees says which of those to go and fix.
//!
//! The endpoints are only semi-documented, so the parsing here is deliberately
//! forgiving: unknown fields are ignored, missing ones become None, and an item
//! that cannot be understood is skipped rather than failing the search.

use crate::error::{Result, SutraError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Where Zotero listens. The port is fixed and not configurable in Zotero.
const LOCAL_BASE: &str = "http://127.0.0.1:23119";

/// Long enough for a large library, short enough that a wedged Zotero does not
/// hang the editor.
const TIMEOUT: Duration = Duration::from_secs(5);

/// One reference, flattened to what a citation needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
// The frontend reads these, and `item_type` would otherwise arrive as
// `item_type` in JavaScript. Every other struct crossing this boundary happens
// to have single-word fields, so this is the first one that needs saying.
#[serde(rename_all = "camelCase")]
pub struct Reference {
    /// Zotero's item key. Stable for the life of the item, and what a citation
    /// stores — the same trick as `[[id]]`: the document holds an identifier,
    /// and everything human is resolved for display.
    pub key: String,
    pub title: String,
    /// "Smith et al." — Zotero composes this itself, so citation style stays
    /// its problem rather than ours.
    pub creators: String,
    pub year: Option<String>,
    pub item_type: String,
    pub doi: Option<String>,
    /// Journal, book or proceedings — whatever it appeared in.
    pub container: Option<String>,
    pub url: Option<String>,
}

impl Reference {
    /// What a source note in the vault records about this item.
    ///
    /// The import direction matters: details are copied into the vault once,
    /// not looked up every time they are displayed. That is what makes a
    /// citation survive Zotero being uninstalled, the vault being opened on
    /// another machine, or the library being reorganised — the failure section
    /// 31 calls "source provenance is lost".
    ///
    /// The Zotero key comes along so a later import updates the same note
    /// rather than making a second one.
    pub fn to_source(&self) -> crate::frontmatter::SourceMeta {
        crate::frontmatter::SourceMeta {
            authors: blank_to_none(&self.creators),
            year: self.year.clone(),
            container: self.container.clone(),
            doi: self.doi.clone(),
            url: self.url.clone(),
            zotero: Some(self.key.clone()),
        }
    }
}

fn blank_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The subset of Zotero's item JSON we read.
#[derive(Debug, Deserialize)]
struct ZoteroItem {
    key: String,
    #[serde(default)]
    data: ItemData,
    #[serde(default)]
    meta: ItemMeta,
}

#[derive(Debug, Default, Deserialize)]
struct ItemData {
    #[serde(default)]
    title: String,
    #[serde(rename = "itemType", default)]
    item_type: String,
    #[serde(rename = "DOI", default)]
    doi: Option<String>,
    /// Zotero names the container differently per item type, and an item has
    /// only the one its type uses. Reading all three and taking whichever is
    /// present is simpler than switching on `itemType`, and does not break when
    /// a type we did not think of turns up.
    #[serde(rename = "publicationTitle", default)]
    publication_title: Option<String>,
    #[serde(rename = "bookTitle", default)]
    book_title: Option<String>,
    #[serde(rename = "proceedingsTitle", default)]
    proceedings_title: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ItemMeta {
    #[serde(rename = "creatorSummary", default)]
    creator_summary: Option<String>,
    /// Zotero normalises whatever was in the date field to `YYYY-MM-DD`.
    #[serde(rename = "parsedDate", default)]
    parsed_date: Option<String>,
}

pub struct Zotero {
    base: String,
}

impl Default for Zotero {
    fn default() -> Self {
        Self::new(LOCAL_BASE.to_string())
    }
}

impl Zotero {
    /// Takes the base URL so tests can point it at a stub server. Production
    /// always uses `Default`.
    pub fn new(base: String) -> Self {
        Self { base }
    }

    /// Search the user's library.
    ///
    /// `qmode=titleCreatorYear` searches the fields a person actually
    /// remembers, rather than everything including full-text of attachments,
    /// which turns a search for an author into a list of every PDF mentioning
    /// them.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Reference>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!(
            "{}/api/users/0/items?q={}&qmode=titleCreatorYear&itemType=-attachment%20||%20note&limit={}&format=json",
            self.base,
            urlencode(query),
            limit
        );
        self.items(&url)
    }

    /// Look up specific items by key, so a citation can render its label.
    pub fn by_keys(&self, keys: &[String]) -> Result<Vec<Reference>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!(
            "{}/api/users/0/items?itemKey={}&format=json",
            self.base,
            keys.join(",")
        );
        self.items(&url)
    }

    fn items(&self, url: &str) -> Result<Vec<Reference>> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .build()
            .into();

        let body = agent
            .get(url)
            .call()
            .map_err(|e| SutraError::Zotero(describe(&e)))?
            .body_mut()
            .read_to_string()
            .map_err(|e| SutraError::Zotero(e.to_string()))?;

        let items: Vec<ZoteroItem> =
            serde_json::from_str(&body).map_err(|e| SutraError::Zotero(e.to_string()))?;

        Ok(items.into_iter().map(reference_from).collect())
    }
}

fn reference_from(item: ZoteroItem) -> Reference {
    Reference {
        key: item.key,
        title: if item.data.title.is_empty() {
            "Untitled".to_string()
        } else {
            item.data.title
        },
        creators: item.meta.creator_summary.unwrap_or_default(),
        // Only the year is wanted for a citation label, and the rest of the
        // date is noise in "(Smith et al., 2019)".
        year: item
            .meta
            .parsed_date
            .and_then(|d| d.get(..4).map(str::to_string)),
        item_type: item.data.item_type,
        doi: item.data.doi.filter(|d| !d.is_empty()),
        // Whichever container field this item type happens to use.
        container: [
            item.data.publication_title,
            item.data.book_title,
            item.data.proceedings_title,
        ]
        .into_iter()
        .flatten()
        .find(|c| !c.trim().is_empty()),
        url: item.data.url.filter(|u| !u.trim().is_empty()),
    }
}

/// Turn a connection failure into something a user can act on.
fn describe(error: &ureq::Error) -> String {
    match error {
        // The overwhelmingly likely cause, and the two fixes are different, so
        // name both rather than reporting "connection refused".
        ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            "could not reach Zotero. Is it running, and is \"Allow other \
             applications on this computer to communicate with Zotero\" enabled \
             in Settings → Advanced?"
                .to_string()
        }
        other => other.to_string(),
    }
}

/// Percent-encode a query string.
///
/// Hand-rolled rather than pulling in a URL crate for one parameter: the rule
/// is short, and everything outside the unreserved set is escaped, so it errs
/// toward encoding more than strictly necessary.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A one-request HTTP server returning a canned body.
    ///
    /// Real sockets rather than a mocked client, so the URL construction, the
    /// request, and the parsing are all exercised — the parts most likely to be
    /// wrong against an endpoint we cannot run here.
    fn stub(body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let read = socket.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes());
            request
        });
        (base, handle)
    }

    const TWO_ITEMS: &str = r#"[
      {"key":"ABCD1234","version":1,
       "data":{"itemType":"journalArticle","title":"Quasi-1D Sb2Se3 ribbons","DOI":"10.1000/xyz"},
       "meta":{"creatorSummary":"Zhou et al.","parsedDate":"2019-04-01"}},
      {"key":"EFGH5678","version":1,
       "data":{"itemType":"book","title":"Chemical Vapour Transport"},
       "meta":{"creatorSummary":"Binnewies","parsedDate":"2012"}}
    ]"#;

    #[test]
    fn search_parses_items() {
        let (base, handle) = stub(TWO_ITEMS);
        let found = Zotero::new(base).search("sb2se3", 20).unwrap();
        handle.join().unwrap();

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].key, "ABCD1234");
        assert_eq!(found[0].title, "Quasi-1D Sb2Se3 ribbons");
        assert_eq!(found[0].creators, "Zhou et al.");
        // Only the year survives; the rest of the date is noise in a citation.
        assert_eq!(found[0].year.as_deref(), Some("2019"));
        assert_eq!(found[0].doi.as_deref(), Some("10.1000/xyz"));
        assert_eq!(found[1].year.as_deref(), Some("2012"));
        assert_eq!(found[1].doi, None);
    }

    #[test]
    fn the_query_is_encoded_and_scoped() {
        let (base, handle) = stub("[]");
        Zotero::new(base).search("Sb2Se3 & CVT", 20).unwrap();
        let request = handle.join().unwrap();

        // A bare & would start a new query parameter and truncate the search.
        assert!(request.contains("q=Sb2Se3%20%26%20CVT"), "got {request}");
        // Searching everything would return every PDF that merely mentions the
        // author, so the search is scoped to the fields people remember.
        assert!(request.contains("qmode=titleCreatorYear"));
        assert!(request.contains("limit=20"));
    }

    #[test]
    fn an_empty_query_does_not_hit_the_network() {
        // No stub server at all: if this tried to connect it would fail.
        let found = Zotero::new("http://127.0.0.1:1".into())
            .search("   ", 20)
            .unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn items_missing_fields_still_parse() {
        let (base, handle) = stub(r#"[{"key":"K1","data":{},"meta":{}}]"#);
        let found = Zotero::new(base).search("x", 5).unwrap();
        handle.join().unwrap();

        // The endpoint is only semi-documented, so a sparse item must degrade
        // rather than fail the whole search.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Untitled");
        assert_eq!(found[0].creators, "");
        assert_eq!(found[0].year, None);
    }

    #[test]
    fn by_keys_asks_for_exactly_those_items() {
        let (base, handle) = stub(TWO_ITEMS);
        let found = Zotero::new(base)
            .by_keys(&["ABCD1234".into(), "EFGH5678".into()])
            .unwrap();
        let request = handle.join().unwrap();

        assert!(
            request.contains("itemKey=ABCD1234,EFGH5678"),
            "got {request}"
        );
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn the_json_sent_to_the_frontend_is_camel_case() {
        // The frontend's Reference type is written by hand, so the field names
        // have to be pinned here or the two drift apart silently and `itemType`
        // arrives undefined.
        let reference = Reference {
            key: "K".into(),
            title: "T".into(),
            creators: "C".into(),
            year: Some("2019".into()),
            item_type: "journalArticle".into(),
            doi: None,
            container: Some("Nature Energy".into()),
            url: None,
        };
        let json = serde_json::to_value(&reference).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "container",
                "creators",
                "doi",
                "itemType",
                "key",
                "title",
                "url",
                "year"
            ]
        );
    }

    #[test]
    fn no_keys_means_no_request() {
        let found = Zotero::new("http://127.0.0.1:1".into())
            .by_keys(&[])
            .unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn an_unreachable_zotero_says_what_to_do_about_it() {
        // Port 1 is not going to be listening.
        let error = Zotero::new("http://127.0.0.1:1".into())
            .search("anything", 5)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Is it running"), "unhelpful error: {error}");
        assert!(
            error.contains("Allow other applications"),
            "unhelpful: {error}"
        );
    }

    #[test]
    fn a_reference_becomes_a_source_the_vault_can_keep() {
        // The point of the conversion: after this, nothing needs Zotero to be
        // running, installed, or ever installed again.
        let reference = Reference {
            key: "ABCD1234".into(),
            title: "Quasi-1D Sb2Se3 ribbons".into(),
            creators: "Zhou et al.".into(),
            year: Some("2019".into()),
            item_type: "journalArticle".into(),
            doi: Some("10.1000/xyz".into()),
            container: Some("Nature Energy".into()),
            url: Some("https://example.org/x".into()),
        };
        let source = reference.to_source();
        assert_eq!(source.authors.as_deref(), Some("Zhou et al."));
        assert_eq!(source.container.as_deref(), Some("Nature Energy"));
        assert_eq!(source.doi.as_deref(), Some("10.1000/xyz"));
        // The key comes along, so a later import updates rather than duplicates.
        assert_eq!(source.zotero.as_deref(), Some("ABCD1234"));
    }

    #[test]
    fn an_item_with_no_creators_has_no_authors_rather_than_an_empty_string() {
        let reference = Reference {
            key: "K".into(),
            title: "T".into(),
            creators: "   ".into(),
            year: None,
            item_type: "manuscript".into(),
            doi: None,
            container: None,
            url: None,
        };
        assert_eq!(reference.to_source().authors, None);
    }
}
