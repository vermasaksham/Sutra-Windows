//! The SQLite index. Derived data only — delete it and nothing is lost.
//!
//! Everything in here is reconstructible from the markdown files by
//! [`Index::rebuild`]. Nothing is ever written to SQLite that does not already
//! exist in a note on disk. That is the rule the whole storage design rests on,
//! and it is what makes the database disposable rather than precious.

use crate::claims::{self, Disagreement};
use crate::duplicates::{self, Duplicate};
use crate::error::Result;
use crate::links;
use crate::related::{self, Candidate, Reason, Related};
use crate::vault::{NoteSummary, Vault};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

/// Bumped whenever the schema changes. On mismatch the index is dropped and
/// rebuilt rather than migrated — migrations are for data you cannot recreate,
/// and this is not that.
const SCHEMA_VERSION: i32 = 8;

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

-- FTS5's own term dictionary, exposed as a table: (term, doc, cnt). Not
-- storage — a view over the index that already exists — and the only honest
-- source of "how many notes use this word", which is what makes a shared word
-- weighable rather than merely countable.
CREATE VIRTUAL TABLE notes_vocab USING fts5vocab(notes_fts, 'row');

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

    /// Notes near this one, and why.
    ///
    /// Five signals, gathered separately and merged: shared sources, shared
    /// tags weighted by how rare they are, shared links (with a link to a
    /// project note singled out), shared distinctive words, and the folder as
    /// a tiebreak. Each contributes a [`Reason`] the panel can show, so the
    /// ranking and the explanation are the same data rather than two things
    /// that can disagree.
    ///
    /// Notes already shown elsewhere in the panel are left out: a note that
    /// links to this one is a backlink, and repeating it here as "related"
    /// would fill a short list with rows the reader has already seen.
    pub fn related(&self, id: &str, body: &str, limit: usize) -> Result<Vec<Related>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let Some((folder, tags)) = note_facts(&guard, id)? else {
            return Ok(Vec::new());
        };
        let total: usize =
            guard.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get::<_, i64>(0))? as usize;

        // Everything already visible in the panel, plus the note itself.
        let mut skip: HashSet<String> = HashSet::from([id.to_string()]);
        skip.extend(neighbours(&guard, id)?);
        skip.extend(cited(&guard, id)?);

        let mut found: HashMap<String, Vec<Reason>> = HashMap::new();
        let mut note = |id: String, reason: Reason, skip: &HashSet<String>| {
            if !skip.contains(&id) {
                found.entry(id).or_default().push(reason);
            }
        };

        // ---- shared sources ----
        let mut stmt = guard.prepare(
            "SELECT other.note_id, COALESCE(source.title, '')
               FROM note_sources mine
               JOIN note_sources other ON other.source_id = mine.source_id
               LEFT JOIN notes source ON source.id = mine.source_id
              WHERE mine.note_id = ?1 AND other.note_id <> ?1",
        )?;
        for row in stmt.query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (other, title) = row?;
            note(other, Reason::Source { title }, &skip);
        }

        // ---- shared tags ----
        for tag in &tags {
            let carrying: usize = guard.query_row(
                "SELECT COUNT(*) FROM note_tags WHERE tag = ?1",
                [tag],
                |r| r.get::<_, i64>(0),
            )? as usize;
            let idf = related::idf(total, carrying);
            let mut stmt =
                guard.prepare("SELECT note_id FROM note_tags WHERE tag = ?1 AND note_id <> ?2")?;
            for row in stmt.query_map(params![tag, id], |r| r.get::<_, String>(0))? {
                note(
                    row?,
                    Reason::Tag {
                        tag: tag.clone(),
                        idf,
                    },
                    &skip,
                );
            }
        }

        // ---- shared links ----
        // Both pointing at a third note. A link to a project note is reported
        // as membership, because "both in PhD Thesis" is what it means.
        let mut stmt = guard.prepare(
            "SELECT other.source, COALESCE(target.title, ''), COALESCE(target.note_type, '')
               FROM links mine
               JOIN links other ON other.target = mine.target
               LEFT JOIN notes target ON target.id = mine.target
              WHERE mine.source = ?1 AND other.source <> ?1",
        )?;
        for row in stmt.query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (other, title, kind) = row?;
            // A link to a note the index has never seen has no title to show,
            // so it is a link that says nothing and is not a reason.
            if title.is_empty() {
                continue;
            }
            let reason = if kind == "project" {
                Reason::Project { title }
            } else {
                Reason::CoLink { title }
            };
            note(other, reason, &skip);
        }

        // ---- shared distinctive words ----
        let mut shared: HashMap<String, (usize, f64)> = HashMap::new();
        for (term, idf) in distinctive(&guard, body, total)? {
            let mut stmt = guard.prepare(
                "SELECT id FROM notes_fts WHERE notes_fts MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let quoted = format!("\"{}\"", term.replace('"', "\"\""));
            for row in
                stmt.query_map(params![quoted, PER_TERM as i64], |r| r.get::<_, String>(0))?
            {
                let other = row?;
                if other == id || skip.contains(&other) {
                    continue;
                }
                let entry = shared.entry(other).or_insert((0, 0.0));
                entry.0 += 1;
                entry.1 += idf;
            }
        }
        for (other, (count, idf)) in shared {
            note(other, Reason::Terms { count, idf }, &skip);
        }

        // ---- the folder, as a tiebreak ----
        for id in found.keys().cloned().collect::<Vec<_>>() {
            let same: bool = guard.query_row(
                "SELECT folder = ?2 FROM notes WHERE id = ?1",
                params![id, folder],
                |r| r.get(0),
            )?;
            if same {
                found.entry(id).or_default().push(Reason::Folder);
            }
        }

        // ---- rank ----
        let mut candidates = Vec::with_capacity(found.len());
        for (id, reasons) in found {
            let Ok((title, folder)) = guard.query_row(
                "SELECT title, folder FROM notes WHERE id = ?1",
                [&id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            ) else {
                continue;
            };
            candidates.push(Candidate {
                id,
                title,
                folder,
                reasons,
            });
        }
        Ok(related::rank(candidates, limit))
    }

    /// Notes that may be this note written twice.
    ///
    /// Candidates come from FTS on the title's words — cheap, indexed, and
    /// generous — and are then compared properly. Nothing is decided here: the
    /// result is a list with a sentence attached, and merging is a button a
    /// person presses.
    pub fn duplicates(
        &self,
        id: &str,
        title: &str,
        body: &str,
        dismissed: &[String],
        limit: usize,
    ) -> Result<Vec<Duplicate>> {
        let query = title_query(title);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let skip: HashSet<&str> = dismissed.iter().map(String::as_str).collect();

        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = guard.prepare(
            "SELECT f.id, f.title, f.body, COALESCE(n.folder, '') \
               FROM notes_fts f LEFT JOIN notes n ON n.id = f.id \
              WHERE notes_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, CANDIDATES as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut found = Vec::new();
        for row in rows {
            let (other, other_title, other_body, folder) = row?;
            if other == id || skip.contains(other.as_str()) {
                continue;
            }
            let alike = duplicates::compare(title, body, &other_title, &other_body);
            found.push(Duplicate {
                id: other,
                title: other_title,
                folder,
                reason: alike.explain(),
                score: alike.score(),
            });
        }
        Ok(duplicates::rank(found, limit))
    }

    /// Numeric claims in this note that disagree with one in a note it is
    /// connected to.
    ///
    /// Connected means sharing a tag or a source, linking to each other, or
    /// both pointing at a third note. Two notes with nothing between them are
    /// not in conversation, and comparing their numbers would flag every pair
    /// of measurements in the vault against every other.
    pub fn disagreements(&self, id: &str, body: &str, limit: usize) -> Result<Vec<Disagreement>> {
        let mine = claims::claims(body);
        if mine.is_empty() {
            return Ok(Vec::new());
        }

        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = guard.prepare(
            "SELECT n.id, n.title, f.body FROM notes n JOIN notes_fts f ON f.id = n.id \
              WHERE n.id <> ?1 AND ( \
                n.id IN (SELECT b.note_id FROM note_tags a JOIN note_tags b ON b.tag = a.tag \
                          WHERE a.note_id = ?1) \
                OR n.id IN (SELECT b.note_id FROM note_sources a JOIN note_sources b \
                             ON b.source_id = a.source_id WHERE a.note_id = ?1) \
                OR n.id IN (SELECT target FROM links WHERE source = ?1) \
                OR n.id IN (SELECT source FROM links WHERE target = ?1) \
                OR n.id IN (SELECT b.source FROM links a JOIN links b ON b.target = a.target \
                             WHERE a.source = ?1)) \
              LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![id, CONNECTED as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (other, title, other_body) = row?;
            for theirs in claims::claims(&other_body) {
                for ours in &mine {
                    if !claims::disagree(ours, &theirs) {
                        continue;
                    }
                    out.push(Disagreement {
                        label: ours.label.clone(),
                        here: ours.text.clone(),
                        id: other.clone(),
                        title: title.clone(),
                        there: theirs.text.clone(),
                        factor: claims::factor(ours.value, theirs.value).unwrap_or(0.0),
                    });
                }
            }
        }
        // Widest apart first: a value out by a thousand is a unit error worth
        // seeing before one out by three.
        out.sort_by(|a, b| {
            b.factor
                .partial_cmp(&a.factor)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.title.cmp(&b.title))
        });
        out.truncate(limit);
        Ok(out)
    }

    /// Every pair of notes in the vault that may be duplicates.
    ///
    /// The tidying pass, run on request rather than in the background. Each
    /// pair is reported once, from whichever side comes first.
    pub fn duplicate_pairs(&self, limit: usize) -> Result<Vec<DuplicatePair>> {
        let notes: Vec<(String, String, String, String)> = {
            let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
            let mut stmt = guard.prepare(
                "SELECT n.id, n.title, f.body, n.folder FROM notes n \
                   JOIN notes_fts f ON f.id = n.id ORDER BY n.title",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;
            rows.filter_map(std::result::Result::ok).collect()
        };

        // Bucketed by normalised title before anything is compared. Two notes
        // whose titles share no word cannot reach the floor, so comparing them
        // is work with a known answer — and without the buckets this is every
        // note against every other.
        let mut by_word: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut words: Vec<Vec<String>> = Vec::with_capacity(notes.len());
        for (_, title, _, _) in &notes {
            words.push(
                duplicates::normalise_title(title)
                    .split(' ')
                    .filter(|w| !w.is_empty())
                    .map(str::to_string)
                    .collect(),
            );
        }
        for (i, note_words) in words.iter().enumerate() {
            for word in note_words {
                by_word.entry(word.as_str()).or_default().push(i);
            }
        }

        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        let mut found = Vec::new();
        for (i, note_words) in words.iter().enumerate() {
            for word in note_words {
                for &j in by_word.get(word.as_str()).into_iter().flatten() {
                    if j <= i || !seen.insert((i, j)) {
                        continue;
                    }
                    let alike =
                        duplicates::compare(&notes[i].1, &notes[i].2, &notes[j].1, &notes[j].2);
                    if alike.score() < duplicates::FLOOR {
                        continue;
                    }
                    found.push(DuplicatePair {
                        left: notes[i].0.clone(),
                        left_title: notes[i].1.clone(),
                        left_folder: notes[i].3.clone(),
                        right: notes[j].0.clone(),
                        right_title: notes[j].1.clone(),
                        right_folder: notes[j].3.clone(),
                        reason: alike.explain(),
                        score: alike.score(),
                    });
                }
            }
        }
        found.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.left_title.cmp(&b.left_title))
        });
        found.truncate(limit);
        Ok(found)
    }

    /// The other notes in this note's folder, most recently edited first.
    pub fn folder_neighbours(&self, id: &str, limit: usize) -> Result<Vec<NoteSummary>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = guard.prepare(
            "SELECT n.id, n.note_type, n.title, n.folder, n.position, n.tags, n.icon, n.cover, \
                    n.excerpt, n.source, n.sources, n.updated \
               FROM notes n \
              WHERE n.folder = (SELECT folder FROM notes WHERE id = ?1) AND n.id <> ?1 \
              ORDER BY n.updated DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![id, limit as i64], row_to_summary)?;
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

/// How many notes one shared word may contribute.
///
/// Bounds the work: a dozen terms, each returning at most this many
/// best-matching notes, is a few hundred rows however large the vault is. The
/// rows are ordered by FTS5's own relevance, so the cap drops the weakest
/// matches rather than an arbitrary slice.
const PER_TERM: usize = 40;

/// How many of a note's words are worth asking about.
const TERM_BUDGET: usize = 12;

/// A note's folder and tags, or `None` if the index has never seen it.
fn note_facts(conn: &Connection, id: &str) -> Result<Option<(String, Vec<String>)>> {
    let mut stmt = conn.prepare("SELECT folder FROM notes WHERE id = ?1")?;
    let Ok(folder) = stmt.query_row([id], |r| r.get::<_, String>(0)) else {
        return Ok(None);
    };
    let mut stmt = conn.prepare("SELECT tag FROM note_tags WHERE note_id = ?1")?;
    let tags = stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(Some((folder, tags)))
}

/// Every note directly linked to or from `id`.
///
/// These are already in the panel as backlinks, or visible in the prose as
/// links. Repeating them under "related" would spend a short list on rows the
/// reader has seen.
fn neighbours(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT target FROM links WHERE source = ?1
         UNION SELECT source FROM links WHERE target = ?1",
    )?;
    Ok(stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .filter_map(std::result::Result::ok)
        .collect())
}

