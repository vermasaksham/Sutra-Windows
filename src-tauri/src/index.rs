//! The SQLite index. Derived data only — delete it and nothing is lost.
//!
//! Everything in here is reconstructible from the markdown files by
//! [`Index::rebuild`]. Nothing is ever written to SQLite that does not already
//! exist in a note on disk. That is the rule the whole storage design rests on,
//! and it is what makes the database disposable rather than precious.

use crate::error::Result;
use crate::links;
use crate::vault::{NoteSummary, Vault};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

/// Bumped whenever the schema changes. On mismatch the index is dropped and
/// rebuilt rather than migrated — migrations are for data you cannot recreate,
/// and this is not that.
const SCHEMA_VERSION: i32 = 7;

const SCHEMA: &str = r#"
CREATE TABLE notes (
    id        TEXT PRIMARY KEY,
    -- Not the SQL keyword `type`, which would need quoting everywhere.
    note_type TEXT NOT NULL DEFAULT 'standard',
    title     TEXT NOT NULL,
    -- Vault-relative directory, '' for the root. Replaces a parent id: a
    -- note's location is where its file is, so this is copied from the path
    -- rather than from anything the note claims about itself.
    folder    TEXT NOT NULL,
    position  INTEGER NOT NULL,
    tags      TEXT NOT NULL,
    icon      TEXT,
    cover     TEXT,
    -- Derived from the body, like everything else here. Kept so the note list
    -- can show a preview without re-reading every file in the vault.
    excerpt   TEXT NOT NULL DEFAULT '',
    -- Both JSON, because the index is derived and its shape is nobody else's
    -- business. `source` is what a source note records about its paper;
    -- `sources` is what this note cites.
    source    TEXT,
    sources   TEXT NOT NULL DEFAULT '[]',
    updated   TEXT NOT NULL
);
CREATE INDEX notes_by_folder ON notes(folder, position);
-- Saved views sort by edit date more often than by anything else, and a view
-- over a large vault should seek this rather than sort the whole table.
CREATE INDEX notes_by_updated ON notes(updated);

-- Tags, one row each, so a view can seek an index instead of scanning every
-- note's JSON. The `tags` column on `notes` stays: it is what the note list
-- reads back, and this is the lookup side of the same fact.
CREATE TABLE note_tags (
    note_id TEXT NOT NULL,
    tag     TEXT NOT NULL,
    PRIMARY KEY (note_id, tag)
);
CREATE INDEX note_tags_by_tag ON note_tags(tag);

-- Which notes cite which sources, flattened so the reverse lookup is a query
-- rather than a scan of every note's frontmatter. Rebuilt from the markdown
-- like everything else here.
CREATE TABLE note_sources (
    source_id TEXT NOT NULL,
    note_id   TEXT NOT NULL,
    page      TEXT,
    PRIMARY KEY (source_id, note_id, page)
);
CREATE INDEX note_sources_by_source ON note_sources(source_id);

-- Contentless-adjacent: we store the text because the body is not otherwise in
-- the database, and FTS5 needs something to tokenise.
CREATE VIRTUAL TABLE notes_fts USING fts5(
    id UNINDEXED,
    title,
    body,
    -- Tags are indexed too, so clicking one finds the notes carrying it.
    -- Search then matches a tag and a prose mention of the same word alike,
    -- which for a research vault is closer to what was meant than an exact
    -- tag filter would be.
    tags,
    tokenize = "unicode61 remove_diacritics 2"
);

CREATE TABLE links (
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    PRIMARY KEY (source, target)
);
CREATE INDEX links_by_target ON links(target);
"#;

/// A note that cites a source, and where in it.
#[derive(Debug, Clone, Serialize)]
pub struct CitingNote {
    pub id: String,
    pub title: String,
    /// Where in the source, if the citation said.
    pub page: Option<String>,
}

/// One search hit.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    /// A fragment of the body with the match marked, straight from FTS5.
    pub excerpt: String,
}

/// What running a saved view returned.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewResult {
    pub notes: Vec<NoteSummary>,
    /// The query in English, for the header above the results. Computed here
    /// rather than in the frontend so there is one rendering of what a term
    /// means, not two that can disagree.
    pub description: String,
    /// The limit was reached, so there may be more. Said out loud rather than
    /// hidden, because a list that quietly stops is a list that lies.
    pub truncated: bool,
    /// Terms this version could not read and skipped. The file still holds
    /// them; the results were computed without them.
    pub ignored: usize,
}

/// A note that links to the one being viewed.
#[derive(Debug, Clone, Serialize)]
pub struct Backlink {
    pub id: String,
    pub title: String,
    pub excerpt: String,
}

pub struct Index {
    // The whole connection behind one lock. SQLite would serialise writes
    // anyway, and a notes vault is far too small for lock contention to matter.
    conn: Mutex<Connection>,
}

