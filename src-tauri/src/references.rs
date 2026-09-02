//! Reference managers, in the abstract.
//!
//! Zotero is the only one implemented, and the only one planned. This layer
//! exists anyway, because the alternative is Zotero's shape leaking into the
//! editor, the index, search and the assistant — and then a second provider is
//! not a new file but a rewrite of five.
//!
//! The trait is deliberately the *reading* half of a reference manager. Writing
//! back to someone's library is a separate, explicit act (a user pressing a
//! button that says so), not something a general interface should make
//! convenient, so no `update` or `delete` appears here.
//!
//! What lives here is provider-agnostic: a reference, its attachments, the
//! collections it belongs to. What lives in zotero.rs is how to obtain those
//! from Zotero specifically. Nothing above this module names Zotero except the
//! settings that choose it and the words on screen.

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// One reference, flattened to what a citation and a source note need.
///
/// This is the *cheap* record — everything obtainable from a single search
/// response. Collections and attachments cost extra requests and live on
/// [`ItemDetail`], so typing in a picker does not fire three calls per
/// keystroke.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
// The frontend reads these, and `item_type` would otherwise arrive as
// `item_type` in JavaScript.
#[serde(rename_all = "camelCase")]
pub struct Reference {
    /// The provider's stable identifier for this item. For Zotero, the item
    /// key. Stable for the life of the item, and what a source note stores —
    /// the same trick as `[[id]]`: the document holds an identifier, and
    /// everything human is resolved for display.
    pub key: String,
    pub title: String,
    /// "Smith et al." — composed by the provider, so citation style stays its
    /// problem rather than ours.
    pub creators: String,
    pub year: Option<String>,
    pub item_type: String,
    pub doi: Option<String>,
    /// Journal, book or proceedings — whatever it appeared in.
    pub container: Option<String>,
    pub url: Option<String>,
    /// The citation key, when the provider has one.
    ///
    /// Zotero only has these when Better BibTeX is installed. `None` means
    /// exactly that and is never filled in with something plausible: a
    /// fabricated `@Ko2024` that does not exist in the user's library is worse
    /// than no key at all, because it looks right in a draft and fails at the
    /// bibliography.
    pub citation_key: Option<String>,
    /// The abstract as the publisher wrote it. Never a generated summary —
    /// see the note on [`Provenance`].
    pub abstract_text: Option<String>,
    pub date_added: Option<String>,
}

impl Reference {
    /// What a source note records about this item.
    ///
    /// The import direction matters: details are copied into the vault once,
    /// not looked up every time they are displayed. That is what makes a
    /// citation survive the reference manager being uninstalled, the vault
    /// being opened on another machine, or the library being reorganised.
    pub fn to_source(&self) -> crate::frontmatter::SourceMeta {
        crate::frontmatter::SourceMeta {
            authors: blank_to_none(&self.creators),
            year: self.year.clone(),
            container: self.container.clone(),
            doi: self.doi.clone(),
            url: self.url.clone(),
            zotero: Some(self.key.clone()),
            citation_key: self.citation_key.clone(),
            abstract_text: self.abstract_text.clone(),
            item_type: blank_to_none(&self.item_type),
            added: self.date_added.clone(),
            collections: Vec::new(),
            pdf: None,
            styled: Default::default(),
        }
    }
}

pub fn blank_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// A file hanging off a reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub key: String,
    pub title: String,
    pub content_type: Option<String>,
    /// Whether this is a PDF, which is the only kind the app treats specially.
    pub is_pdf: bool,
}

/// A folder in the reference manager's own hierarchy.
///
/// Read, never mirrored. A Zotero collection and a Sutra folder are
/// independent structures on purpose (section 12 of the brief): an item can
/// sit in three collections while the notes about it sit in one folder
/// somewhere else entirely, and forcing either to follow the other loses that.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub key: String,
    pub name: String,
}

/// Everything about one item, including what costs extra requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    #[serde(flatten)]
    pub reference: Reference,
    /// Collection *names*, resolved. Empty means the item is in no collection,
    /// which is a real state and not an error.
    pub collections: Vec<String>,
    pub attachments: Vec<Attachment>,
}

/// One reference rendered by a CSL style: the inline form and the entry.
///
/// Produced by the reference manager, never by us. Formatting a citation
/// correctly is the Citation Style Language, an entire specification with
/// hundreds of styles, and Zotero already contains a complete implementation
/// of it. Reimplementing that here would be a second, worse engine that
/// disagrees with the one the user's supervisor reads — so the app asks
/// Zotero and caches the answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StyledCitation {
    /// "(Ko et al., 2024)" — what goes in the sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<String>,
    /// "Ko, J.; ... Nature Energy 2024, 14, 221–230." — what goes in the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bib: Option<String>,
}

impl StyledCitation {
    /// Whether this is worth storing at all.
    ///
    /// A style the provider could not apply comes back with both halves empty,
    /// and caching that would be caching a failure — the next lookup would
    /// find an entry, believe the question answered, and never ask again.
    pub fn is_empty(&self) -> bool {
        self.citation.is_none() && self.bib.is_none()
    }
}

/// Whether the provider can be reached right now.
///
/// A separate type rather than a bool, because "unavailable" has to carry its
/// reason to the surface: the app must keep working with cached metadata and
/// say *why* the live half is missing, rather than silently degrading and
/// leaving the user to wonder whether their library is gone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub ready: bool,
    /// Machine name, for a caller that needs to branch on which provider this
    /// is without matching on a display label that may be translated.
    pub provider_id: String,
    /// What to call it on screen.
    pub provider: String,
    /// Present only when `ready` is false.
    pub reason: Option<String>,
}

/// The reading half of a reference manager.
///
/// Method names follow the brief's `ReferenceProvider` (search, getItem,
/// getCollections, getAttachments, getMetadata, openItem) with the `get_`
/// prefixes dropped, which is what Rust does. `detail` is `getMetadata`.
pub trait ReferenceProvider: Send + Sync {
    /// Stable machine name, stored in frontmatter beside a key so a future
    /// provider's identifiers are never confused with this one's.
    fn id(&self) -> &'static str;

    /// What to call it on screen.
    fn label(&self) -> &'static str;

    /// Whether the library can be reached, and why not when it cannot.
    fn availability(&self) -> Availability;

    fn search(&self, query: &str, limit: usize) -> Result<Vec<Reference>>;

    /// Look up specific items by key, so a citation can render its label.
    fn items(&self, keys: &[String]) -> Result<Vec<Reference>>;

    /// Everything about one item, collections and attachments included.
    fn detail(&self, key: &str) -> Result<ItemDetail>;

    fn collections(&self) -> Result<Vec<Collection>>;

    fn attachments(&self, key: &str) -> Result<Vec<Attachment>>;

    /// Show the item in the reference manager's own window.
    fn open(&self, key: &str) -> Result<()>;

    /// Ask the provider to render these items in a CSL style.
    ///
    /// `style` is a Zotero Style Repository id — "american-chemical-society",
    /// "nature", "apa" — or the URL of a CSL file. `locale` is a BCP-47 tag.
    /// Returns only the items it could render: a style the provider does not
    /// have yields fewer entries rather than an error, because a missing
    /// styled form falls back to a plain label and losing the whole lookup
    /// would lose the ones that did work.
    fn styled(
        &self,
        keys: &[String],
        style: &str,
        locale: &str,
    ) -> Result<Vec<(String, StyledCitation)>>;
}