/// The sources `id` cites.
///
/// Already listed in the panel's own Sources section, with the page and the
/// quote. A source note reappearing under "related" because its title happens
/// to share a word is the same row twice.
fn cited(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT source_id FROM note_sources WHERE note_id = ?1")?;
    Ok(stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .filter_map(std::result::Result::ok)
        .collect())
}

/// The words in `body` worth looking for elsewhere, with what each is worth.
///
/// Distinctive means "used here and in a few other notes". A word in no other
/// note finds nothing; a word in most of them says nothing about either note.
/// The window between is where the useful signal lives, and it is read from
/// FTS5's own term dictionary rather than guessed at, so it moves with the
/// vault instead of being a constant someone picked once.
fn distinctive(conn: &Connection, body: &str, total: usize) -> Result<Vec<(String, f64)>> {
    // Above this share of the vault a word is furniture, not a subject.
    let ceiling = (total / 4).max(2);
    let mut scored: Vec<(usize, String, f64)> = Vec::new();
    let mut stmt = conn.prepare("SELECT doc FROM notes_vocab WHERE term = ?1")?;
    for term in related::terms(body) {
        let carrying = stmt
            .query_row([&term], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            .max(0) as usize;
        if carrying == 0 || carrying > ceiling {
            continue;
        }
        // Two buckets, and the first is spent before the second is touched.
        //
        // A word in two or more notes is certainly in one other than this
        // one. A word in exactly one note usually *is* this one — worth
        // nothing — but not always: while a note is being typed its edits are
        // not in the index, so a word it now shares with one other note counts
        // only that other note. Preferring the first bucket means the common
        // case never spends its budget on words that match nothing, and the
        // unsaved case is still served by what is left.
        let bucket = usize::from(carrying < 2);
        scored.push((bucket, term, related::idf(total, carrying)));
    }
    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.truncate(TERM_BUDGET);
    Ok(scored
        .into_iter()
        .map(|(_, term, idf)| (term, idf))
        .collect())
}

/// How many FTS hits to compare properly when looking for a duplicate.
const CANDIDATES: usize = 30;

/// How many connected notes to read claims from.
///
/// A cap, because a common tag can connect a note to hundreds and reading
/// every one of their bodies to compare arithmetic is not worth the wait.
const CONNECTED: usize = 200;

/// An FTS query matching any of a title's words.
///
/// Deliberately generous: this only produces candidates, and the comparison
/// afterwards is what decides. Missing a duplicate here cannot be recovered
/// later, whereas an extra candidate costs one comparison.
fn title_query(title: &str) -> String {
    let terms: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    terms.join(" OR ")
}

/// A pair of notes that may be duplicates, for the vault-wide list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatePair {
    pub left: String,
    pub left_title: String,
    pub left_folder: String,
    pub right: String,
    pub right_title: String,
    pub right_folder: String,
    pub reason: String,
    pub score: f64,
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

    /// Every markdown file in the vault: what it says and when it was last
    /// written. The index database is deliberately not included — it is
    /// derived, and SQLite may touch it freely.
    ///
    /// Shared by the tests that prove reading the vault does not change it.
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
    // ---- the context panel ---------------------------------------------------

    /// The reasons given for each neighbour of `id`, best first.
    fn near(f: &Fixture, id: &str) -> Vec<(String, String)> {
        let body = f.vault.read_note(id).unwrap().body;
        f.index
            .related(id, &body, 5)
            .unwrap()
            .into_iter()
            .map(|r| (r.title, r.reason))
            .collect()
    }

    #[test]
    fn a_shared_source_makes_two_notes_related_and_says_so() {
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

        let cites = |title: &str| {
            let id = note(&f, "Research", title, &[], "Some prose.");
            f.vault
                .set_citations(
                    &id,
                    vec![crate::frontmatter::Citation {
                        id: source.summary.id.clone(),
                        page: None,
                        quote: None,
                        captured: None,
                    }],
                )
                .unwrap();
            id
        };
        let a = cites("Phonon transport");
        cites("Seebeck coefficient");
        note(
            &f,
            "Research",
            "Nothing to do with it",
            &[],
            "Unrelated prose.",
        );
        f.index.rebuild(&f.vault).unwrap();

        let found = near(&f, &a);
        assert_eq!(
            found.len(),
            1,
            "the unrelated note should not be near: {found:?}"
        );
        assert_eq!(found[0].0, "Seebeck coefficient");
        assert!(
            found[0].1.starts_with("cites Zhou 2019 too"),
            "the reason should name the paper: {:?}",
            found[0].1
        );
    }

    #[test]
    fn a_rare_tag_beats_a_common_one_in_the_panel() {
        // The ordering that makes the panel worth reading. Both notes share a
        // tag with the open one; only one of those tags says anything.
        let f = Fixture::new();
        // `#note` is on everything, `#sbsei` on two.
        for i in 0..12 {
            note(&f, "Research", &format!("Filler {i}"), &["note"], "Filler.");
        }
        let open = note(&f, "Research", "Open note", &["note", "sbsei"], "Open.");
        let rare = note(
            &f,
            "Research",
            "The rare neighbour",
            &["note", "sbsei"],
            "Other.",
        );
        let common = note(&f, "Research", "A common neighbour", &["note"], "Other.");
        let _ = (rare, common);
        f.index.rebuild(&f.vault).unwrap();

        let found = near(&f, &open);
        assert_eq!(found[0].0, "The rare neighbour");
        assert_eq!(found[0].1, "shares #sbsei");
        // And the notes sharing only `#note` are below the floor entirely.
        assert!(
            !found.iter().any(|(title, _)| title == "A common neighbour"),
            "a tag on every note is not a reason: {found:?}"
        );
    }

    #[test]
    fn a_backlink_is_not_repeated_as_a_related_note() {
        // It is already in the panel one section up. A short list spent on
        // rows the reader has just seen is a list that stops being read.
        let f = Fixture::new();
        // Filler, so `#sb2se3` is a rare tag rather than one the whole vault
        // carries — which would say nothing, as
        // `a_word_in_every_note_is_not_a_reason` pins.
        for i in 0..12 {
            note(&f, "Filler", &format!("Filler {i}"), &[], "Filler prose.");
        }
        let open = note(&f, "Research", "Open note", &["sb2se3"], "Open.");
        let linking = note(
            &f,
            "Research",
            "Links here",
            &["sb2se3"],
            &format!("As in [[{open}]]."),
        );
        note(&f, "Research", "Merely tagged", &["sb2se3"], "Other.");
        f.index.rebuild(&f.vault).unwrap();

        let found = near(&f, &open);
        let titles: Vec<&str> = found.iter().map(|(t, _)| t.as_str()).collect();
        assert!(titles.contains(&"Merely tagged"), "{found:?}");
        assert!(!titles.contains(&"Links here"), "{found:?}");
        // But it *is* a backlink, so it has not been lost.
        assert_eq!(f.index.backlinks(&open).unwrap()[0].id, linking);
    }

    #[test]
    fn two_notes_in_the_same_project_are_related_by_it() {
        // A project is a note, and belonging to one is linking to it. That
        // gives the panel "both in PhD Thesis" with no project field anywhere
        // in the data model.
        let f = Fixture::new();
        let project = f.vault.create_note("PhD Thesis", None).unwrap().summary.id;
        f.vault
            .set_type(&project, crate::frontmatter::NoteType::Project)
            .unwrap();

        let a = note(
            &f,
            "Research",
            "Chapter 2 work",
            &[],
            &format!("Part of [[{project}]]."),
        );
        note(
            &f,
            "Elsewhere",
            "Chapter 3 work",
            &[],
            &format!("Part of [[{project}]]."),
        );
        f.index.rebuild(&f.vault).unwrap();

        let found = near(&f, &a);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "Chapter 3 work");
        // Leading, because it is the strongest reason. A second clause about
        // shared words is legitimate and not what this test is about.
        assert!(
            found[0].1.starts_with("both in PhD Thesis"),
            "{:?}",
            found[0].1
        );
    }

    #[test]
    fn a_plain_shared_link_says_what_they_both_point_at() {
        let f = Fixture::new();
        let target = note(&f, "Research", "Phonon transport", &[], "Phonons.");
        let a = note(&f, "Research", "One", &[], &format!("See [[{target}]]."));
        note(&f, "Research", "Two", &[], &format!("Also [[{target}]]."));
        f.index.rebuild(&f.vault).unwrap();

        let found = near(&f, &a);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "Two");
        assert!(
            found[0].1.starts_with("both link to Phonon transport"),
            "{:?}",
            found[0].1
        );
    }

    #[test]
    fn shared_prose_finds_a_neighbour_nothing_else_connects() {
        // The signal that earns the panel its keep: two notes in different
        // folders, no shared tag, no shared source, no link between them —
        // found because they are about the same thing.
        let f = Fixture::new();
        for i in 0..10 {
            note(
                &f,
                "Filler",
                &format!("Filler {i}"),
                &[],
                "Ordinary prose about ordinary things.",
            );
        }
        let a = note(
            &f,
            "Research",
            "Ribbon anisotropy",
            &[],
            "Quasi-1D ribbons align along the crystallographic axis, so boundary scattering dominates.",
        );
        note(
            &f,
            "Elsewhere",
            "Scattering regimes",
            &[],
            "Boundary scattering dominates below the Debye temperature in quasi-1D ribbons.",
        );
        f.index.rebuild(&f.vault).unwrap();

        let found = near(&f, &a);
        assert_eq!(found[0].0, "Scattering regimes");
        assert!(found[0].1.contains("distinctive word"), "{found:?}");
    }

    #[test]
    fn a_word_in_every_note_is_not_a_reason() {
        // Without the document-frequency window, the panel would find a
        // neighbour for every note and every one of them would be the same
        // four notes with the longest bodies.
        let f = Fixture::new();
        for i in 0..12 {
            note(
                &f,
                "Research",
                &format!("Sample {i}"),
                &[],
                "Measurement recorded during the experiment.",
            );
        }
        let a = note(
            &f,
            "Research",
            "Open note",
            &[],
            "Measurement recorded during the experiment.",
        );
        f.index.rebuild(&f.vault).unwrap();
        assert!(
            near(&f, &a).is_empty(),
            "words shared by the whole vault say nothing: {:?}",
            near(&f, &a)
        );
    }

    #[test]
    fn a_boilerplate_preamble_does_not_make_every_note_a_neighbour() {
        // The document-frequency ceiling's real job. Lab notes carry a shared
        // preamble — the same instrument, the same standard, the same
        // corrections — and it is long enough that its words add up past the
        // floor if nothing stops them. Every note would then be "related" to
        // every other by a paragraph none of them is about.
        let f = Fixture::new();
        let preamble = "Instrument calibrated against the sapphire standard before each \
                        measurement, baseline correction applied, argon purge maintained \
                        throughout, ambient humidity logged separately";
        for i in 0..8 {
            note(&f, "Research", &format!("Run {i}"), &[], preamble);
        }
        for i in 0..16 {
            note(
                &f,
                "Admin",
                &format!("Other {i}"),
                &[],
                "Nothing in common.",
            );
        }
        let open = note(&f, "Research", "Today's run", &[], preamble);
        f.index.rebuild(&f.vault).unwrap();

        let found = near(&f, &open);
        assert!(
            found.is_empty(),
            "a shared preamble is not a shared subject: {found:?}"
        );
    }

    #[test]
    fn the_panel_reads_the_body_it_is_given_rather_than_the_saved_file() {
        // Typing about a subject should surface neighbours before autosave
        // has run. Asking a question about the open note must not depend on
        // whether it has been written to disk yet.
        let f = Fixture::new();
        for i in 0..10 {
            note(
                &f,
                "Filler",
                &format!("Filler {i}"),
                &[],
                "Ordinary prose here.",
            );
        }
        note(
            &f,
            "Elsewhere",
            "Neumann-Kopp rule",
            &[],
            "Additivity of heat capacities across constituent binaries.",
        );
        let a = note(&f, "Research", "Empty so far", &[], "");
        f.index.rebuild(&f.vault).unwrap();

        assert!(near(&f, &a).is_empty(), "nothing typed yet");
        let unsaved = "Additivity of heat capacities, per the constituent binaries.";
        let found = f.index.related(&a, unsaved, 5).unwrap();
        assert_eq!(found[0].title, "Neumann-Kopp rule");
    }

    #[test]
    fn asking_what_is_near_a_note_changes_nothing() {
        // The same rule as a saved view, for the same reason. A panel that
        // recorded what it had computed would make reading a note edit it.
        let f = Fixture::new();
        let a = note(&f, "Research", "Alpha", &["sb2se3"], "Selenide ribbons.");
        note(&f, "Research", "Beta", &["sb2se3"], "Selenide chains.");
        f.index.rebuild(&f.vault).unwrap();

        let before = snapshot(&f.root);
        for _ in 0..10 {
            f.index.related(&a, "Selenide ribbons.", 5).unwrap();
            f.index.folder_neighbours(&a, 5).unwrap();
        }
        assert_eq!(
            before,
            snapshot(&f.root),
            "the context panel rewrote a note"
        );
    }

    #[test]
    fn the_folder_list_is_siblings_and_never_the_note_itself() {
        let f = Fixture::new();
        let a = note(&f, "Research/Sb2Se3", "Open note", &[], "");
        note(&f, "Research/Sb2Se3", "A sibling", &[], "");
        note(&f, "Research", "A parent's note", &[], "");
        f.index.rebuild(&f.vault).unwrap();

        let titles: Vec<String> = f
            .index
            .folder_neighbours(&a, 10)
            .unwrap()
            .into_iter()
            .map(|n| n.title)
            .collect();
        assert_eq!(titles, ["A sibling"]);
    }

    #[test]
    fn a_note_the_index_has_never_seen_has_no_neighbours_rather_than_an_error() {
        let f = Fixture::new();
        assert!(
            f.index
                .related("01HQNOTAREALID", "prose", 5)
                .unwrap()
                .is_empty()
        );
        assert!(
            f.index
                .folder_neighbours("01HQNOTAREALID", 5)
                .unwrap()
                .is_empty()
        );
    }
    /// A vault shaped like real work, for judging the panel rather than
    /// asserting about it.
    ///
    /// Thirty-odd notes across four strands of a materials-chemistry project,
    /// with the overlaps a real vault has: the same paper cited from three
    /// places, a method tag spanning strands, prose that repeats vocabulary
    /// without repeating tags. The `#[ignore]`d test below prints what the
    /// panel would say for each, which is the only way to answer "is at least
    /// one of these genuinely useful" — that is a judgement, and a number
    /// asserting it would be a number pretending to be one.
    fn realistic_vault(f: &Fixture) {
        let paper = |title: &str, year: &str| {
            f.vault
                .create_source(
                    title,
                    crate::frontmatter::SourceMeta {
                        year: Some(year.into()),
                        ..Default::default()
                    },
                )
                .unwrap()
                .summary
                .id
        };
        let zhou = paper("Zhou 2019 — quasi-1D Sb2Se3 ribbons", "2019");
        let chen = paper("Chen 2021 — Seebeck in selenides", "2021");
        let liu = paper("Liu 2018 — DSC of antimony chalcogenides", "2018");

        let cite = |id: &str, source: &str, page: &str| {
            f.vault
                .set_citations(
                    id,
                    vec![crate::frontmatter::Citation {
                        id: source.to_string(),
                        page: Some(page.into()),
                        quote: None,
                        captured: None,
                    }],
                )
                .unwrap();
        };

        let thesis = f.vault.create_note("PhD Thesis", None).unwrap().summary.id;
        f.vault
            .set_type(&thesis, crate::frontmatter::NoteType::Project)
            .unwrap();

        // ---- thermodynamics ----
        let cp = note(
            f,
            "Research/Sb2Se3/Thermodynamics",
            "Sb2Se3 heat capacity",
            &["sb2se3", "method/dsc"],
            "Cp fitted as a + bT + cT^2 + dT^-2 over 300-800 K. The Neumann-Kopp \
             estimate from the constituent binaries sits 4% low at the top of the range.",
        );
        cite(&cp, &liu, "112");
        note(
            f,
            "Research/Thermodynamics",
            "Neumann-Kopp rule",
            &["method/dsc"],
            "Additivity of heat capacities across the constituent binaries. Reliable to \
             a few percent for chalcogenides; worse where a phase transition intervenes.",
        );
        let dsc = note(
            f,
            "Research/Sb2Se3/Thermodynamics",
            "DSC run 2026-08-27",
            &["sb2se3", "method/dsc"],
            "Ramped 300-800 K at 10 K/min under argon. Baseline drift corrected against \
             the sapphire standard. Cp agrees with the fitted polynomial.",
        );
        cite(&dsc, &liu, "108");

        // ---- transport ----
        let phonon = note(
            f,
            "Research/Sb2Se3/Transport",
            "Phonon transport in ribbons",
            &["sb2se3", "method/ppms"],
            &format!(
                "Quasi-1D ribbons align along the crystallographic axis, so boundary \
                 scattering dominates over Umklapp below the Debye temperature. Part of [[{thesis}]]."
            ),
        );
        cite(&phonon, &zhou, "6");
        let seebeck = note(
            f,
            "Research/Sb2Se3/Transport",
            "Seebeck coefficient",
            &["sb2se3", "method/ppms"],
            &format!(
                "Seebeck rises with the ribbon axis alignment. Boundary scattering again, \
                 read through the carrier concentration. Part of [[{thesis}]]."
            ),
        );
        cite(&seebeck, &chen, "3");
        note(
            f,
            "Research/Sb2Se3/Transport",
            "Carrier concentration",
            &["sb2se3", "method/hall"],
            "Hall measurements put the carrier concentration near 10^17 per cubic centimetre, \
             which is what the Seebeck reading assumes.",
        );

        // ---- a different material, same methods ----
        note(
            f,
            "Research/SbSeI",
            "SbSeI heat capacity",
            &["sbsei", "method/dsc"],
            "Cp measured the same way as the selenide. Neumann-Kopp from the constituent \
             binaries again, and again a few percent low.",
        );
        note(
            f,
            "Research/SbSeI",
            "SbSeI ribbons",
            &["sbsei", "method/xrd"],
            "Quasi-1D ribbons here too, along a different crystallographic axis. Boundary \
             scattering should follow.",
        );

        // ---- reading ----
        let reading = note(
            f,
            "Research/Reading",
            "Reading — Zhou 2019",
            &["sb2se3", "unread"],
            "Source says the ribbon axis sets the thermal conductivity. My question: does \
             that survive at the grain sizes we actually get?",
        );
        cite(&reading, &zhou, "6");

        // ---- filler, so the common words are common ----
        for i in 0..20 {
            note(
                f,
                "Admin",
                &format!("Group meeting {i}"),
                &["meeting"],
                "Discussed progress and next steps. Actions recorded for the week.",
            );
        }
        f.index.rebuild(&f.vault).unwrap();
    }

    /// Run with `cargo test judge -- --ignored --nocapture` to read the panel.
    ///
    /// Ignored because its output is for a person, not an assertion. The
    /// step's proof — "at least one genuinely useful neighbour in the top
    /// five" — is a judgement, and writing a number that stands in for one
    /// would be measuring something else and calling it the same thing.
    #[test]
    #[ignore = "prints the panel for a person to judge"]
    fn judge_the_panel_on_a_realistic_vault() {
        let f = Fixture::new();
        realistic_vault(&f);
        for summary in f.index.all_notes().unwrap() {
            if summary.folder.starts_with("Admin") || summary.folder == "Library" {
                continue;
            }
            let body = f.vault.read_note(&summary.id).unwrap().body;
            let found = f.index.related(&summary.id, &body, 5).unwrap();
            println!("\n{}  ({})", summary.title, summary.folder);
            if found.is_empty() {
                println!("    (nothing near it)");
            }
            for r in found {
                println!("    {:5.2}  {}  —  {}", r.score, r.title, r.reason);
            }
        }
    }

    #[test]
    fn a_realistic_vault_puts_something_useful_near_every_working_note() {
        // Not the judgement — that is the ignored test above — but the floor
        // under it: a panel that is empty on a vault this connected would be
        // broken, and one that fires on the meeting notes would be noise.
        let f = Fixture::new();
        realistic_vault(&f);
        for summary in f.index.all_notes().unwrap() {
            let body = f.vault.read_note(&summary.id).unwrap().body;
            let found = f.index.related(&summary.id, &body, 5).unwrap();
            if summary.folder == "Admin" {
                assert!(
                    found.is_empty(),
                    "twenty identical meeting notes should say nothing about each other, \
                     but {} got {found:?}",
                    summary.title
                );
            } else if summary.folder.starts_with("Research") {
                assert!(
                    !found.is_empty(),
                    "{} has no neighbours at all",
                    summary.title
                );
                for r in &found {
                    assert!(!r.reason.is_empty(), "{} gave no reason", r.title);
                    assert_ne!(r.id, summary.id, "a note is not near itself");
                }
            }
        }
    }
    // ---- duplicates and disagreements ----------------------------------------

    fn dupes(f: &Fixture, id: &str) -> Vec<(String, String)> {
        let doc = f.vault.read_note(id).unwrap();
        f.index
            .duplicates(
                id,
                &doc.summary.title,
                &doc.body,
                &f.vault.dismissed_duplicates(id).unwrap(),
                5,
            )
            .unwrap()
            .into_iter()
            .map(|d| (d.title, d.reason))
            .collect()
    }

    #[test]
    fn the_same_title_in_a_different_order_is_found() {
        // The step's first proof, end to end through FTS and the index.
        let f = Fixture::new();
        let a = note(
            &f,
            "Research",
            "Thermal conductivity Sb2Se3",
            &[],
            "Boundary scattering dominates below the Debye temperature.",
        );
        note(
            &f,
            "Research/Sb2Se3",
            "Sb2Se3 thermal conductivity",
            &[],
            "Boundary scattering dominates below the Debye temperature, so the ribbon axis matters.",
        );
        note(
            &f,
            "Admin",
            "Group meeting",
            &[],
            "Progress and next steps.",
        );
        f.index.rebuild(&f.vault).unwrap();

        let found = dupes(&f, &a);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, "Sb2Se3 thermal conductivity");
        assert!(found[0].1.contains("same title"), "{:?}", found[0].1);
    }

    #[test]
    fn a_dismissed_pair_is_never_offered_again_from_either_side() {
        let f = Fixture::new();
        let a = note(
            &f,
            "Research",
            "DSC run 27 August",
            &[],
            "Ramped under argon.",
        );
        let b = note(&f, "Research", "DSC run 27 Aug", &[], "Ramped under argon.");
        f.index.rebuild(&f.vault).unwrap();
        assert_eq!(dupes(&f, &a).len(), 1);

        f.vault.not_duplicates(&a, &b).unwrap();
        assert!(
            dupes(&f, &a).is_empty(),
            "dismissed from the side that asked"
        );
        assert!(dupes(&f, &b).is_empty(), "and from the other side too");
    }

    #[test]
    fn dismissing_a_pair_does_not_stamp_either_note_as_edited() {
        // Saying "these are two different notes" is a statement about a
        // suggestion, not a change to either note. A vault whose timestamps
        // move when someone dismisses a prompt has lost real information about
        // when the work happened.
        //
        // The timestamps are written into the past deliberately. Sutra records
        // whole seconds, so a note created and dismissed in the same second
        // has the same `updated` either way — a version of this test that made
        // its own notes passed even when dismissal did stamp them, which is to
        // say it tested nothing.
        let f = Fixture::new();
        let write = |id: &str, title: &str| {
            std::fs::write(
                f.root.join(format!("{title}.md")),
                format!(
                    "---\nid: {id}\ntitle: {title}\nposition: 0\n\
                     created: 2026-01-02T03:04:05Z\nupdated: 2026-01-02T03:04:05Z\n---\n\nProse.\n"
                ),
            )
            .unwrap();
        };
        let a = "01HQ3M8K2P0000000000000NA1";
        let b = "01HQ3M8K2P0000000000000NB2";
        write(a, "Alpha");
        write(b, "Beta");
        let stamped = time::macros::datetime!(2026-01-02 03:04:05 UTC);
        assert_eq!(f.vault.read_note(a).unwrap().summary.updated, stamped);

        f.vault.not_duplicates(a, b).unwrap();
        assert_eq!(f.vault.read_note(a).unwrap().summary.updated, stamped);
        assert_eq!(f.vault.read_note(b).unwrap().summary.updated, stamped);

        // And the fact is in the markdown, not only in the index.
        assert_eq!(f.vault.dismissed_duplicates(a).unwrap(), [b.to_string()]);
        assert_eq!(f.vault.dismissed_duplicates(b).unwrap(), [a.to_string()]);
    }

    #[test]
    fn dismissing_the_same_pair_twice_records_it_once() {
        let f = Fixture::new();
        let a = note(&f, "Research", "Alpha", &[], "One.");
        let b = note(&f, "Research", "Beta", &[], "Two.");
        f.vault.not_duplicates(&a, &b).unwrap();
        f.vault.not_duplicates(&b, &a).unwrap();
        assert_eq!(f.vault.dismissed_duplicates(&a).unwrap().len(), 1);
    }

    #[test]
    fn merging_keeps_everything_and_leaves_no_dead_link() {
        // The whole safety argument for merge, in one test. Nothing thrown
        // away, no note left pointing at something that is gone, and the
        // absorbed note recoverable from the trash.
        let f = Fixture::new();
        let source = f
            .vault
            .create_source("Zhou 2019", crate::frontmatter::SourceMeta::default())
            .unwrap()
            .summary
            .id;

        let keep = note(
            &f,
            "Research",
            "Sb2Se3 Cp",
            &["sb2se3"],
            "The fitted polynomial.",
        );
        let absorb = note(
            &f,
            "Research",
            "Cp Sb2Se3",
            &["method/dsc"],
            "The measured points.",
        );
        f.vault
            .set_citations(
                &absorb,
                vec![crate::frontmatter::Citation {
                    id: source.clone(),
                    page: Some("112".into()),
                    quote: None,
                    captured: None,
                }],
            )
            .unwrap();
        let pointer = note(
            &f,
            "Research",
            "Points at the absorbed one",
            &[],
            &format!("See [[{absorb}]] for the raw data."),
        );
        f.index.rebuild(&f.vault).unwrap();

        let merged = f.vault.merge_notes(&keep, &absorb).unwrap();

        // Both bodies survive, and which was which is still legible.
        let body = f.vault.read_note(&keep).unwrap().body;
        assert!(body.contains("The fitted polynomial."), "{body}");
        assert!(body.contains("The measured points."), "{body}");
        assert!(body.contains("## Merged from Cp Sb2Se3"), "{body}");

        // Tags and provenance are unioned, not replaced.
        assert!(merged.tags.contains(&"sb2se3".to_string()));
        assert!(merged.tags.contains(&"method/dsc".to_string()));
        assert_eq!(merged.sources.len(), 1);
        assert_eq!(merged.sources[0].page.as_deref(), Some("112"));

        // Nothing is left pointing at a note that no longer exists.
        let pointing = f.vault.read_note(&pointer).unwrap().body;
        assert!(pointing.contains(&format!("[[{keep}]]")), "{pointing}");
        assert!(!pointing.contains(&absorb), "{pointing}");

        // And the absorbed note is recoverable rather than gone.
        assert!(f.vault.read_note(&absorb).is_err());
        let trash: Vec<_> = std::fs::read_dir(f.root.join(".sutra").join("trash"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(trash.len(), 1, "the absorbed note should be in the trash");
    }

    #[test]
    fn merging_an_empty_note_adds_no_empty_heading() {
        let f = Fixture::new();
        let keep = note(&f, "Research", "Kept", &[], "The only prose.");
        let absorb = note(&f, "Research", "Kept as well", &[], "");
        f.index.rebuild(&f.vault).unwrap();

        f.vault.merge_notes(&keep, &absorb).unwrap();
        let body = f.vault.read_note(&keep).unwrap().body;
        assert!(!body.contains("Merged from"), "{body}");
        assert_eq!(body.trim(), "The only prose.");
    }

    #[test]
    fn a_note_cannot_be_merged_into_itself() {
        let f = Fixture::new();
        let a = note(&f, "Research", "Alone", &[], "Prose.");
        assert!(f.vault.merge_notes(&a, &a).is_err());
        assert_eq!(f.vault.read_note(&a).unwrap().body.trim(), "Prose.");
    }

    #[test]
    fn the_vault_wide_pass_reports_each_pair_once() {
        let f = Fixture::new();
        note(
            &f,
            "Research",
            "Thermal conductivity Sb2Se3",
            &[],
            "Boundary scattering.",
        );
        note(
            &f,
            "Research",
            "Sb2Se3 thermal conductivity",
            &[],
            "Boundary scattering.",
        );
        note(
            &f,
            "Research",
            "Seebeck coefficient",
            &[],
            "Carrier concentration.",
        );
        f.index.rebuild(&f.vault).unwrap();

        let pairs = f.index.duplicate_pairs(20).unwrap();
        assert_eq!(pairs.len(), 1, "{pairs:?}");
        let titles = [pairs[0].left_title.as_str(), pairs[0].right_title.as_str()];
        assert!(
            titles.contains(&"Thermal conductivity Sb2Se3"),
            "{titles:?}"
        );
        assert!(
            titles.contains(&"Sb2Se3 thermal conductivity"),
            "{titles:?}"
        );
    }

    // ---- numeric claims that differ ------------------------------------------

    fn differs(f: &Fixture, id: &str) -> Vec<(String, String, String, f64)> {
        let body = f.vault.read_note(id).unwrap().body;
        f.index
            .disagreements(id, &body, 5)
            .unwrap()
            .into_iter()
            .map(|d| (d.label, d.here, d.there, d.factor.round()))
            .collect()
    }

    #[test]
    fn two_values_of_the_same_quantity_a_factor_apart_are_flagged() {
        // The step's second proof. Neither is declared correct — the result
        // carries both claims and the ratio, and says nothing else.
        let f = Fixture::new();
        let here = note(
            &f,
            "Research/Sb2Se3",
            "Sb2Se3 Cp",
            &["sb2se3"],
            "κ = 0.037 W m⁻¹ K⁻¹ at 300 K, from the fitted polynomial.",
        );
        note(
            &f,
            "Research/Sb2Se3",
            "DSC run 2026-08-27",
            &["sb2se3"],
            "Ramped 300-800 K at 10 K/min. κ = 0.37 W/mK from the same sample.",
        );
        f.index.rebuild(&f.vault).unwrap();

        let found = differs(&f, &here);
        assert_eq!(found.len(), 1, "{found:?}");
        let (label, here_text, there_text, factor) = &found[0];
        assert_eq!(label, "κ");
        assert_eq!(factor, &10.0);
        // Both claims come back as written. Nothing says which is right,
        // because nothing here knows.
        assert!(here_text.contains("0.037"), "{here_text}");
        assert!(there_text.contains("0.37"), "{there_text}");
    }

    #[test]
    fn unconnected_notes_are_not_compared() {
        // Without this, every measurement in the vault is flagged against
        // every other and the panel is noise from the first week.
        let f = Fixture::new();
        let here = note(&f, "Research", "Mine", &["sb2se3"], "κ = 0.037 W/mK");
        note(
            &f,
            "Elsewhere",
            "Nothing in common",
            &["unrelated"],
            "κ = 0.37 W/mK",
        );
        f.index.rebuild(&f.vault).unwrap();
        assert!(differs(&f, &here).is_empty());
    }

    #[test]
    fn a_shared_source_or_link_is_enough_of_a_connection() {
        let f = Fixture::new();
        let target = note(&f, "Research", "Phonons", &[], "Prose.");
        let here = note(
            &f,
            "Research",
            "Mine",
            &[],
            &format!("κ = 0.037 W/mK. See [[{target}]]."),
        );
        note(
            &f,
            "Elsewhere",
            "Linked, not tagged",
            &[],
            &format!("κ = 0.37 W/mK. Also [[{target}]]."),
        );
        f.index.rebuild(&f.vault).unwrap();
        assert_eq!(differs(&f, &here).len(), 1);
    }

    #[test]
    fn a_note_with_no_claims_is_not_compared_at_all() {
        let f = Fixture::new();
        let here = note(
            &f,
            "Research",
            "Prose only",
            &["sb2se3"],
            "No numbers here.",
        );
        note(&f, "Research", "Has one", &["sb2se3"], "κ = 0.37 W/mK");
        f.index.rebuild(&f.vault).unwrap();
        assert!(differs(&f, &here).is_empty());
    }

    #[test]
    fn the_widest_disagreement_comes_first() {
        // A value out by a thousand is a unit error and is worth seeing before
        // one out by three.
        let f = Fixture::new();
        let here = note(&f, "Research", "Mine", &["sb2se3"], "κ = 0.037 W/mK");
        note(&f, "Research", "Out by three", &["sb2se3"], "κ = 0.11 W/mK");
        note(
            &f,
            "Research",
            "Out by a thousand",
            &["sb2se3"],
            "κ = 37 W/mK",
        );
        f.index.rebuild(&f.vault).unwrap();

        let found = differs(&f, &here);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].3, 1000.0);
        assert_eq!(found[1].3, 3.0);
    }
    // ---- optional AI ---------------------------------------------------------

    /// An assistant that answers everything, so rejecting it proves something.
    use crate::ai::Assistant as _;

    struct Chatty;

    impl crate::ai::Assistant for Chatty {
        fn label(&self) -> String {
            "chatty".into()
        }
        fn respond(&self, ask: &crate::ai::Ask) -> Result<crate::ai::Draft> {
            Ok(crate::ai::finish(
                ask,
                "A confident summary, with a fabricated citation [@01HQFAKEFAKEFAKEFAKEFAKEFA] \
                 and some tags.\nsb2se3\nmethod/dsc",
                "chatty".into(),
            ))
        }
    }

    #[test]
    fn with_ai_answering_and_every_draft_rejected_the_vault_is_byte_identical() {
        // The step's proof. Rejecting is not an operation at all — there is
        // nothing to undo, because asking never wrote anything. That is the
        // architecture rather than a careful implementation: `ai` does not
        // import `vault`, a `Draft` carries no path and no id, and the only
        // module that writes is not on this path.
        let f = Fixture::new();
        let source = f
            .vault
            .create_source("Zhou 2019", crate::frontmatter::SourceMeta::default())
            .unwrap()
            .summary
            .id;
        let a = note(
            &f,
            "Research",
            "Sb2Se3 Cp",
            &["sb2se3"],
            "Cp fitted over 300-800 K.",
        );
        note(
            &f,
            "Research",
            "DSC run",
            &["method/dsc"],
            "Ramped under argon.",
        );
        f.index.rebuild(&f.vault).unwrap();

        let before = snapshot(&f.root);
        let assistant = Chatty;

        for _ in 0..5 {
            for task in [
                crate::ai::Task::Summarise,
                crate::ai::Task::Tags,
                crate::ai::Task::Questions,
            ] {
                let doc = f.vault.read_note(&a).unwrap();
                let ask = crate::ai::Ask {
                    task,
                    title: doc.summary.title.clone(),
                    body: doc.body.clone(),
                    vault_tags: f.vault.list_tags().unwrap().into_keys().collect(),
                    known_sources: vec![source.clone()],
                };
                let draft = assistant.respond(&ask).unwrap();
                // Rejecting: the value goes out of scope. There is no call to
                // make, which is the whole point.
                assert!(!draft.model.is_empty());
                drop(draft);
            }
        }

        assert_eq!(
            before,
            snapshot(&f.root),
            "asking the assistant changed a note"
        );
    }

    #[test]
    fn accepting_a_draft_goes_through_the_same_write_as_typing_it() {
        // The other half of the argument. Accepted text is not written by the
        // assistant — it is handed to `save_note`, exactly as if the person
        // had typed it, so there is one write path in the app and generated
        // text has no privileges on it.
        let f = Fixture::new();
        let a = note(
            &f,
            "Research",
            "Sb2Se3 Cp",
            &[],
            "Cp fitted over 300-800 K.",
        );
        let ask = crate::ai::Ask {
            task: crate::ai::Task::Summarise,
            title: "Sb2Se3 Cp".into(),
            body: "Cp fitted over 300-800 K.".into(),
            vault_tags: Vec::new(),
            known_sources: Vec::new(),
        };
        let draft = Chatty.respond(&ask).unwrap();

        let body = format!(
            "{}\n\n{}",
            f.vault.read_note(&a).unwrap().body.trim(),
            draft.text
        );
        f.vault.save_note(&a, "Sb2Se3 Cp", &body).unwrap();

        let saved = f.vault.read_note(&a).unwrap().body;
        assert!(saved.contains("A confident summary"), "{saved}");
        // And the fabricated citation never reached the file, because it was
        // removed before the draft was ever shown.
        assert!(!saved.contains("01HQFAKE"), "{saved}");
    }
}
