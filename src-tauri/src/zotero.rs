//! Zotero, as a [`ReferenceProvider`].
//!
//! Two ways in, one implementation. Zotero's local connector and its web
//! service speak the same API — same paths, same parameters, same JSON — so
//! the only differences are the root URL and an auth header, and those are the
//! only things [`Flavour`] carries.
//!
//! **Local** talks to 127.0.0.1:23119, which Zotero opens when "Allow other
//! applications on this computer to communicate with Zotero" is on. No API
//! key, no web service, nothing leaves the machine. It is the default for
//! exactly that reason.
//!
//! **Account** talks to api.zotero.org with the user\'s API key. It works when
//! Zotero is not running and on a machine where it is not installed, and it is
//! the only path here that sends anything off this computer — which is why it
//! is off unless configured and says so where it is configured.
//!
//! Everything Zotero-shaped is confined to this file: the port, the JSON field
//! names, the `zotero://` URI scheme, the header name. What leaves it is the
//! provider-agnostic types in references.rs.

use std::time::Duration;

use serde::Deserialize;

use crate::error::{Result, SutraError};
use crate::references::{
    Attachment, Availability, Collection, ItemDetail, Reference, ReferenceProvider, StyledCitation,
    blank_to_none,
};

/// Where Zotero listens. The port is fixed and not configurable in Zotero.
const LOCAL_BASE: &str = "http://127.0.0.1:23119";

/// Zotero\'s web service.
const WEB_BASE: &str = "https://api.zotero.org";

/// The header Zotero\'s web API authenticates with.
const KEY_HEADER: &str = "Zotero-API-Key";

/// Long enough for a large library, short enough that a wedged Zotero does not
/// hang the editor.
const TIMEOUT: Duration = Duration::from_secs(5);

/// How many children an item can have before we stop reading them. A book with
/// four hundred annotations is not worth walking to answer "is there a PDF".
const CHILD_LIMIT: usize = 50;

/// Which Zotero this is talking to.
#[derive(Debug, Clone, PartialEq)]
pub enum Flavour {
    /// The connector on this machine. No key, nothing leaves.
    Local,
    /// zotero.org, with the user\'s numeric id and API key.
    Account { user_id: String, api_key: String },
}

pub struct Zotero {
    base: String,
    flavour: Flavour,
}

impl Default for Zotero {
    fn default() -> Self {
        Self::local()
    }
}

impl Zotero {
    /// The connector on this machine.
    pub fn local() -> Self {
        Self::new(LOCAL_BASE.to_string(), Flavour::Local)
    }

    /// The web service, for a library this machine cannot reach locally.
    pub fn account(user_id: String, api_key: String) -> Self {
        Self::new(WEB_BASE.to_string(), Flavour::Account { user_id, api_key })
    }

    /// Takes the base URL so tests can point either flavour at a stub server.
    pub fn new(base: String, flavour: Flavour) -> Self {
        Self { base, flavour }
    }

    /// Everything before `/items`.
    ///
    /// The local connector serves the API under `/api` and calls every library
    /// user 0, since there is only ever one. The web service has no prefix and
    /// needs the real numeric id.
    fn root(&self) -> String {
        match &self.flavour {
            Flavour::Local => format!("{}/api/users/0", self.base),
            Flavour::Account { user_id, .. } => format!("{}/users/{}", self.base, user_id),
        }
    }

    fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .build()
            .into()
    }

    fn body(&self, url: &str) -> Result<String> {
        let request = self.agent().get(url);
        let request = match &self.flavour {
            Flavour::Local => request,
            Flavour::Account { api_key, .. } => request.header(KEY_HEADER, api_key),
        };
        request
            .call()
            .map_err(|e| SutraError::Zotero(describe(&e, &self.flavour)))?
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
        match self.flavour {
            Flavour::Local => "zotero-local",
            Flavour::Account { .. } => "zotero-account",
        }
    }

    fn label(&self) -> &'static str {
        match self.flavour {
            Flavour::Local => "Zotero",
            Flavour::Account { .. } => "Zotero account",
        }
    }

    /// A cheap request that succeeds only if Zotero is actually answering.
    ///
    /// `limit=1` rather than a bare collections listing: it is the smallest
    /// response the items endpoint can give, and it exercises the same path
    /// the picker will use a moment later.
    fn availability(&self) -> Availability {
        let url = format!("{}/items?limit=1&format=json", self.root());
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
            "{}/items?q={}&qmode=titleCreatorYear&itemType=-attachment%20||%20note&limit={}&format=json",
            self.root(),
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
            "{}/items?itemKey={}&format=json",
            self.root(),
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
            "{}/items/{}/children?limit={}&format=json",
            self.root(),
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
        let url = format!("{}/items/{}?format=json", self.root(), urlencode(key));
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

    /// Ask Zotero to render these items in a CSL style.
    ///
    /// `include=citation,bib` is the whole feature: Zotero runs its own CSL
    /// engine over the items and hands back the formatted strings. Every style
    /// in the Zotero Style Repository works, because it is Zotero's repository
    /// being consulted, and the result agrees with what the same library would
    /// produce in Word — which a second engine written here would not.
    ///
    /// The strings come back as HTML and are flattened to markdown, because
    /// they are going into a markdown file and a note full of `<i>` tags is not
    /// a note anyone wants to open in another editor.
    fn styled(
        &self,
        keys: &[String],
        style: &str,
        locale: &str,
    ) -> Result<Vec<(String, StyledCitation)>> {
        if keys.is_empty() || style.trim().is_empty() {
            return Ok(Vec::new());
        }

        let url = format!(
            "{}/items?itemKey={}&format=json&include=citation,bib&style={}&locale={}",
            self.root(),
            keys.join(","),
            urlencode(style.trim()),
            urlencode(locale.trim()),
        );

        let body = self.body(&url)?;
        let items: Vec<StyledItem> =
            serde_json::from_str(&body).map_err(|e| SutraError::Zotero(e.to_string()))?;

        Ok(items
            .into_iter()
            .filter_map(|item| {
                let styled = StyledCitation {
                    citation: item.citation.as_deref().map(html_to_markdown),
                    bib: item.bib.as_deref().map(html_to_markdown),
                };
                // A style Zotero could not apply comes back with both halves
                // absent. Caching that would cache a failure: the next lookup
                // would find an entry, take the question as answered, and never
                // ask again.
                (!styled.is_empty()).then_some((item.key, styled))
            })
            .collect())
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

/// What `include=citation,bib` adds beside the item data.
#[derive(Debug, Deserialize)]
struct StyledItem {
    key: String,
    #[serde(default)]
    citation: Option<String>,
    #[serde(default)]
    bib: Option<String>,
}

/// Flatten Zotero's rendered HTML to markdown.
///
/// Zotero wraps a citation in spans and a bibliography entry in nested divs,
/// and italicises the journal or book title — which carries real meaning in
/// every style that uses it, so it is kept as markdown emphasis rather than
/// discarded with the rest of the tags.
///
/// Deliberately not a general HTML parser. The input is one program's citation
/// output, not the web; anything unexpected degrades to its own text, which is
/// still a usable citation.
pub fn html_to_markdown(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '<' => {
                let mut tag = String::new();
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                    tag.push(t);
                }
                let name = tag.trim_start_matches('/').trim();
                let name = name.split([' ', '\t']).next().unwrap_or("").to_lowercase();
                // Emphasis survives; everything else is structure that has no
                // meaning once this is markdown.
                if matches!(name.as_str(), "i" | "em") {
                    out.push('*');
                } else if matches!(name.as_str(), "b" | "strong") {
                    out.push_str("**");
                } else if matches!(name.as_str(), "div" | "p" | "br") && !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            '&' => {
                let mut entity = String::new();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == ';' {
                        break;
                    }
                    entity.push(next);
                    if entity.len() > 8 {
                        break;
                    }
                }
                out.push_str(&decode_entity(&entity));
            }
            _ => out.push(c),
        }
    }

    // Zotero indents its nested divs, so once the tags are gone the text is
    // full of runs of spaces and newlines that never meant anything.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entity(entity: &str) -> String {
    match entity {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" | "#39" => "'".to_string(),
        "nbsp" | "#160" => " ".to_string(),
        other => {
            // A numeric entity we have no name for is still recoverable, and
            // citations are full of en dashes.
            if let Some(digits) = other.strip_prefix('#') {
                if let Ok(code) = digits.parse::<u32>() {
                    if let Some(c) = char::from_u32(code) {
                        return c.to_string();
                    }
                }
            }
            // Not an entity after all — put back exactly what was consumed.
            format!("&{other};")
        }
    }
}

