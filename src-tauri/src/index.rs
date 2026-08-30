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
const SCHEMA_VERSION: i32 = 3;

const SCHEMA: &str = r#"
CREATE TABLE notes (
    id        TEXT PRIMARY KEY,
    title     TEXT NOT NULL,
    parent    TEXT,
    position  INTEGER NOT NULL,
    tags      TEXT NOT NULL,
    icon      TEXT,
    cover     TEXT,
    -- Derived from the body, like everything else here. Kept so the note list
    -- can show a preview without re-reading every file in the vault.
    excerpt   TEXT NOT NULL DEFAULT '',
    updated   TEXT NOT NULL
);
CREATE INDEX notes_by_parent ON notes(parent, position);

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

/// One search hit.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    /// A fragment of the body with the match marked, straight from FTS5.
    pub excerpt: String,
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
            "SELECT id, title, parent, position, tags, icon, cover, excerpt, updated
             FROM notes
             ORDER BY position, title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], row_to_summary)?;
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
        "INSERT INTO notes (id, title, parent, position, tags, icon, cover, excerpt, updated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            note.id,
            note.title,
            note.parent,
            note.position,
            tags,
            note.icon,
            note.cover,
            note.excerpt,
            note.updated
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        ],
    )?;
    tx.execute(
        "INSERT INTO notes_fts (id, title, body, tags) VALUES (?1, ?2, ?3, ?4)",
        params![note.id, note.title, body, note.tags.join(" ")],
    )?;
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
    Ok(())
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteSummary> {
    let tags: String = row.get(4)?;
    let updated: String = row.get(8)?;
    Ok(NoteSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        parent: row.get(2)?,
        position: row.get(3)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        icon: row.get(5)?,
        cover: row.get(6)?,
        excerpt: row.get(7)?,
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
fn fts_query(input: &str) -> String {
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
}
