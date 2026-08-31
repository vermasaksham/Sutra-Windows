//! Saved views: a typed query, compiled to one indexed SQL statement.
//!
//! A view is a note. Not a row in a settings table and not a `.json` beside the
//! vault — a note of `type: view` whose frontmatter carries a `view:` block,
//! ordinary in every other way. It can be tagged, linked to, moved, backed up
//! and read in ten years with none of this software installed, exactly like
//! every other note here.
//!
//! # The boundary
//!
//! A view is a **saved query**, not a database view. It has no schema of its
//! own, no columns, no per-view fields, and nothing can be edited from inside
//! one. Its results are the notes themselves, opened in their real folders. The
//! moment a view can hold a value that is not in some note, this has become the
//! filtered-table feature Sutra exists in opposition to.
//!
//! # Why typed and not a string language
//!
//! `tag:xrd AND (type:literature OR type:experiment) -tag:archive` is a parser,
//! an error-message design, a syntax to teach, and an escaping problem the
//! first time a tag contains a space. The query here is a small tree of typed
//! terms, so YAML does the parsing, the UI can offer a real form instead of a
//! text box, and every term compiles to a SQL fragment that can use an index.
//! The cost is that the expressible queries are the ones enumerated in
//! [`Condition`] and no others, which is a boundary worth having.

use crate::frontmatter::NoteType;
use rusqlite::ToSql;
use serde::{Deserialize, Serialize};

/// One thing a note must (or must not) be.
///
/// Externally tagged, so each reads as one key in YAML:
///
/// ```yaml
/// all:
///   - under: Research/Sb2Se3
///   - tag: method/xrd
///   - type: literature
/// none:
///   - tag: archive
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Condition {
    /// In exactly this folder. `""` is the vault root.
    In(String),
    /// In this folder or any folder beneath it.
    Under(String),
    /// Carries this tag, or any tag beneath it: `method` finds `method/xrd`.
    ///
    /// Asymmetric with [`Condition::In`] on purpose. A folder is a place and
    /// asking for one is asking for that place; a hierarchical tag is a
    /// category, and a category that did not include its own subdivisions
    /// would not be one. This is also how clicking a tag already behaves
    /// everywhere else in the app, and a view that disagreed with the sidebar
    /// about what a tag means would be a bug wearing a feature's clothes.
    Tag(String),
    Type(NoteType),
    /// Cites this source note.
    Cites(String),
    /// Contains a `[[wikilink]]` to this note.
    LinksTo(String),
    /// Matches this full-text query, over title, body and tags.
    Text(String),
    /// Edited on or after this date, as `YYYY-MM-DD`.
    UpdatedAfter(String),
    /// Edited before this date, as `YYYY-MM-DD`.
    UpdatedBefore(String),
}

impl Condition {
    /// The YAML key this condition is written as.
    ///
    /// Must match the `rename_all = "kebab-case"` names the derived
    /// `Deserialize` reads, which `every_condition_survives_a_round_trip`
    /// pins — the two drifting apart would write files this app cannot read.
    fn key(&self) -> &'static str {
        match self {
            Self::In(_) => "in",
            Self::Under(_) => "under",
            Self::Tag(_) => "tag",
            Self::Type(_) => "type",
            Self::Cites(_) => "cites",
            Self::LinksTo(_) => "links-to",
            Self::Text(_) => "text",
            Self::UpdatedAfter(_) => "updated-after",
            Self::UpdatedBefore(_) => "updated-before",
        }
    }
}

impl Serialize for Condition {
    /// By hand, because deriving it writes `!under Research` — valid YAML, and
    /// not what anyone opening the file would have typed. A view is a file
    /// people read and edit, so it is written the way they would write it:
    /// one key, one value.
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(1))?;
        match self {
            Self::In(v)
            | Self::Under(v)
            | Self::Tag(v)
            | Self::Cites(v)
            | Self::LinksTo(v)
            | Self::Text(v)
            | Self::UpdatedAfter(v)
            | Self::UpdatedBefore(v) => map.serialize_entry(self.key(), v)?,
            Self::Type(t) => map.serialize_entry(self.key(), t.as_str())?,
        }
        map.end()
    }
}