/// Turn a connection failure into something a user can act on.
fn describe(error: &ureq::Error, flavour: &Flavour) -> String {
    match error {
        // The overwhelmingly likely cause, and the two fixes are different, so
        // name both rather than reporting "connection refused".
        ureq::Error::ConnectionFailed | ureq::Error::Io(_) => match flavour {
            Flavour::Local => "could not reach Zotero. Is it running, and is \"Allow other \
                 applications on this computer to communicate with Zotero\" enabled in \
                 Settings → Advanced?"
                .to_string(),
            Flavour::Account { .. } => {
                "could not reach zotero.org. Check the network connection.".to_string()
            }
        },
        // A wrong or revoked key is the likely cause and the message from the
        // server is not obviously about that, so say it.
        ureq::Error::StatusCode(403) => {
            "Zotero refused the API key. Check the key and the user ID in Settings.".to_string()
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
        let found = Zotero::new(base, Flavour::Local)
            .search("sb2se3", 20)
            .unwrap();
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
        Zotero::new(base, Flavour::Local)
            .search("Sb2Se3 & CVT", 20)
            .unwrap();
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
        let found = Zotero::new("http://127.0.0.1:1".into(), Flavour::Local)
            .search("   ", 20)
            .unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn items_missing_fields_still_parse() {
        let (base, handle) = stub(r#"[{"key":"K1","data":{},"meta":{}}]"#);
        let found = Zotero::new(base, Flavour::Local).search("x", 5).unwrap();
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
        let found = Zotero::new(base, Flavour::Local)
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
        let found = Zotero::new("http://127.0.0.1:1".into(), Flavour::Local)
            .items(&[])
            .unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn an_unreachable_zotero_says_what_to_do_about_it() {
        // Port 1 is not going to be listening.
        let error = Zotero::new("http://127.0.0.1:1".into(), Flavour::Local)
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
        let found = Zotero::new(base, Flavour::Local)
            .search("sb2se3", 20)
            .unwrap();
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
        let found = Zotero::new(base, Flavour::Local).search("x", 20).unwrap();
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
        let found = Zotero::new(base, Flavour::Local).search("x", 5).unwrap();
        handle.join().unwrap();

        assert_eq!(found[0].citation_key, None);
        assert_eq!(found[0].abstract_text, None);
    }

    #[test]
    fn availability_reports_the_reason_rather_than_failing() {
        // Zotero being closed is an ordinary state of the world, not an error
        // the user should see as a broken app. Nothing here returns Err.
        let status = Zotero::new("http://127.0.0.1:1".into(), Flavour::Local).availability();
        assert!(!status.ready);
        assert_eq!(status.provider, "Zotero");
        assert_eq!(status.provider_id, "zotero-local");
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
        let found = Zotero::new(base, Flavour::Local)
            .attachments("KO2024")
            .unwrap();
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

    #[test]
    fn the_account_flavour_signs_its_requests_and_uses_the_real_user_id() {
        let (base, handle) = stub(TWO_ITEMS);
        let zotero = Zotero::new(
            base,
            Flavour::Account {
                user_id: "48291".into(),
                api_key: "P9NiFoyLeZu2bZNvvuQPDWsd".into(),
            },
        );
        zotero.search("sb2se3", 20).unwrap();
        let request = handle.join().unwrap();

        // The web service has no /api prefix and needs the numeric id; the
        // connector serves /api and calls every library user 0. Getting either
        // wrong is a 404 that reads like an empty library.
        assert!(request.contains("GET /users/48291/items"), "got {request}");
        assert!(!request.contains("/api/users/0"), "got {request}");
        // Header names are case-insensitive on the wire and ureq lowercases
        // them, so this looks for the value under either spelling rather than
        // pinning a casing the HTTP layer is free to choose.
        let lowered = request.to_lowercase();
        assert!(
            lowered.contains("zotero-api-key: p9nifoylezu2bznvvuqpdwsd"),
            "the key must be sent, and in a header rather than the query \
             string where it would end up in server logs: {request}"
        );
        assert!(
            !request.contains("key=P9NiFoyLeZu2bZNvvuQPDWsd"),
            "the key must not be in the URL: {request}"
        );
    }

    #[test]
    fn the_local_flavour_sends_no_key_at_all() {
        let (base, handle) = stub(TWO_ITEMS);
        Zotero::new(base, Flavour::Local).search("x", 5).unwrap();
        let request = handle.join().unwrap();

        // The whole claim of the local path is that nothing leaves and no
        // credential exists. A key header here would be a lie in the docs.
        assert!(!request.contains("Zotero-API-Key"), "got {request}");
        assert!(request.contains("GET /api/users/0/items"), "got {request}");
    }

    #[test]
    fn the_two_flavours_are_named_apart() {
        // The frontend branches on these, and a status line that says "Zotero"
        // when the request is going to zotero.org would hide the one fact the
        // user most needs.
        assert_eq!(Zotero::local().id(), "zotero-local");
        assert_eq!(
            Zotero::account("1".into(), "k".into()).id(),
            "zotero-account"
        );
        assert_eq!(Zotero::local().label(), "Zotero");
        assert_eq!(
            Zotero::account("1".into(), "k".into()).label(),
            "Zotero account"
        );
    }

    #[test]
    fn styled_asks_zotero_to_do_the_formatting() {
        let (base, handle) = stub(
            r#"[{"key":"KO2024","citation":"<span>(1)</span>",
                 "bib":"<div class=\"csl-bib-body\"><div class=\"csl-entry\">Ko, J. Thermal conductivity. <i>Nature Energy</i> <b>2024</b>, 14, 221&#8211;230.</div></div>"}]"#,
        );
        let found = Zotero::new(base, Flavour::Local)
            .styled(&["KO2024".into()], "american-chemical-society", "en-US")
            .unwrap();
        let request = handle.join().unwrap();

        // This is the whole feature: Zotero owns CSL, so the app asks for the
        // rendered strings rather than growing a second citation engine.
        assert!(request.contains("include=citation,bib"), "got {request}");
        assert!(
            request.contains("style=american-chemical-society"),
            "got {request}"
        );
        assert!(request.contains("locale=en-US"), "got {request}");

        assert_eq!(found.len(), 1);
        let (key, styled) = &found[0];
        assert_eq!(key, "KO2024");
        assert_eq!(styled.citation.as_deref(), Some("(1)"));
        // Italics carry meaning in every style that uses them, so they survive
        // as markdown; the divs and the entity do not.
        assert_eq!(
            styled.bib.as_deref(),
            Some("Ko, J. Thermal conductivity. *Nature Energy* **2024**, 14, 221–230.")
        );
    }

    #[test]
    fn a_style_zotero_cannot_apply_is_not_cached_as_an_answer() {
        // Both halves absent means Zotero declined. Storing that would store a
        // failure: the next lookup would find an entry, take the question as
        // answered, and never ask again.
        let (base, handle) = stub(r#"[{"key":"KO2024"}]"#);
        let found = Zotero::new(base, Flavour::Local)
            .styled(&["KO2024".into()], "no-such-style", "en-US")
            .unwrap();
        handle.join().unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn styling_nothing_does_not_hit_the_network() {
        // No stub server: connecting would fail.
        let zotero = Zotero::new("http://127.0.0.1:1".into(), Flavour::Local);
        assert!(zotero.styled(&[], "apa", "en-US").unwrap().is_empty());
        // An empty style is "label only", which is a real setting, not a query.
        assert!(
            zotero
                .styled(&["K".into()], "  ", "en-US")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rendered_html_becomes_readable_markdown() {
        assert_eq!(
            html_to_markdown("<span>(Ko et al., 2024)</span>"),
            "(Ko et al., 2024)"
        );
        assert_eq!(html_to_markdown("A <i>B</i> C"), "A *B* C");
        assert_eq!(html_to_markdown("<b>2024</b>"), "**2024**");
        // Entities citations are actually full of.
        assert_eq!(html_to_markdown("221&#8211;230"), "221–230");
        assert_eq!(html_to_markdown("Smith &amp; Jones"), "Smith & Jones");
        assert_eq!(html_to_markdown("a&nbsp;b"), "a b");
        // Zotero indents its nested divs; none of that whitespace means
        // anything once the tags are gone.
        assert_eq!(
            html_to_markdown("<div>\n  <div>Ko, J.</div>\n</div>"),
            "Ko, J."
        );
        // Something that only looks like an entity is left alone rather than
        // silently eaten.
        assert_eq!(html_to_markdown("cost &lt; 5"), "cost < 5");
    }
}
