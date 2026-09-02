//! Zotero, as a [`ReferenceProvider`].
//!
//! Talks to the local HTTP API Zotero exposes on 127.0.0.1:23119 when "Allow
//! other applications on this computer to communicate with Zotero" is on. No
//! API key, no web service, nothing leaves the machine — which is the whole
//! reason this is the local API rather than zotero.org.
//!
//! Everything Zotero-shaped is confined to this file: the port, the JSON field
//! names, the `zotero://` URI scheme. What leaves it is the provider-agnostic
//! types in references.rs.

use std::time::Duration;

use serde::Deserialize;

use crate::error::{Result, SutraError};
use crate::references::{
    Attachment, Availability, Collection, ItemDetail, Reference, ReferenceProvider, blank_to_none,
};

/// Where Zotero listens. The port is fixed and not configurable in Zotero.
const LOCAL_BASE: &str = "http://127.0.0.1:23119";

/// Long enough for a large library, short enough that a wedged Zotero does not
/// hang the editor.
const TIMEOUT: Duration = Duration::from_secs(5);

/// How many children an item can have before we stop reading them. A book with
/// four hundred annotations is not worth walking to answer "is there a PDF".
const CHILD_LIMIT: usize = 50;

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

    fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .build()
            .into()
    }

    fn body(&self, url: &str) -> Result<String> {
        self.agent()
            .get(url)
            .call()
            .map_err(|e| SutraError::Zotero(describe(&e)))?
            .body_mut()
            .read_to_string()
            .map_err(|e| SutraError::Zotero(e.to_string()))
    }

    /// Fetch and parse a list endpoint. Named apart from the trait's
    /// `items`, which takes keys rather than a URL.
    fn fetch(&self, url: &str) -> Result<Vec<Reference>> {
        let body = self.body(url)?;
        let items: Vec<ZoteroItem> =
            serde_json::from_str(&body).map_err(|e| SutraError::Zotero(e.to_string()))?;
        Ok(items.into_iter().map(reference_from).collect())
    }

    fn raw_items(&self, url: &str) -> Result<Vec<ZoteroItem>> {
        let body = self.body(url)?;
        serde_json::from_str(&body).map_err(|e| SutraError::Zotero(e.to_string()))
    }
}

impl ReferenceProvider for Zotero {
    fn id(&self) -> &'static str {
        "zotero"
    }

    fn label(&self) -> &'static str {
        "Zotero"
    }

    /// A cheap request that succeeds only if Zotero is actually answering.
    ///
    /// `limit=1` rather than a bare collections listing: it is the smallest
    /// response the items endpoint can give, and it exercises the same path
    /// the picker will use a moment later.
    fn availability(&self) -> Availability {
        let url = format!("{}/api/users/0/items?limit=1&format=json", self.base);
        match self.body(&url) {
            Ok(_) => Availability {
                ready: true,
                provider_id: self.id().to_string(),
                provider: self.label().to_string(),
                reason: None,
            },
            Err(e) => Availability {
                ready: false,
                provider_id: self.id().to_string(),
                provider: self.label().to_string(),
                reason: Some(e.to_string()),
            },
        }
    }

    /// Search the user's library.
    ///
    /// `qmode=titleCreatorYear` searches the fields a person actually
    /// remembers, rather than everything including the full text of
    /// attachments, which turns a search for an author into a list of every
    /// PDF mentioning them.
    fn search(&self, query: &str, limit: usize) -> Result<Vec<Reference>> {
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
        self.fetch(&url)
    }

    fn items(&self, keys: &[String]) -> Result<Vec<Reference>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!(
            "{}/api/users/0/items?itemKey={}&format=json",
            self.base,
            keys.join(",")
        );
        self.fetch(&url)
    }

    fn collections(&self) -> Result<Vec<Collection>> {
        let url = format!(
            "{}/api/users/0/collections?limit=200&format=json",
            self.base
        );
        let body = self.body(&url)?;
        let raw: Vec<ZoteroCollection> =
            serde_json::from_str(&body).map_err(|e| SutraError::Zotero(e.to_string()))?;
        Ok(raw
            .into_iter()
            .map(|c| Collection {
                key: c.key,
                name: c.data.name,
            })
            .collect())
    }

    fn attachments(&self, key: &str) -> Result<Vec<Attachment>> {
        let url = format!(
            "{}/api/users/0/items/{}/children?limit={}&format=json",
            self.base,
            urlencode(key),
            CHILD_LIMIT
        );
        let children = self.raw_items(&url)?;
        Ok(children
            .into_iter()
            .filter(|c| c.data.item_type == "attachment")
            .map(|c| {
                let content_type = c.data.content_type.filter(|t| !t.trim().is_empty());
                Attachment {
                    is_pdf: content_type.as_deref() == Some("application/pdf"),
                    key: c.key,
                    title: if c.data.title.trim().is_empty() {
                        "Attachment".to_string()
                    } else {
                        c.data.title
                    },
                    content_type,
                }
            })
            .collect())
    }

    /// Everything about one item.
    ///
    /// Attachments and collection names are fetched here and nowhere else,
    /// because each costs a round trip and a picker that fired three requests
    /// per keystroke would be unusable. A failure in either half degrades to
    /// an empty list rather than failing the whole lookup: knowing the title
    /// and DOI but not whether there is a PDF is far more useful than an
    /// error.
    fn detail(&self, key: &str) -> Result<ItemDetail> {
        let url = format!(
            "{}/api/users/0/items/{}?format=json",
            self.base,
            urlencode(key)
        );
        let body = self.body(&url)?;
        // The single-item endpoint returns an object; the list endpoints return
        // an array. Accept either, so a future Zotero that changes its mind
        // does not break this.
        let item: ZoteroItem = match serde_json::from_str::<ZoteroItem>(&body) {
            Ok(item) => item,
            Err(_) => serde_json::from_str::<Vec<ZoteroItem>>(&body)
                .map_err(|e| SutraError::Zotero(e.to_string()))?
                .into_iter()
                .next()
                .ok_or_else(|| SutraError::Zotero(format!("no item {key} in Zotero")))?,
        };

        let collection_keys = item.data.collections.clone();
        let reference = reference_from(item);

        let names = if collection_keys.is_empty() {
            Vec::new()
        } else {
            self.collections()
                .map(|all| {
                    collection_keys
                        .iter()
                        .filter_map(|k| all.iter().find(|c| &c.key == k))
                        .map(|c| c.name.clone())
                        .collect()
                })
                .unwrap_or_default()
        };

        Ok(ItemDetail {
            reference,
            collections: names,
            attachments: self.attachments(key).unwrap_or_default(),
        })
    }

    /// Show the item in Zotero's own window.
    ///
    /// Handing a `zotero://` URI to the desktop is the documented way, and the
    /// only one: the local HTTP API can read the library but cannot raise the
    /// window. That means asking the OS to run its handler, which is why this
    /// is the one place in the app that spawns a process.
    fn open(&self, key: &str) -> Result<()> {
        let uri = select_uri(key);
        let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "windows") {
            // The empty string is the window title `start` expects first, and
            // omitting it makes `start` treat the URI as the title.
            ("cmd", vec!["/C", "start", "", &uri])
        } else if cfg!(target_os = "macos") {
            ("open", vec![&uri])
        } else {
            ("xdg-open", vec![&uri])
        };

        std::process::Command::new(program)
            .args(&args)
            .spawn()
            .map(|_| ())
            .map_err(|e| SutraError::Zotero(format!("could not open Zotero: {e}")))
    }
}

