# Sutra's architectural invariants

Frozen at v0.3. Each of these is a boundary that took a bug to find or a
decision to settle, and each is held by a test named in the right-hand column.
Changing one is a breaking change to the vault format, not a refactor.

The point of writing them down is that they are all _separations_. Almost every
mistake this codebase has made was two concepts collapsing into one — location
into identity, formatting into fact, the index into the record.

| Invariant                               | Why it exists                                                                                                                                                                                           | Held by                                                      |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| **Stable ID ≠ filename, path or title** | A note is renamed and moved constantly; a link that named any of those would break every time. The ULID lives in frontmatter, so the file can be called anything and live anywhere.                     | `moving_a_note_preserves_every_relationship`                 |
| **Folder = filesystem location**        | v0.1 stored hierarchy in a `parent:` key. Two sources of truth for where a note lives, and they drifted. A folder is now where the file is — the two cannot disagree because there is only one of them. | `folders_are_listed_from_the_filesystem`                     |
| **Markdown = durable knowledge**        | The vault must outlive this program. Everything a researcher wrote is in a `.md` file they can read in a text editor in ten years.                                                                      | `deleting_the_database_loses_no_research_content`            |
| **SQLite = rebuildable index**          | Nothing is written to the database that does not already exist in a note. Delete it and the worst case is one scan. This is what makes the database disposable rather than precious.                    | `deleting_the_database_loses_nothing`                        |
| **Zotero = bibliographic authority**    | Sutra does not parse author names, invent citation keys, or reimplement CSL. It asks. What it caches, it caches as a _copy of an answer_, never as its own opinion.                                     | `re_importing_keeps_the_cached_citation_styles`              |
| **Source ≠ Evidence**                   | A source is a paper. Evidence is one reading of it — a page, a quote, a kind. One paper yields many pieces of evidence, and they are not interchangeable.                                               | `every_recorded_piece_of_evidence_gets_its_own_id`           |
| **Evidence ≠ Interpretation**           | What the paper says and what the researcher concluded are different claims with different owners. Evidence is structured in frontmatter; interpretation is prose the author wrote. Nothing merges them. | `SourcesPanel` keeps `quote` in its own field                |
| **Citation ≠ evidence ownership**       | A `[@ref]` in prose is a _use_ of evidence, not the evidence itself. Deleting the sentence does not delete the page number and the transcribed quote.                                                   | `divergence` reports, never deletes                          |
| **Attachment ownership is explicit**    | An attachment referenced by exactly one note belongs to it and travels with it. One referenced by two belongs to neither and never moves. Nothing is deleted for looking unused.                        | `an_attachment_two_notes_use_is_left_where_it_is`            |
| **Views query, never store**            | A view is a saved question. The moment it can hold a value that is not in some note, it has become a database and the vault is no longer the truth.                                                     | `views.rs` module doc                                        |
| **AI is never a source of fact**        | Generated text arrives through the editor buffer like typing, and abstracts are the publisher's words or absent. Nothing generated is ever written where a reader would take it for something recorded. | `abstract_text` is never a summary                           |
| **No silent provenance fabrication**    | A citation key that does not exist reads correctly in a draft and fails at the bibliography. Absent is reported as absent.                                                                              | `citation_key: None` rather than a guess                     |
| **No silent destructive migration**     | Every migration detects, plans, previews, backs up, applies and verifies. A vault is somebody's research.                                                                                               | `migrating_citations_keeps_a_copy_of_every_note_first`       |
| **A migration is idempotent**           | An interrupted run is indistinguishable from a finished one asked to run again, so running it twice must change nothing. v0.3 found the reverse: a resumed run would have flattened the vault.          | `migrating_a_second_time_moves_nothing`                      |
| **No gap in a move breaks a figure**    | Moving a note is three writes. Copy the attachment, commit the reference, then delete — so an interruption leaves a duplicate file, never a picture that cannot load.                                   | `an_interrupted_move_never_leaves_a_figure_that_cannot_load` |
| **Per-note cost ≠ vault size**          | Opening, saving, reindexing and back-linking one note must cost the same in a vault of fifty thousand as of one thousand, or the app gets worse the longer it is used.                                  | `assert_flat` in `benchmarks.rs`                             |
| **One way to write the index**          | `notes_fts` is an external-content index maintained by triggers on `notes`. There is no second way to change it, so it cannot drift — and an ordinary `DELETE` against it is not even legal.            | `an_edit_removes_the_old_text_from_search`                   |
| **A credential is never in the vault**  | A vault is synced, zipped, copied and backed up. Keys live in the platform credential store or the app config directory, and nothing Sutra prints repeats one.                                          | `no_line_this_program_prints_interpolates_a_credential`      |

## The one that is easiest to get wrong

**Citation ≠ evidence ownership.** It looks like tidiness to delete a
provenance record when the last `[@ref]` to it disappears from the prose. It is
not. The record holds a page number and a quote transcribed by hand from a
paper; the sentence citing it is a draft. Half-written paragraphs are the
normal state of research writing, and they are indistinguishable from mistakes.
Sutra reports the disagreement in both directions and changes nothing.

## Where the rest of the reasoning lives

| Document                               | What it settles                                                        |
| -------------------------------------- | ---------------------------------------------------------------------- |
| [vault-contract.md](vault-contract.md) | What is in a note file, and what is deliberately not                   |
| [attachments.md](attachments.md)       | Who owns a picture, and when it moves                                  |
| [zotero-pdfs.md](zotero-pdfs.md)       | Why Sutra never copies a PDF out of a Zotero library                   |
| [index-audit.md](index-audit.md)       | Every column in SQLite, classified, and why none of it is precious     |
| [migrations.md](migrations.md)         | The six-step contract, and the registry of the two that exist          |
| [recovery.md](recovery.md)             | What survives what, with the test that holds each answer               |
| [performance.md](performance.md)       | What costs what at 1k, 10k and 50k notes, and which shapes are allowed |
| [credentials.md](credentials.md)       | Where secrets live, and the three claims about them                    |