/// A term in one of the three lists.
///
/// [`Term::Unreadable`] is a condition this version does not understand — a
/// key from a newer Sutra, or a typo. It is kept verbatim and written back
/// unchanged, because silently dropping part of someone's query on the next
/// save is data loss, and because a view saved on a newer machine must survive
/// a round trip through an older one.
/// Untagged, which is what makes reading one never fail: serde tries
/// [`Condition`] first and falls back to keeping the raw YAML, and writing
/// puts back whichever it holds with no wrapper of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Term {
    Known(Condition),
    Unreadable(serde_yaml_ng::Value),
}

/// What order results come back in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sort {
    /// Most recently edited first. The default, because a view is nearly always
    /// asking "what is going on with this".
    #[default]
    Recent,
    /// Least recently edited first — the "what have I abandoned" ordering.
    Stale,
    Title,
    /// By folder, then by the order notes sit in it.
    Folder,
}

impl Sort {
    fn order_by(self) -> &'static str {
        match self {
            Self::Recent => "n.updated DESC, n.title COLLATE NOCASE",
            Self::Stale => "n.updated ASC, n.title COLLATE NOCASE",
            Self::Title => "n.title COLLATE NOCASE",
            Self::Folder => "n.folder, n.position, n.title COLLATE NOCASE",
        }
    }
}

/// How many results a view returns when it does not say.
///
/// A view is a list to read, not a dataset. Past a couple of hundred rows
/// nobody is reading it and the honest answer is to narrow the query, so the
/// default stops there and says it stopped.
pub const DEFAULT_LIMIT: usize = 200;

/// The `view:` block of a view note's frontmatter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Query {
    /// Every one of these must hold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all: Vec<Term>,
    /// At least one of these must hold. Empty means "no such requirement",
    /// not "nothing matches".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any: Vec<Term>,
    /// None of these may hold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub none: Vec<Term>,
    #[serde(default)]
    pub sort: Sort,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// A compiled query: one statement and its parameters, ready to run.
pub struct Compiled {
    pub sql: String,
    pub params: Vec<Box<dyn ToSql>>,
    /// Terms that were skipped because this version cannot read them.
    pub ignored: usize,
}

impl Query {
    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_LIMIT)
    }

    /// Every term this version could not read, in the order they appear.
    pub fn unreadable(&self) -> Vec<&serde_yaml_ng::Value> {
        self.all
            .iter()
            .chain(&self.any)
            .chain(&self.none)
            .filter_map(|t| match t {
                Term::Unreadable(v) => Some(v),
                Term::Known(_) => None,
            })
            .collect()
    }

    /// Compile to a single SELECT.
    ///
    /// One statement, not a filter loop in Rust: every condition here reduces
    /// to something SQLite can answer from an index, and letting it plan the
    /// whole thing is the difference between a view over a large vault being
    /// instant and being a full scan per term.
    pub fn compile(&self) -> Compiled {
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();
        let mut clauses: Vec<String> = Vec::new();

        let mut fold = |terms: &[Term], join: &str, negate: bool, out: &mut Vec<String>| {
            let parts: Vec<String> = terms
                .iter()
                .filter_map(|term| match term {
                    Term::Known(c) => Some(compile_condition(c, &mut params)),
                    Term::Unreadable(_) => None,
                })
                .collect();
            if parts.is_empty() {
                return;
            }
            let joined = parts.join(join);
            out.push(if negate {
                format!("NOT ({joined})")
            } else {
                format!("({joined})")
            });
        };

        fold(&self.all, " AND ", false, &mut clauses);
        fold(&self.any, " OR ", false, &mut clauses);
        // `none` is the OR of its terms, negated: a note is excluded if any one
        // of them holds. `NOT (a OR b)` rather than `NOT a AND NOT b` only
        // because it reads the way the key does.
        fold(&self.none, " OR ", true, &mut clauses);

        // No conditions at all is the whole vault, not nothing. An empty view
        // is a view someone has not finished writing, and showing them
        // everything makes that obvious in a way an empty list does not.
        let where_clause = if clauses.is_empty() {
            "1".to_string()
        } else {
            clauses.join(" AND ")
        };

        params.push(Box::new(self.limit() as i64));
        Compiled {
            sql: format!(
                "SELECT n.id, n.note_type, n.title, n.folder, n.position, n.tags, n.icon, \
                        n.cover, n.excerpt, n.source, n.sources, n.updated \
                 FROM notes n WHERE {where_clause} ORDER BY {} LIMIT ?",
                self.sort.order_by()
            ),
            params,
            // Counted from the query rather than tallied while compiling, so
            // the number the UI shows and the terms it could not read are the
            // same fact rather than two that can disagree.
            ignored: self.unreadable().len(),
        }
    }
}

