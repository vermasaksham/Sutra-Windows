# What is in SQLite, and why none of it matters

Frozen at v0.3.

The rule: **nothing is written to the database that does not already exist in a
note on disk.** Delete the file and the worst case is one scan. This document
is the audit that makes that claim checkable rather than asserted — every
column, classified.

The index lives in the OS application-data directory, named by a hash of the
vault path, **not** in the vault. An index is machine-local; a vault is a
folder people sync between machines, and a half-synced SQLite file is worse
than no index at all.

## `notes`

| Column          | Class   | Rebuilt from                       |
| --------------- | ------- | ---------------------------------- |
| `id`            | derived | frontmatter `id`                   |
| `note_type`     | derived | frontmatter `type`                 |
| `title`         | derived | frontmatter `title`                |
| `folder`        | derived | the file's directory               |
| `position`      | derived | frontmatter `position`             |
| `tags`          | derived | frontmatter `tags`                 |
| `icon`, `cover` | derived | frontmatter                        |
| `excerpt`       | derived | the opening prose of the body      |
| `body`          | derived | the note's markdown, verbatim      |
| `source`        | derived | frontmatter `source:` (JSON copy)  |
| `sources`       | derived | frontmatter `sources:` (JSON copy) |
| `updated`       | derived | frontmatter `updated`              |

## `note_tags`, `note_sources`, `links`

| Table          | Class   | Rebuilt from                                                                                  |
| -------------- | ------- | --------------------------------------------------------------------------------------------- |
| `note_tags`    | derived | `tags` in frontmatter, flattened for lookup                                                   |
| `note_sources` | derived | `sources:` in frontmatter, flattened so "what cites this paper" is a query rather than a scan |
| `links`        | derived | `[[id]]` scanned out of note bodies                                                           |

## `notes_fts`, `notes_vocab`

|               | Class   | Rebuilt from                                                                              |
| ------------- | ------- | ----------------------------------------------------------------------------------------- |
| `notes_fts`   | derived | an external-content FTS5 index over `notes`, kept in step by three triggers on that table |
| `notes_vocab` | derived | an fts5vocab view over the above                                                          |

`notes_fts` holds no text of its own. `content = 'notes'` means FTS5 stores only
the inverted index and reads column values back from `notes` when it needs them —
for `snippet()`, for instance. That is why `notes.body` exists: it is where the
text actually lives, and it is the column FTS5 reads.

This replaced a self-contained FTS table carrying `id UNINDEXED`, and the reason
is measured rather than aesthetic. An `UNINDEXED` column has no index, so
`WHERE id = ?` against it scanned every note in the vault. Two of the app's most
ordinary actions were paying for that scan:

| Operation                 | Was, at 10,000 notes | Now     |
| ------------------------- | -------------------- | ------- |
| reindexing one saved note | 9.6 ms               | 1.6 ms  |
| one note's backlinks      | 23.2 ms              | 0.11 ms |

Backlinks were the worse of the two: the preview beside each backlink came from a
subquery against the FTS table, so a note with twenty-five backlinks scanned the
vault twenty-five times. Both are now rowid lookups — `notes_fts.rowid` _is_
`notes.rowid` — and both are flat in vault size. See
[performance.md](performance.md) for the measurements at 1k, 10k and 50k.

Three triggers on `notes` maintain the index: insert, delete and update. FTS5's
`'delete'` command has to be given the values the row held, which is what
`old.*` supplies, and is why the delete and update triggers spell every column
out. Nothing else may write to `notes_fts`, and an ordinary `DELETE FROM
notes_fts` is not even legal against an external-content table — which is the
useful part: the index cannot drift, because there is no second way to change it.

## Durable knowledge, and where it lives instead

Three facts are the ones somebody would expect to find in a database. All three
are deliberately in the markdown, and the reasoning is the same each time: the
index is disposable, and deleting it must not resurrect or destroy a decision
the researcher made.

| Fact                                 | Lives in                                          | Why not the index                                                                                                     |
| ------------------------------------ | ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| "these two notes are not duplicates" | `not_duplicates:` in both notes' frontmatter      | Deleting the index would resurrect a suggestion already dismissed.                                                    |
| a saved view                         | a note of `type: view`, with the query in `view:` | A view is backed up, synced, versioned and readable as text like everything else. Deleting the index cannot lose one. |
| evidence: page, quote, kind, `eid`   | `sources:` in frontmatter                         | It is the research.                                                                                                   |

## Schema changes

`SCHEMA_VERSION` mismatch **drops and rebuilds** rather than migrating.
Migrations are for data you cannot recreate, and this is not that. Adding a
column to this index is therefore never a vault migration.

## Detecting an index that is not usable

`Index::open` asks two questions before trusting a database, and v0.3 added the
second because the first was not enough.

`PRAGMA user_version` reads bytes 60-63 of the file _header_. A header survives
truncation — so a database cut short by an interrupted write, a full disk or a
killed process reported the correct schema version, was judged usable, was not
discarded, and then failed on the first real query. The index is meant to be
disposable; that made a truncated one fatal to opening the vault.

The second question is `SELECT 1 FROM notes LIMIT 1`, which touches the table's
b-tree root rather than the header. Cheap — it stops at the first row, so it
costs the same on fifty notes and fifty thousand — and decisive. Either
question failing discards the file and rebuilds from the markdown.

A full `PRAGMA integrity_check` was considered and rejected: it is expensive on
every launch, and anything it would catch that the probe does not is caught by
the query that hits it, with the same response either way.

## The test

`deleting_the_database_loses_no_research_content` builds a vault with a source,
a cached citation style, an evidence record carrying page, quote and kind, and
a note citing it; deletes the SQLite file; rebuilds; and asserts every one of
those comes back, including the reverse "what cites this source" relation.

If a future column cannot be listed above as derived, that is an architecture
bug, not a schema decision.