/// The URI that selects one item in the Zotero desktop window.
///
/// Its own function so the shape is pinned by a test: launching a process is
/// not testable here, but getting this string wrong is the likely failure and
/// it is pure.
pub fn select_uri(key: &str) -> String {
    format!("zotero://select/library/items/{key}")
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
    /// Better BibTeX's citation key. Absent unless that plugin is installed,
    /// and absent is a perfectly ordinary state — never filled in with a
    /// guess.
    #[serde(rename = "citationKey", default)]
    citation_key: Option<String>,
    #[serde(rename = "abstractNote", default)]
    abstract_note: Option<String>,
    #[serde(rename = "dateAdded", default)]
    date_added: Option<String>,
    /// Collection keys, not names. Resolving them costs another request, so
    /// only `detail` does it.
    #[serde(default)]
    collections: Vec<String>,
    #[serde(rename = "contentType", default)]
    content_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ItemMeta {
    #[serde(rename = "creatorSummary", default)]
    creator_summary: Option<String>,
    /// Zotero normalises whatever was in the date field to `YYYY-MM-DD`.
    #[serde(rename = "parsedDate", default)]
    parsed_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZoteroCollection {
    key: String,
    #[serde(default)]
    data: CollectionData,
}

#[derive(Debug, Default, Deserialize)]
struct CollectionData {
    #[serde(default)]
    name: String,
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
        doi: item.data.doi.filter(|d| !d.trim().is_empty()),
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
        citation_key: item.data.citation_key.as_deref().and_then(blank_to_none),
        abstract_text: item.data.abstract_note.as_deref().and_then(blank_to_none),
        date_added: item.data.date_added.filter(|d| !d.trim().is_empty()),
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
            .items(&["ABCD1234".into(), "EFGH5678".into()])
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
            citation_key: Some("Ko2024".into()),
            abstract_text: None,
            date_added: None,
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
                "abstractText",
                "citationKey",
                "container",
                "creators",
                "dateAdded",
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
        let found = Zotero::new("http://127.0.0.1:1".into()).items(&[]).unwrap();
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
            ..Default::default()
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
            ..Default::default()
        };
        assert_eq!(reference.to_source().authors, None);
    }

    const RICH_ITEM: &str = r#"[
      {"key":"KO2024","version":1,
       "data":{"itemType":"journalArticle","title":"Thermal conductivity of Sb2Se3 nanowires",
               "DOI":"10.1000/abc","publicationTitle":"Nature Energy",
               "citationKey":"Ko2024","abstractNote":"We report kappa = 0.037 W/mK.",
               "dateAdded":"2024-03-04T09:00:00Z","collections":["COLL1","COLL9"]},
       "meta":{"creatorSummary":"Ko et al.","parsedDate":"2024-05-01"}}
    ]"#;

    #[test]
    fn the_citation_key_and_abstract_are_read_when_present() {
        let (base, handle) = stub(RICH_ITEM);
        let found = Zotero::new(base).search("sb2se3", 20).unwrap();
        handle.join().unwrap();

        assert_eq!(found[0].citation_key.as_deref(), Some("Ko2024"));
        assert_eq!(
            found[0].abstract_text.as_deref(),
            Some("We report kappa = 0.037 W/mK.")
        );
        assert_eq!(found[0].date_added.as_deref(), Some("2024-03-04T09:00:00Z"));
    }

    #[test]
    fn a_missing_citation_key_stays_missing() {
        // The rule the whole feature turns on: Better BibTeX is not installed
        // for most people, and an invented "@Zhou2019" reads correctly in a
        // draft and then fails at the bibliography, which is worse than no key
        // at all. There is no code path that fills this in — this test is what
        // stops one being added.
        let (base, handle) = stub(TWO_ITEMS);
        let found = Zotero::new(base).search("x", 20).unwrap();
        handle.join().unwrap();

        assert_eq!(found[0].citation_key, None);
        assert_eq!(found[0].abstract_text, None);
        assert_eq!(found[1].citation_key, None);
    }

    #[test]
    fn a_blank_citation_key_is_absent_rather_than_empty() {
        // Zotero writes "" rather than omitting the field in some versions.
        // An empty string would render as "@" in a citation, which looks like
        // a key and is not one.
        let (base, handle) =
            stub(r#"[{"key":"K","data":{"citationKey":"  ","abstractNote":""},"meta":{}}]"#);
        let found = Zotero::new(base).search("x", 5).unwrap();
        handle.join().unwrap();

        assert_eq!(found[0].citation_key, None);
        assert_eq!(found[0].abstract_text, None);
    }

    #[test]
    fn availability_reports_the_reason_rather_than_failing() {
        // Zotero being closed is an ordinary state of the world, not an error
        // the user should see as a broken app. Nothing here returns Err.
        let status = Zotero::new("http://127.0.0.1:1".into()).availability();
        assert!(!status.ready);
        assert_eq!(status.provider, "Zotero");
        assert_eq!(status.provider_id, "zotero");
        let reason = status.reason.unwrap();
        // The two fixes are different, so the message names both.
        assert!(reason.contains("Is it running"), "got {reason}");
        assert!(reason.contains("Allow other"), "got {reason}");
    }

    #[test]
    fn attachments_keep_only_the_attachments_and_mark_the_pdf() {
        let (base, handle) = stub(
            r#"[
              {"key":"A1","data":{"itemType":"attachment","title":"Ko 2024.pdf","contentType":"application/pdf"}},
              {"key":"A2","data":{"itemType":"attachment","title":"Snapshot","contentType":"text/html"}},
              {"key":"N1","data":{"itemType":"note","title":"a note"}}
            ]"#,
        );
        let found = Zotero::new(base).attachments("KO2024").unwrap();
        let request = handle.join().unwrap();

        assert!(request.contains("/items/KO2024/children"), "got {request}");
        assert_eq!(found.len(), 2, "the child note is not an attachment");
        assert!(found[0].is_pdf);
        assert_eq!(found[0].title, "Ko 2024.pdf");
        assert!(!found[1].is_pdf, "a web snapshot is not a PDF");
    }

    #[test]
    fn the_select_uri_addresses_one_item() {
        // Launching a process is not testable here; the string is, and getting
        // it wrong is the likely failure.
        assert_eq!(select_uri("KO2024"), "zotero://select/library/items/KO2024");
    }

    #[test]
    fn a_reference_becomes_a_source_carrying_its_provenance() {
        let reference = Reference {
            key: "KO2024".into(),
            title: "Thermal conductivity".into(),
            creators: "Ko et al.".into(),
            year: Some("2024".into()),
            item_type: "journalArticle".into(),
            doi: Some("10.1000/abc".into()),
            container: Some("Nature Energy".into()),
            url: None,
            citation_key: Some("Ko2024".into()),
            abstract_text: Some("We report...".into()),
            date_added: Some("2024-03-04T09:00:00Z".into()),
        };
        let source = reference.to_source();

        // The key is what makes a re-import update this note rather than make
        // a second one, and what "open in Zotero" needs years later.
        assert_eq!(source.zotero.as_deref(), Some("KO2024"));
        assert_eq!(source.citation_key.as_deref(), Some("Ko2024"));
        assert_eq!(source.doi.as_deref(), Some("10.1000/abc"));
        assert_eq!(source.item_type.as_deref(), Some("journalArticle"));
        assert_eq!(source.added.as_deref(), Some("2024-03-04T09:00:00Z"));
    }
}
