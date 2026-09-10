# What survives what

Frozen at v0.3.

A vault is somebody's research, and the machines it lives on are ordinary: a
laptop lid closes mid-write, OneDrive rewrites a file underneath a save, an
external disk is unplugged, a file is edited in Notepad and saved with a colon in
the wrong place. None of those are exceptional, so none of them may be a way to
lose work.

The rule the whole design serves: **the markdown files are the research, and
nothing else is.** Every other artefact — the search index, the id-to-path map,
the excerpt in a note list — is derived and can be thrown away. That is what
makes most of the table below short: the answer is usually "rebuild it".

## The matrix

| What happened                                           | What survives                                                    | Held by                                                       |
| ------------------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------- |
| The search index is deleted                             | Everything. One scan rebuilds it.                                | `deleting_the_database_loses_nothing`                         |
| The search index is deleted, and it held dismissals     | Everything, including "these two are not duplicates"             | `deleting_the_database_loses_no_research_content`             |
| The index file is garbage (not a database at all)       | Everything. It is discarded and rebuilt.                         | `a_corrupt_database_is_discarded_and_rebuilt`                 |
| The index file is garbage _and_ another window holds it | Everything, but that window has to be closed first               | see "What is not recoverable" below                           |
| The index file is truncated to a valid-looking header   | Everything. v0.3 detects this; v0.2 did not.                     | `a_truncated_database_is_discarded_and_rebuilt`               |
| The index schema changes between versions               | Everything. A version mismatch drops and rebuilds.               | `a_schema_reset_can_run_again_on_the_same_file`               |
| A save is interrupted part-way                          | The previous version of the note, whole                          | `a_note_left_half_written_does_not_replace_the_good_one`      |
| A sync client rewrites files during a save              | One version or the other, never half of each                     | `a_sync_client_rewriting_files_never_tears_a_note`            |
| A note is hand-edited into invalid YAML                 | Every other note lists and opens; that one is reported           | `a_corrupt_note_does_not_break_the_listing`                   |
| A move is interrupted after the file moved              | The note, and every figure in it still loads                     | `an_interrupted_move_never_leaves_a_figure_that_cannot_load`  |
| A move completes                                        | One copy of each picture, beside the note                        | `a_completed_move_leaves_no_stray_copy_behind`                |
| A migration is interrupted half way                     | Every note, in the folder the migration was moving it to         | `a_migration_interrupted_half_way_finishes_where_it_left_off` |
| A migration is run twice                                | Nothing moves the second time                                    | `migrating_a_second_time_moves_nothing`                       |
| A vault contains notes that never claimed a parent      | Their folders. The migration does not touch them.                | `a_note_with_no_claim_is_left_in_the_folder_it_is_in`         |
| Two files end up claiming one note id                   | Both files. One opens; the other is named, not silently shadowed | `two_files_claiming_one_id_are_reported_not_just_resolved`    |
| An attachment is deleted outside Sutra                  | The note, and an error rather than a crash                       | `a_missing_attachment_is_an_error_not_a_panic`                |
| A note is deleted                                       | The file, in `.sutra/trash/`, with its own attachments           | `deleting_a_note_takes_its_own_attachment_to_the_trash`       |
| A credential store refuses a key                        | The key, in the settings file, with the user told                | `a_warning_never_repeats_the_key`                             |

## The two orderings that do the work

Almost everything above comes down to two decisions about the order of writes.

**Write to a temporary file, then rename.** A rename is atomic on both platforms
Sutra targets, so a reader sees either the old file or the new one. Every note
write goes through `note::write_atomic`, and so does the settings file — a
half-written `sutra.json` that parses as "no vault, no key, no fonts" would be a
worse failure than a note.

**Copy, commit the reference, then delete.** Moving a note brings its own
attachments with it, which is three writes with two gaps in between. Copying
first means both paths hold the file while the note still names the old one;
writing the note next means the new name is committed before the old file goes;
deleting last means an interruption leaves a duplicate file, which is visible and
harmless, rather than a figure that cannot load. `Relocated` in `vault.rs` exists
only to hold this order in place.

## What is not recoverable, and is said so

An **id clash** — two files whose frontmatter claims the same ULID — cannot be
resolved by Sutra, because only the author knows which is which. One file opens
by id and the other is unreachable by id, so v0.3 reports the pair rather than
resolving it quietly. Both files are still on disk and still readable; what is
lost is the ability to reach one of them by link until a person renames an id.

A **corrupt index file that another process still holds open** cannot be
recovered in place. Sutra discards a database it cannot read and rebuilds, and
the discard is retried — on Windows an open handle makes a file impossible to
unlink, and the handle is usually one that is in the act of closing. But if it
never closes there is nothing to be done: the bytes are not a database, so they
cannot be wiped with SQL either. The realistic cause is a second Sutra window on
the same vault, and the remedy is to close it. This is the one index state that
needs a person.

A note **edited outside Sutra between the scan and a save** is written over.
Sutra re-reads before writing wherever it can, but the window is not zero, and
closing it would need a lock on a folder people deliberately sync.