/// One condition as a SQL fragment, pushing its parameters in order.
fn compile_condition(condition: &Condition, params: &mut Vec<Box<dyn ToSql>>) -> String {
    match condition {
        Condition::In(folder) => {
            params.push(Box::new(normalise_folder(folder)));
            "n.folder = ?".into()
        }
        Condition::Under(folder) => {
            // A prefix range rather than LIKE or GLOB. Both of those would need
            // the folder name escaped — a folder called `Data [raw]` is a
            // perfectly ordinary thing to have — and neither reliably uses the
            // index on `folder`. A half-open range does both for free.
            let folder = normalise_folder(folder);
            if folder.is_empty() {
                return "1".into();
            }
            let (lo, hi) = descendant_range(&folder);
            params.push(Box::new(folder));
            params.push(Box::new(lo));
            params.push(Box::new(hi));
            "(n.folder = ? OR (n.folder >= ? AND n.folder < ?))".into()
        }
        Condition::Tag(tag) => {
            let tag = tag.trim().trim_matches('/').to_string();
            if tag.is_empty() {
                return "0".into();
            }
            let (lo, hi) = descendant_range(&tag);
            params.push(Box::new(tag));
            params.push(Box::new(lo));
            params.push(Box::new(hi));
            // `IN (SELECT …)` rather than `EXISTS (… WHERE note_id = n.id)`.
            // The two mean the same thing, but the EXISTS form pins SQLite to
            // walking every candidate note and looking up its tags, while this
            // one lets it start from the tag index and find the few notes that
            // carry the tag — which is the whole reason that index exists.
            "n.id IN (SELECT t.note_id FROM note_tags t \
                      WHERE t.tag = ? OR (t.tag >= ? AND t.tag < ?))"
                .into()
        }
        Condition::Type(note_type) => {
            params.push(Box::new(note_type.as_str()));
            "n.note_type = ?".into()
        }
        Condition::Cites(id) => {
            params.push(Box::new(id.clone()));
            "n.id IN (SELECT s.note_id FROM note_sources s WHERE s.source_id = ?)".into()
        }
        Condition::LinksTo(id) => {
            params.push(Box::new(id.clone()));
            "n.id IN (SELECT l.source FROM links l WHERE l.target = ?)".into()
        }
        Condition::Text(query) => {
            let query = query.trim();
            if query.is_empty() {
                return "1".into();
            }
            params.push(Box::new(crate::index::fts_query(query)));
            "n.id IN (SELECT id FROM notes_fts WHERE notes_fts MATCH ?)".into()
        }
        // `updated` is stored as RFC 3339 in UTC, so it sorts and compares as
        // text. Comparing against the bare date works because every timestamp
        // for that day begins with it: `>= "2026-01-01"` includes the whole of
        // the 1st, and `< "2026-01-01"` excludes it.
        Condition::UpdatedAfter(date) => {
            params.push(Box::new(date.trim().to_string()));
            "n.updated >= ?".into()
        }
        Condition::UpdatedBefore(date) => {
            params.push(Box::new(date.trim().to_string()));
            "n.updated < ?".into()
        }
    }
}