impl Index {
    /// Open (or create) the index database at `path`.
    ///
    /// A file that fails to open, or was written by a different schema, is
    /// discarded and recreated. Since every row is derived, throwing it away
    /// costs one rebuild and never costs data.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = match Connection::open(path) {
            Ok(c) if schema_matches(&c) => c,
            Ok(_) | Err(_) => {
                let _ = std::fs::remove_file(path);
                let conn = Connection::open(path)?;
                create_schema(&conn)?;
                conn
            }
        };

        // WAL keeps a read during a write from blocking, which matters because
        // the file watcher reindexes on its own thread while the UI queries.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.pragma_update(None, "foreign_keys", "ON")?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Discard everything and reindex the vault from its markdown files.
    ///
    /// This is the function that makes the index disposable, so it is also the
    /// one worth testing hardest: whatever it produces is the whole truth the
    /// rest of the app sees.
    pub fn rebuild(&self, vault: &Vault) -> Result<usize> {
        let mut guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = guard.transaction()?;

        tx.execute("DELETE FROM notes", [])?;
        tx.execute("DELETE FROM notes_fts", [])?;
        tx.execute("DELETE FROM links", [])?;

        let mut count = 0;
        for summary in vault.list_notes()? {
            // Read the body too — the summary alone cannot feed search or links.
            let body = vault
                .read_note(&summary.id)
                .map(|doc| doc.body)
                .unwrap_or_default();
            insert_note(&tx, &summary, &body)?;
            count += 1;
        }

        tx.commit()?;
        Ok(count)
    }

    /// Add or replace one note in the index.
    pub fn upsert(&self, summary: &NoteSummary, body: &str) -> Result<()> {
        let mut guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = guard.transaction()?;
        remove_note(&tx, &summary.id)?;
        insert_note(&tx, summary, body)?;
        tx.commit()?;
        Ok(())
    }

    /// Drop one note from the index.
    ///
    /// Links *from* it go too, but links *to* it are left alone: another note
    /// still contains that text, and the index must reflect the files as they
    /// are. Those become dangling links, which the UI shows as such.
    pub fn remove(&self, id: &str) -> Result<()> {
        let mut guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = guard.transaction()?;
        remove_note(&tx, id)?;
        tx.commit()?;
        Ok(())
    }

    /// Every note, ordered for tree building: siblings by position, then title.
    pub fn all_notes(&self) -> Result<Vec<NoteSummary>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = guard.prepare(
            "SELECT id, note_type, title, folder, position, tags, icon, cover, excerpt, source, sources, updated
             FROM notes
             ORDER BY folder, position, title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], row_to_summary)?;
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }

    /// Which notes cite a source, and where in it.
    ///
    /// The other half of provenance: a source note can show what has been built
    /// on it, which is what makes an evidence trail walkable in both directions.
    pub fn citing(&self, source_id: &str) -> Result<Vec<CitingNote>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = guard.prepare(
            "SELECT n.id, n.title, s.page
             FROM note_sources s
             JOIN notes n ON n.id = s.note_id
             WHERE s.source_id = ?1
             ORDER BY n.title COLLATE NOCASE, s.page",
        )?;
        let rows = stmt.query_map([source_id], |row| {
            Ok(CitingNote {
                id: row.get(0)?,
                title: row.get(1)?,
                page: row.get(2)?,
            })
        })?;
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }

    /// Full-text search over titles and bodies.
    ///
    /// Returns nothing for an empty query rather than everything: a search box
    /// the user has not typed into should show no results, not the whole vault.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = guard.prepare(
            "SELECT id, title, snippet(notes_fts, 2, '<mark>', '</mark>', '…', 12)
             FROM notes_fts
             WHERE notes_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![fts_query(query), limit as i64], |row| {
            Ok(SearchHit {
                id: row.get(0)?,
                title: row.get(1)?,
                excerpt: row.get(2)?,
            })
        })?;
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }

    /// Run a saved view.
    ///
    /// One statement, planned by SQLite, reading only the index. Nothing here
    /// opens, reads, stats or writes a note file: a view is a question about
    /// the vault, and asking a question must not be able to change the answer.
    pub fn run_view(&self, query: &crate::views::Query) -> Result<ViewResult> {
        let compiled = query.compile();
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = guard.prepare(&compiled.sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = compiled
            .params
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect();
        let rows = stmt.query_map(params.as_slice(), row_to_summary)?;
        let notes: Vec<NoteSummary> = rows.filter_map(std::result::Result::ok).collect();
        Ok(ViewResult {
            description: crate::views::describe(query),
            truncated: notes.len() == query.limit(),
            ignored: compiled.ignored,
            notes,
        })
    }

    /// Notes that link to `id`.
    pub fn backlinks(&self, id: &str) -> Result<Vec<Backlink>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = guard.prepare(
            "SELECT n.id, n.title,
                    COALESCE((SELECT substr(f.body, 1, 160) FROM notes_fts f
                              WHERE f.id = n.id), '')
             FROM links l
             JOIN notes n ON n.id = l.source
             WHERE l.target = ?1
             ORDER BY n.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([id], |row| {
            Ok(Backlink {
                id: row.get(0)?,
                title: row.get(1)?,
                excerpt: row.get(2)?,
            })
        })?;
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }
}