/// The character one past `/`, which happens to be `0`.
const AFTER_SEPARATOR: char = '0';

/// The half-open text range holding every descendant of `path`.
///
/// `["Research/", "Research0")` — nothing sorts between `Research/…` and
/// `Research0`, so the range holds every string beginning `Research/` and
/// nothing else. Used for both folders and tags, which are the same shape of
/// `/`-separated path.
///
/// A range rather than a prefix match so SQLite can seek an index to it.
fn descendant_range(path: &str) -> (String, String) {
    (format!("{path}/"), format!("{path}{AFTER_SEPARATOR}"))
}

/// Folders are stored with no leading or trailing slash, and the root is `""`.
fn normalise_folder(folder: &str) -> String {
    folder.trim().trim_matches('/').to_string()
}

/// A one-line English rendering of a query, for the view's header.
///
/// Not a round trip and not a syntax — the file is the query. This exists so a
/// view says what it is looking for above its results, because a list of notes
/// with no statement of why they are there is the thing that makes saved
/// searches rot.
pub fn describe(query: &Query) -> String {
    let phrase = |terms: &[Term], join: &str| -> Option<String> {
        let parts: Vec<String> = terms.iter().filter_map(describe_term).collect();
        (!parts.is_empty()).then(|| parts.join(join))
    };

    let mut out: Vec<String> = Vec::new();
    if let Some(p) = phrase(&query.all, ", ") {
        out.push(p);
    }
    if let Some(p) = phrase(&query.any, " or ") {
        out.push(format!("either {p}"));
    }
    if let Some(p) = phrase(&query.none, " or ") {
        out.push(format!("but not {p}"));
    }

    if out.is_empty() {
        return "Every note in the vault".into();
    }
    let mut sentence = format!("Notes {}", out.join(", "));
    sentence.push_str(match query.sort {
        Sort::Recent => ", most recently edited first",
        Sort::Stale => ", least recently edited first",
        Sort::Title => ", by title",
        Sort::Folder => ", by folder",
    });
    sentence
}

fn describe_term(term: &Term) -> Option<String> {
    let condition = match term {
        Term::Known(c) => c,
        Term::Unreadable(_) => return None,
    };
    Some(match condition {
        Condition::In(folder) if normalise_folder(folder).is_empty() => {
            "in the top level of the vault".into()
        }
        Condition::In(folder) => format!("in {}", normalise_folder(folder)),
        Condition::Under(folder) if normalise_folder(folder).is_empty() => "anywhere".into(),
        Condition::Under(folder) => format!("under {}", normalise_folder(folder)),
        Condition::Tag(tag) => format!("tagged #{}", tag.trim_matches('/')),
        Condition::Type(note_type) => format!("of type {}", note_type.as_str()),
        Condition::Cites(_) => "citing a particular source".into(),
        Condition::LinksTo(_) => "linking to a particular note".into(),
        Condition::Text(text) => format!("mentioning “{}”", text.trim()),
        Condition::UpdatedAfter(date) => format!("edited since {}", date.trim()),
        Condition::UpdatedBefore(date) => format!("untouched since {}", date.trim()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Query {
        serde_yaml_ng::from_str(yaml).expect("a view block should parse")
    }

    #[test]
    fn a_query_with_no_conditions_is_the_whole_vault() {
        // Not nothing. An empty view is one someone has started and not
        // finished, and showing them the vault makes that legible in a way an
        // empty list never does.
        let compiled = Query::default().compile();
        assert!(compiled.sql.contains("WHERE 1"), "{}", compiled.sql);
        assert_eq!(compiled.params.len(), 1, "just the limit");
    }

    #[test]
    fn under_compiles_to_a_range_rather_than_a_prefix_match() {
        // The difference between a view SQLite can seek an index for and one
        // it has to scan, and the reason a folder called `Data [raw]` works.
        let q = parse("all:\n  - under: Research/Sb2Se3\n");
        let compiled = q.compile();
        assert!(!compiled.sql.contains("LIKE"), "{}", compiled.sql);
        assert!(!compiled.sql.contains("GLOB"), "{}", compiled.sql);
        assert!(
            compiled.sql.contains(">=") && compiled.sql.contains('<'),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn a_descendant_range_holds_children_and_stops_at_the_next_name() {
        let (lo, hi) = descendant_range("Research");
        assert!(lo.as_str() <= "Research/Sb2Se3" && "Research/Sb2Se3" < hi.as_str());
        assert!(lo.as_str() <= "Research/a/b/c" && "Research/a/b/c" < hi.as_str());
        // A sibling whose name merely starts the same way is not inside it.
        assert!("Researchers" >= hi.as_str());
        // Nor is the tag `method/xrd-old` inside `method/xrd`.
        let (lo, hi) = descendant_range("method/xrd");
        assert!("method/xrd-old" < lo.as_str() || "method/xrd-old" >= hi.as_str());
    }

    #[test]
    fn the_three_lists_join_the_way_their_names_say() {
        let q = parse(
            "all:\n  - type: literature\n  - tag: xrd\n\
             any:\n  - in: A\n  - in: B\n\
             none:\n  - tag: archive\n",
        );
        let sql = q.compile().sql;
        // `all` is an AND, `any` is an OR, `none` is a negated OR — and the
        // three groups are ANDed together.
        assert!(sql.contains("AND"), "{sql}");
        assert!(sql.contains(" OR "), "{sql}");
        assert!(sql.contains("NOT ("), "{sql}");
    }

    #[test]
    fn parameters_come_out_in_the_order_the_sql_asks_for_them() {
        // Positional parameters, so a condition that pushes the wrong number
        // of them silently shifts every later one. Count them.
        let q = parse("all:\n  - under: A\n  - tag: t\n  - type: idea\n  - cites: X\n");
        let compiled = q.compile();
        let holes = compiled.sql.matches('?').count();
        assert_eq!(holes, compiled.params.len(), "{}", compiled.sql);
    }

    #[test]
    fn every_condition_pushes_as_many_parameters_as_it_uses() {
        // The same check across all of them, so adding a condition without a
        // matching parameter cannot pass unnoticed.
        for yaml in [
            "all: [{in: A}]",
            "all: [{under: A}]",
            "all: [{under: ''}]",
            "all: [{tag: a/b}]",
            "all: [{tag: ''}]",
            "all: [{type: literature}]",
            "all: [{cites: X}]",
            "all: [{links-to: X}]",
            "all: [{text: sulfide}]",
            "all: [{text: '  '}]",
            "all: [{updated-after: '2026-01-01'}]",
            "all: [{updated-before: '2026-01-01'}]",
        ] {
            let compiled = parse(yaml).compile();
            assert_eq!(
                compiled.sql.matches('?').count(),
                compiled.params.len(),
                "{yaml} -> {}",
                compiled.sql
            );
        }
    }

    #[test]
    fn a_term_this_version_cannot_read_is_skipped_but_not_lost() {
        // A view written by a newer Sutra, opened here. The unknown term must
        // not match anything, must not make the note unreadable, and must
        // still be in the file afterwards.
        let q = parse("all:\n  - tag: xrd\n  - written-by: alice\n");
        let compiled = q.compile();
        assert_eq!(compiled.ignored, 1);
        assert!(!compiled.sql.contains("alice"));
        assert_eq!(q.unreadable().len(), 1);

        let written = serde_yaml_ng::to_string(&q).unwrap();
        assert!(written.contains("written-by"), "{written}");
        assert!(written.contains("alice"), "{written}");
    }

    #[test]
    fn a_view_of_only_unreadable_terms_does_not_become_the_whole_vault() {
        // The dangerous shape of the previous test: if every term is skipped
        // the WHERE collapses to `1`, and a view meant to be narrow silently
        // returns everything. It has to say it ignored them.
        let compiled = parse("all:\n  - written-by: alice\n").compile();
        assert!(compiled.sql.contains("WHERE 1"));
        assert_eq!(
            compiled.ignored, 1,
            "so the UI can say the results are wrong"
        );
    }

    #[test]
    fn a_view_round_trips_through_yaml_unchanged() {
        let yaml = "all:\n- under: Research/Sb2Se3\n- tag: method/xrd\nnone:\n- tag: archive\nsort: title\nlimit: 50\n";
        let q: Query = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(serde_yaml_ng::to_string(&q).unwrap(), yaml);
    }

    #[test]
    fn every_condition_survives_a_round_trip() {
        // Two lists of YAML keys exist: the one `rename_all` gives the derived
        // Deserialize, and the one `Condition::key` writes. If they ever
        // disagree, Sutra writes view files it cannot read back — and the
        // symptom is a view that silently loses a term on the next save. So
        // every variant goes out and comes back here.
        let every = vec![
            Condition::In("Research".into()),
            Condition::Under("Research/Sb2Se3".into()),
            Condition::Tag("method/xrd".into()),
            Condition::Type(NoteType::Literature),
            Condition::Cites("01HQ3M8K2P".into()),
            Condition::LinksTo("01HQ3M8K2Q".into()),
            Condition::Text("thermal conductivity".into()),
            Condition::UpdatedAfter("2026-01-01".into()),
            Condition::UpdatedBefore("2026-06-01".into()),
        ];
        let query = Query {
            all: every.iter().cloned().map(Term::Known).collect(),
            ..Query::default()
        };
        let written = serde_yaml_ng::to_string(&query).unwrap();
        assert!(
            !written.contains('!'),
            "no YAML tags in a file people edit: {written}"
        );
        let read: Query = serde_yaml_ng::from_str(&written).unwrap();
        assert_eq!(read, query, "{written}");
    }

    #[test]
    fn an_absent_sort_and_limit_are_the_defaults() {
        let q = parse("all: [{tag: xrd}]");
        assert_eq!(q.sort, Sort::Recent);
        assert_eq!(q.limit(), DEFAULT_LIMIT);
        // And they are not written back, so a file stays as short as it was.
        let written = serde_yaml_ng::to_string(&q).unwrap();
        assert!(!written.contains("limit"), "{written}");
    }

    #[test]
    fn a_query_describes_itself_as_a_sentence() {
        let q = parse(
            "all:\n  - under: Research\n  - tag: method/xrd\n\
             none:\n  - type: source\nsort: title\n",
        );
        assert_eq!(
            describe(&q),
            "Notes under Research, tagged #method/xrd, but not of type source, by title"
        );
    }

    #[test]
    fn an_empty_query_describes_itself_honestly() {
        assert_eq!(describe(&Query::default()), "Every note in the vault");
    }

    #[test]
    fn a_folder_written_with_slashes_around_it_means_the_same_folder() {
        // Someone will type `/Research/`. Two views that differ only in
        // punctuation must not return different notes.
        let bare = parse("all: [{in: Research}]").compile();
        let slashed = parse("all: [{in: '/Research/'}]").compile();
        assert_eq!(bare.sql, slashed.sql);
        assert_eq!(
            format!("{:?}", describe(&parse("all: [{in: '/Research/'}]"))),
            format!("{:?}", describe(&parse("all: [{in: Research}]")))
        );
    }

    #[test]
    fn the_root_folder_is_expressible_and_under_the_root_is_everything() {
        assert_eq!(describe(&parse("all: [{in: ''}]")), {
            let mut s = "Notes in the top level of the vault".to_string();
            s.push_str(", most recently edited first");
            s
        });
        let compiled = parse("all: [{under: ''}]").compile();
        // "under the root" is the whole vault, and must not become a range
        // starting at `/` that matches nothing.
        assert!(compiled.sql.contains("(1)"), "{}", compiled.sql);
    }
}