fn schema_matches(conn: &Connection) -> bool {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
        .map(|v| v == SCHEMA_VERSION)
        .unwrap_or(false)
}

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn insert_note(tx: &rusqlite::Transaction<'_>, note: &NoteSummary, body: &str) -> Result<()> {
    let tags = serde_json::to_string(&note.tags).unwrap_or_else(|_| "[]".into());
    tx.execute(
        "INSERT INTO notes (id, note_type, title, folder, position, tags, icon, cover, excerpt, source, sources, updated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            note.id,
            note.note_type.as_str(),
            note.title,
            note.folder,
            note.position,
            tags,
            note.icon,
            note.cover,
            note.excerpt,
            note.source
                .as_ref()
                .map(|s| serde_json::to_string(s).unwrap_or_else(|_| "null".into())),
            serde_json::to_string(&note.sources).unwrap_or_else(|_| "[]".into()),
            note.updated
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        ],
    )?;
    tx.execute(
        "INSERT INTO notes_fts (id, title, body, tags) VALUES (?1, ?2, ?3, ?4)",
        params![note.id, note.title, body, note.tags.join(" ")],
    )?;
    for tag in &note.tags {
        // Trimmed and de-duplicated by the primary key: `#xrd` and `#xrd/`
        // are one tag, and a note listing it twice is still one row.
        let tag = tag.trim().trim_matches('/');
        if tag.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO note_tags (note_id, tag) VALUES (?1, ?2)",
            params![note.id, tag],
        )?;
    }
    for citation in &note.sources {
        // A note citing one source at two pages is two rows; citing it twice at
        // the same page is one, which the primary key enforces.
        tx.execute(
            "INSERT OR IGNORE INTO note_sources (source_id, note_id, page)
             VALUES (?1, ?2, ?3)",
            params![citation.id, note.id, citation.page],
        )?;
    }
    for target in links::extract(body) {
        // A note linking to itself is not a backlink worth showing.
        if target == note.id {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO links (source, target) VALUES (?1, ?2)",
            params![note.id, target],
        )?;
    }
    Ok(())
}

fn remove_note(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<()> {
    tx.execute("DELETE FROM notes WHERE id = ?1", [id])?;
    tx.execute("DELETE FROM notes_fts WHERE id = ?1", [id])?;
    tx.execute("DELETE FROM links WHERE source = ?1", [id])?;
    tx.execute("DELETE FROM note_sources WHERE note_id = ?1", [id])?;
    tx.execute("DELETE FROM note_tags WHERE note_id = ?1", [id])?;
    Ok(())
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteSummary> {
    let note_type: String = row.get(1)?;
    let tags: String = row.get(5)?;
    let source: Option<String> = row.get(9)?;
    let sources: String = row.get(10)?;
    let updated: String = row.get(11)?;
    Ok(NoteSummary {
        id: row.get(0)?,
        note_type: crate::frontmatter::NoteType::parse(&note_type),
        title: row.get(2)?,
        folder: row.get(3)?,
        position: row.get(4)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        icon: row.get(6)?,
        cover: row.get(7)?,
        excerpt: row.get(8)?,
        source: source.and_then(|s| serde_json::from_str(&s).ok()),
        sources: serde_json::from_str(&sources).unwrap_or_default(),
        updated: time::OffsetDateTime::parse(
            &updated,
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH),
    })
}

/// Turn what a person typed into an FTS5 query.
///
/// FTS5's syntax has operators — quotes, `*`, `NEAR`, `AND` — and a raw string
/// containing an unbalanced quote or a bare `-` is a syntax error, not a
/// search. Someone typing `Sb2Se3 (run 3)` means it literally, so every term is
/// quoted, and a trailing `*` makes the last one a prefix so results narrow as
/// they type.
pub(crate) fn fts_query(input: &str) -> String {
    let mut terms: Vec<String> = input
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();

    if let Some(last) = terms.last_mut() {
        last.push('*');
    }
    terms.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use ulid::Ulid;

    struct Fixture {
        vault: Vault,
        index: Index,
        root: std::path::PathBuf,
        db: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("sutra-idx-{}", Ulid::generate()));
            std::fs::create_dir_all(&root).unwrap();
            let db = std::env::temp_dir().join(format!("sutra-idx-{}.sqlite", Ulid::generate()));
            Self {
                vault: Vault::open(root.clone()).unwrap(),
                index: Index::open(&db).unwrap(),
                root,
                db,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
            let _ = std::fs::remove_file(&self.db);
        }
    }

    #[test]
    fn rebuild_reflects_the_vault() {
        let f = Fixture::new();
        let a = f.vault.create_note("Alpha phase", None).unwrap();
        f.vault
            .save_note(&a.summary.id, "Alpha phase", "Selenide ribbons.")
            .unwrap();
        f.vault.create_note("Beta phase", None).unwrap();

        assert_eq!(f.index.rebuild(&f.vault).unwrap(), 2);
        let titles: Vec<_> = f
            .index
            .all_notes()
            .unwrap()
            .into_iter()
            .map(|n| n.title)
            .collect();
        assert!(titles.contains(&"Alpha phase".to_string()));
        assert!(titles.contains(&"Beta phase".to_string()));
    }

    #[test]
    fn deleting_the_database_loses_nothing() {
        // The rule the whole storage design rests on. If this ever fails, the
        // index has become a source of truth and the architecture is broken.
        let f = Fixture::new();
        let a = f.vault.create_note("Recoverable", None).unwrap();
        f.vault
            .save_note(&a.summary.id, "Recoverable", "Body worth finding.")
            .unwrap();
        f.index.rebuild(&f.vault).unwrap();
        let before = f.index.search("worth", 10).unwrap();
        assert_eq!(before.len(), 1);

        // Throw the database away entirely and start over from the files.
        drop(std::fs::remove_file(&f.db));
        let fresh = Index::open(&f.db).unwrap();
        fresh.rebuild(&f.vault).unwrap();

        let after = fresh.search("worth", 10).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].title, "Recoverable");
    }

    #[test]
    fn search_matches_body_and_title() {
        let f = Fixture::new();
        let a = f.vault.create_note("Antimony selenide", None).unwrap();
        f.vault
            .save_note(&a.summary.id, "Antimony selenide", "Quasi-1D ribbons.")
            .unwrap();
        f.index.rebuild(&f.vault).unwrap();

        assert_eq!(f.index.search("antimony", 10).unwrap().len(), 1);
        assert_eq!(f.index.search("ribbons", 10).unwrap().len(), 1);
        assert!(f.index.search("tungsten", 10).unwrap().is_empty());
    }

    #[test]
    fn search_is_prefix_matched_so_it_narrows_as_you_type() {
        let f = Fixture::new();
        let a = f.vault.create_note("Selenide", None).unwrap();
        f.vault
            .save_note(&a.summary.id, "Selenide", "Deposition notes.")
            .unwrap();
        f.index.rebuild(&f.vault).unwrap();

        for partial in ["dep", "depo", "deposition"] {
            assert_eq!(
                f.index.search(partial, 10).unwrap().len(),
                1,
                "prefix {partial:?} should match"
            );
        }
    }

    #[test]
    fn a_tag_is_findable_by_search() {
        let f = Fixture::new();
        let note = f.vault.create_note("Untagged title", None).unwrap();
        f.vault
            .save_note(&note.summary.id, "Untagged title", "Body without the word.")
            .unwrap();
        f.vault
            .set_meta(&note.summary.id, None, None, vec!["sb2se3".into()])
            .unwrap();
        f.index.rebuild(&f.vault).unwrap();

        // The tag appears in neither the title nor the body.
        let hits = f.index.search("sb2se3", 10).unwrap();
        assert_eq!(hits.len(), 1, "a tag should be findable");
        assert_eq!(hits[0].title, "Untagged title");
    }

    #[test]
    fn an_empty_query_returns_nothing() {
        let f = Fixture::new();
        f.vault.create_note("Something", None).unwrap();
        f.index.rebuild(&f.vault).unwrap();
        assert!(f.index.search("", 10).unwrap().is_empty());
        assert!(f.index.search("   ", 10).unwrap().is_empty());
    }

    #[test]
    fn punctuation_in_a_query_is_not_a_syntax_error() {
        // Raw FTS5 would reject several of these outright.
        let f = Fixture::new();
        let a = f.vault.create_note("Run notes", None).unwrap();
        f.vault
            .save_note(&a.summary.id, "Run notes", "Sb2Se3 (run 3) \"quoted\"")
            .unwrap();
        f.index.rebuild(&f.vault).unwrap();

        for query in ["Sb2Se3 (run 3)", "\"unbalanced", "-minus", "a OR b", "NEAR"] {
            assert!(
                f.index.search(query, 10).is_ok(),
                "query {query:?} should not error"
            );
        }
    }

    #[test]
    fn backlinks_point_the_right_way() {
        let f = Fixture::new();
        let target = f.vault.create_note("Target", None).unwrap();
        let source = f.vault.create_note("Source", None).unwrap();
        let target_id = target.summary.id.clone();

        f.vault
            .save_note(
                &source.summary.id,
                "Source",
                &format!("As described in [[{target_id}]]."),
            )
            .unwrap();
        f.index.rebuild(&f.vault).unwrap();

        let back = f.index.backlinks(&target_id).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].title, "Source");

        // And not the other way round.
        assert!(f.index.backlinks(&source.summary.id).unwrap().is_empty());
    }

    #[test]
    fn a_self_link_is_not_a_backlink() {
        let f = Fixture::new();
        let note = f.vault.create_note("Self", None).unwrap();
        let id = note.summary.id.clone();
        f.vault
            .save_note(&id, "Self", &format!("See [[{id}]]."))
            .unwrap();
        f.index.rebuild(&f.vault).unwrap();
        assert!(f.index.backlinks(&id).unwrap().is_empty());
    }

    #[test]
    fn removing_a_note_keeps_links_pointing_at_it() {
        // Deleting a note does not edit the notes that mention it, so the index
        // must still report those links — they dangle, and that is the truth.
        let f = Fixture::new();
        let target = f.vault.create_note("Target", None).unwrap();
        let source = f.vault.create_note("Source", None).unwrap();
        let target_id = target.summary.id.clone();
        f.vault
            .save_note(&source.summary.id, "Source", &format!("[[{target_id}]]"))
            .unwrap();
        f.index.rebuild(&f.vault).unwrap();

        f.index.remove(&target_id).unwrap();

        assert_eq!(f.index.backlinks(&target_id).unwrap().len(), 1);
        // The note itself is gone from the index even though links to it remain.
        assert!(
            f.index
                .all_notes()
                .unwrap()
                .iter()
                .all(|n| n.id != target_id)
        );
    }

    #[test]
    fn upsert_replaces_rather_than_duplicates() {
        let f = Fixture::new();
        let note = f.vault.create_note("First title", None).unwrap();
        f.index.rebuild(&f.vault).unwrap();

        let updated = f
            .vault
            .save_note(&note.summary.id, "Second title", "New body")
            .unwrap();
        f.index.upsert(&updated, "New body").unwrap();

        let all = f.index.all_notes().unwrap();
        assert_eq!(all.len(), 1, "upsert must not duplicate the row");
        assert_eq!(all[0].title, "Second title");
        assert_eq!(f.index.search("First", 10).unwrap().len(), 0);
        assert_eq!(f.index.search("Second", 10).unwrap().len(), 1);
    }

    #[test]
    fn stale_links_are_dropped_when_a_note_stops_linking() {
        let f = Fixture::new();
        let target = f.vault.create_note("Target", None).unwrap();
        let source = f.vault.create_note("Source", None).unwrap();
        let target_id = target.summary.id.clone();
        let updated = f
            .vault
            .save_note(&source.summary.id, "Source", &format!("[[{target_id}]]"))
            .unwrap();
        f.index
            .upsert(&updated, &format!("[[{target_id}]]"))
            .unwrap();
        assert_eq!(f.index.backlinks(&target_id).unwrap().len(), 1);

        // Remove the link from the body; the backlink must go with it.
        let updated = f
            .vault
            .save_note(&source.summary.id, "Source", "No links any more.")
            .unwrap();
        f.index.upsert(&updated, "No links any more.").unwrap();
        assert!(f.index.backlinks(&target_id).unwrap().is_empty());
    }

    #[test]
    fn a_schema_bump_discards_the_old_database() {
        let f = Fixture::new();
        f.vault.create_note("Note", None).unwrap();
        f.index.rebuild(&f.vault).unwrap();

        // Pretend the file was written by an older version of the app.
        {
            let conn = Connection::open(&f.db).unwrap();
            conn.pragma_update(None, "user_version", SCHEMA_VERSION - 1)
                .unwrap();
        }

        // Opening must not fail, and must give a usable empty index.
        let reopened = Index::open(&f.db).unwrap();
        assert!(reopened.all_notes().unwrap().is_empty());
        assert_eq!(reopened.rebuild(&f.vault).unwrap(), 1);
    }
    // ---- provenance --------------------------------------------------------

    /// The proof from the plan: one source cited from six notes, each at a
    /// different page. Changing the source's details updates every view of it
    /// and duplicates nothing.
    ///
    /// This is the whole argument for a source being a note. The six notes hold
    /// its id and nothing else — no copied title, no copied DOI — so there is
    /// exactly one place the details live and no way for six copies to drift.
    #[test]
    fn one_source_cited_six_times_stays_one_source() {
        let f = Fixture::new();
        let source = f
            .vault
            .create_source(
                "Quasi-1D Sb2Se3 ribbons",
                crate::frontmatter::SourceMeta {
                    authors: Some("Zhou, Y.".into()),
                    year: Some("2019".into()),
                    doi: Some("10.1000/before".into()),
                    zotero: Some("ABCD1234".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        f.index.upsert(&source.summary, &source.body).unwrap();

        let pages = ["3", "6", "6-8", "S12", "iv", "114"];
        let mut citing_ids = Vec::new();
        for page in pages {
            let note = f
                .vault
                .create_note(&format!("Reading note {page}"), Some("Research".into()))
                .unwrap();
            let summary = f
                .vault
                .set_citations(
                    &note.summary.id,
                    vec![crate::frontmatter::Citation {
                        id: source.summary.id.clone(),
                        page: Some(page.to_string()),
                        quote: Some(format!("what it says on {page}")),
                        captured: Some(crate::frontmatter::now()),
                    }],
                )
                .unwrap();
            f.index.upsert(&summary, "").unwrap();
            citing_ids.push(note.summary.id);
        }

        // All six are reachable from the source, with their pages.
        let citing = f.index.citing(&source.summary.id).unwrap();
        assert_eq!(citing.len(), 6, "{citing:?}");
        let mut seen: Vec<String> = citing.iter().filter_map(|c| c.page.clone()).collect();
        seen.sort();
        let mut expected: Vec<String> = pages.iter().map(|p| p.to_string()).collect();
        expected.sort();
        assert_eq!(seen, expected);

        // ---- change the source's DOI ----
        f.vault
            .set_source_meta(
                &source.summary.id,
                crate::frontmatter::SourceMeta {
                    authors: Some("Zhou, Y.".into()),
                    year: Some("2019".into()),
                    doi: Some("10.1000/after".into()),
                    zotero: Some("ABCD1234".into()),
                    ..Default::default()
                },
            )
            .unwrap();

        // Nothing duplicated: still one source note, still six citations.
        assert_eq!(f.vault.list_sources().unwrap().len(), 1);
        assert_eq!(f.index.citing(&source.summary.id).unwrap().len(), 6);

        // And every citing note sees the new DOI, because none of them has a
        // copy of it — they resolve through the one note that owns it.
        for id in &citing_ids {
            let note = f.vault.read_note(id).unwrap();
            assert_eq!(note.summary.sources[0].id, source.summary.id);
            let resolved = f.vault.read_note(&note.summary.sources[0].id).unwrap();
            assert_eq!(
                resolved.summary.source.unwrap().doi.as_deref(),
                Some("10.1000/after")
            );
        }
    }

    #[test]
    fn a_rebuild_reconstructs_the_source_relation_from_the_markdown() {
        // The invariant that makes the index safe to delete has to hold for
        // provenance too, or the evidence trail is really living in SQLite.
        let f = Fixture::new();
        let source = f
            .vault
            .create_source("Zhou 2019", Default::default())
            .unwrap();
        let note = f.vault.create_note("Citing", None).unwrap();
        f.vault
            .set_citations(
                &note.summary.id,
                vec![crate::frontmatter::Citation {
                    id: source.summary.id.clone(),
                    page: Some("6".into()),
                    quote: None,
                    captured: None,
                }],
            )
            .unwrap();

        f.index.rebuild(&f.vault).unwrap();

        let citing = f.index.citing(&source.summary.id).unwrap();
        assert_eq!(citing.len(), 1);
        assert_eq!(citing[0].id, note.summary.id);
        assert_eq!(citing[0].page.as_deref(), Some("6"));
    }

    #[test]
    fn dropping_a_citation_drops_the_relation() {
        let f = Fixture::new();
        let source = f.vault.create_source("S", Default::default()).unwrap();
        let note = f.vault.create_note("N", None).unwrap();
        let with = f
            .vault
            .set_citations(
                &note.summary.id,
                vec![crate::frontmatter::Citation {
                    id: source.summary.id.clone(),
                    page: None,
                    quote: None,
                    captured: None,
                }],
            )
            .unwrap();
        f.index.upsert(&with, "").unwrap();
        assert_eq!(f.index.citing(&source.summary.id).unwrap().len(), 1);

        let without = f.vault.set_citations(&note.summary.id, vec![]).unwrap();
        f.index.upsert(&without, "").unwrap();
        assert!(f.index.citing(&source.summary.id).unwrap().is_empty());
    }
    // ---- saved views ---------------------------------------------------------

    /// A view over the fixture's vault, by its YAML.
    fn view(f: &Fixture, yaml: &str) -> Vec<String> {
        let query: crate::views::Query = serde_yaml_ng::from_str(yaml).unwrap();
        f.index
            .run_view(&query)
            .unwrap()
            .notes
            .into_iter()
            .map(|n| n.title)
            .collect()
    }

    /// A note at a folder, with tags, indexed.
    fn note(f: &Fixture, folder: &str, title: &str, tags: &[&str], body: &str) -> String {
        let doc = f
            .vault
            .create_note(title, Some(folder.to_string()))
            .unwrap();
        f.vault.save_note(&doc.summary.id, title, body).unwrap();
        let summary = f
            .vault
            .set_meta(
                &doc.summary.id,
                None,
                None,
                tags.iter().map(|t| (*t).to_string()).collect(),
            )
            .unwrap();
        f.index.upsert(&summary, body).unwrap();
        doc.summary.id
    }

    #[test]
    fn under_a_folder_finds_descendants_and_not_siblings_that_start_the_same_way() {
        let f = Fixture::new();
        note(&f, "Research", "At the top", &[], "");
        note(&f, "Research/Sb2Se3", "One down", &[], "");
        note(&f, "Research/Sb2Se3/XRD", "Two down", &[], "");
        note(&f, "Researchers", "A different folder entirely", &[], "");

        let mut found = view(&f, "all: [{under: Research}]\nsort: title\n");
        found.sort();
        assert_eq!(found, ["At the top", "One down", "Two down"]);

        // `in` is that folder and nothing beneath it.
        assert_eq!(view(&f, "all: [{in: Research}]"), ["At the top"]);
    }

    #[test]
    fn a_folder_named_with_a_glob_character_matches_literally() {
        // The reason `under` is a range and not LIKE or GLOB. `Data [raw]` is
        // an ordinary folder name and a bracket in a GLOB pattern is a
        // character class, so this would silently return nothing.
        let f = Fixture::new();
        note(&f, "Data [raw]/2026", "Under the awkward name", &[], "");
        note(
            &f,
            "Data x/2026",
            "Under a name a glob would also match",
            &[],
            "",
        );
        assert_eq!(
            view(&f, "all: [{under: 'Data [raw]'}]"),
            ["Under the awkward name"]
        );
    }

    #[test]
    fn a_tag_view_includes_the_tags_beneath_it() {
        // A hierarchical tag is a category. Asking for `method` and not being
        // shown `method/xrd` would make the tag tree a lie.
        let f = Fixture::new();
        note(&f, "", "XRD run", &["method/xrd"], "");
        note(&f, "", "Raman run", &["method/raman"], "");
        note(&f, "", "Methodology", &["method"], "");
        note(&f, "", "Older XRD work", &["method/xrd-old"], "");
        note(&f, "", "Unrelated", &["sb2se3"], "");

        let mut found = view(&f, "all: [{tag: method}]\nsort: title\n");
        found.sort();
        assert_eq!(
            found,
            ["Methodology", "Older XRD work", "Raman run", "XRD run"]
        );

        // But `method/xrd` must not swallow `method/xrd-old`: a tag whose name
        // merely starts the same way is a different tag.
        assert_eq!(view(&f, "all: [{tag: method/xrd}]"), ["XRD run"]);
    }

    #[test]
    fn all_any_and_none_mean_what_they_say() {
        let f = Fixture::new();
        note(&f, "Research", "Kept", &["xrd"], "");
        note(&f, "Research", "Excluded by none", &["xrd", "archive"], "");
        note(&f, "Elsewhere", "Excluded by all", &["xrd"], "");
        note(&f, "Research", "Excluded by tag", &["raman"], "");

        assert_eq!(
            view(
                &f,
                "all:\n  - in: Research\n  - tag: xrd\nnone:\n  - tag: archive\n"
            ),
            ["Kept"]
        );
    }

    #[test]
    fn any_is_a_union_and_an_empty_any_is_not_a_filter() {
        let f = Fixture::new();
        note(&f, "A", "In A", &[], "");
        note(&f, "B", "In B", &[], "");
        note(&f, "C", "In C", &[], "");

        let mut found = view(&f, "any:\n  - in: A\n  - in: B\n");
        found.sort();
        assert_eq!(found, ["In A", "In B"]);

        // `all` alone, with `any` absent, must not be intersected with nothing.
        assert_eq!(view(&f, "all: [{in: C}]"), ["In C"]);
    }

    #[test]
    fn a_view_can_ask_about_text_citations_and_links() {
        let f = Fixture::new();
        let source = f
            .vault
            .create_source(
                "Zhou 2019",
                crate::frontmatter::SourceMeta {
                    year: Some("2019".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        f.index.upsert(&source.summary, "").unwrap();

        let target = note(&f, "", "The linked note", &[], "");
        let citing = note(&f, "", "Cites the paper", &[], "Selenide ribbons.");
        f.vault
            .set_citations(
                &citing,
                vec![crate::frontmatter::Citation {
                    id: source.summary.id.clone(),
                    page: Some("S12".into()),
                    quote: None,
                    captured: None,
                }],
            )
            .unwrap();
        let citing_summary = f.vault.read_note(&citing).unwrap().summary;
        f.index
            .upsert(&citing_summary, "Selenide ribbons.")
            .unwrap();

        let linking = note(&f, "", "Links to it", &[], &format!("See [[{target}]]."));
        let _ = linking;

        assert_eq!(view(&f, "all: [{text: selenide}]"), ["Cites the paper"]);
        assert_eq!(
            view(&f, &format!("all: [{{cites: {}}}]", source.summary.id)),
            ["Cites the paper"]
        );
        assert_eq!(
            view(&f, &format!("all: [{{links-to: {target}}}]")),
            ["Links to it"]
        );
    }

    #[test]
    fn a_view_says_when_it_stopped_short() {
        // A list that quietly stops at its limit is a list that lies about the
        // vault. This is the flag the header reads.
        let f = Fixture::new();
        for i in 0..5 {
            note(&f, "", &format!("Note {i}"), &[], "");
        }
        let query: crate::views::Query = serde_yaml_ng::from_str("limit: 3").unwrap();
        let result = f.index.run_view(&query).unwrap();
        assert_eq!(result.notes.len(), 3);
        assert!(result.truncated);

        let query: crate::views::Query = serde_yaml_ng::from_str("limit: 50").unwrap();
        assert!(!f.index.run_view(&query).unwrap().truncated);
    }

    #[test]
    fn evaluating_a_view_touches_no_note_file() {
        // The other half of the step's proof, and the line that keeps a view a
        // question rather than an operation. A saved search that rewrote
        // frontmatter — a `lastViewed`, a cached result set, a stamped
        // `updated` — would make reading the vault change it, and would show
        // up months later as five hundred notes that all claim to have been
        // edited on the same afternoon.
        let f = Fixture::new();
        note(&f, "Research", "Alpha", &["xrd"], "Selenide ribbons.");
        note(
            &f,
            "Research/Sb2Se3",
            "Beta",
            &["xrd", "archive"],
            "Antimony.",
        );
        note(&f, "Elsewhere", "Gamma", &["raman"], "Raman shift.");
        let view_note = f
            .vault
            .create_view(
                "Everything XRD",
                serde_yaml_ng::from_str("all: [{tag: xrd}]\nnone: [{tag: archive}]").unwrap(),
            )
            .unwrap();

        /// Every markdown file in the vault: what it says and when it was last
        /// written. The index database is deliberately not included — it is
        /// derived, and SQLite may touch it freely.
        fn snapshot(
            root: &std::path::Path,
        ) -> Vec<(std::path::PathBuf, Vec<u8>, std::time::SystemTime)> {
            let mut out = Vec::new();
            let mut stack = vec![root.to_path_buf()];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.extension().is_some_and(|e| e == "md") {
                        let meta = std::fs::metadata(&path).unwrap();
                        out.push((
                            path.clone(),
                            std::fs::read(&path).unwrap(),
                            meta.modified().unwrap(),
                        ));
                    }
                }
            }
            out.sort_by(|a, b| a.0.cmp(&b.0));
            out
        }

        f.index.rebuild(&f.vault).unwrap();
        let before = snapshot(&f.root);
        assert_eq!(before.len(), 4, "three notes and the view itself");

        // The realistic path: read the view's query from its file, then run it.
        let query = f.vault.view_query(&view_note.summary.id).unwrap().unwrap();
        for _ in 0..20 {
            let result = f.index.run_view(&query).unwrap();
            assert_eq!(
                result
                    .notes
                    .iter()
                    .map(|n| n.title.as_str())
                    .collect::<Vec<_>>(),
                ["Alpha"]
            );
        }

        assert_eq!(
            before,
            snapshot(&f.root),
            "evaluating a view rewrote a note"
        );
    }

    #[test]
    fn a_view_over_five_thousand_notes_is_answered_from_the_index() {
        // The step's proof. Five thousand notes, a query touching folder, tag
        // and type at once, answered in one statement.
        //
        // 50 ms is the number the plan committed to. This runs in a debug
        // build, where both Rust and the bundled SQLite are unoptimised, and
        // it still comes in around 5 ms — so the ceiling is the promise, not
        // a threshold tuned to just pass.
        let f = Fixture::new();
        let now = crate::frontmatter::now();
        {
            let guard = f.index.conn.lock().unwrap();
            let tx = guard.unchecked_transaction().unwrap();
            for i in 0..5_000 {
                let summary = NoteSummary {
                    id: Ulid::generate().to_string(),
                    note_type: if i % 7 == 0 {
                        crate::frontmatter::NoteType::Literature
                    } else {
                        crate::frontmatter::NoteType::Standard
                    },
                    title: format!("Note {i}"),
                    folder: format!("Research/Batch{}", i % 50),
                    position: i,
                    tags: vec![format!("method/m{}", i % 30), "sb2se3".into()],
                    icon: None,
                    cover: None,
                    source: None,
                    sources: Vec::new(),
                    excerpt: String::new(),
                    updated: now,
                };
                insert_note(&tx, &summary, "Body text about selenide ribbons.").unwrap();
            }
            tx.commit().unwrap();
        }

        let query: crate::views::Query = serde_yaml_ng::from_str(
            "all:\n  - under: Research\n  - tag: method/m3\n  - type: literature\n\
             none:\n  - tag: archive\nlimit: 500\n",
        )
        .unwrap();

        // Once to warm SQLite's page cache, then the measured run.
        f.index.run_view(&query).unwrap();
        let started = std::time::Instant::now();
        let result = f.index.run_view(&query).unwrap();
        let elapsed = started.elapsed();

        assert!(!result.notes.is_empty(), "the query should match something");
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "a view over 5,000 notes took {elapsed:?}"
        );
        eprintln!(
            "view over 5,000 notes: {} results in {elapsed:?}",
            result.notes.len()
        );

        // And it is fast because SQLite is seeking indexes, not because 5,000
        // rows is small. A plan that says SCAN here is a plan that will stop
        // being fast at 50,000.
        let compiled = query.compile();
        let guard = f.index.conn.lock().unwrap();
        let mut stmt = guard
            .prepare(&format!("EXPLAIN QUERY PLAN {}", compiled.sql))
            .unwrap();
        let params: Vec<&dyn rusqlite::ToSql> = compiled
            .params
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect();
        let plan: Vec<String> = stmt
            .query_map(params.as_slice(), |row| row.get::<_, String>(3))
            .unwrap()
            .filter_map(std::result::Result::ok)
            .collect();
        let plan = plan.join("\n");
        // Every table access is a SEARCH — an index seek — and not one is a
        // SCAN. Which index SQLite picks is its business and changes with the
        // shape of the data; that it never falls back to walking a table is
        // the property that keeps this fast at fifty thousand notes as well as
        // at five.
        for line in plan.lines() {
            assert!(
                !line.trim_start().starts_with("SCAN"),
                "a view should never scan a table:\n{plan}"
            );
        }
        assert!(
            plan.contains("note_tags_by_tag"),
            "the tag term should seek the tag index this step added:\n{plan}"
        );
    }
}
