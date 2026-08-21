# Phase 3 — storage design

Decisions taken before implementation. Recorded here so the reasoning survives
the conversation that produced it.

## File naming

Notes are `<title-slug>_<ULID>.md`, flat in the vault root.

```
Sb2Se3-growth-log_01HQ3M8K2P.md
CVT-runs_01HQ3M8K1A.md
```

The slug is for the human browsing the folder; the ULID is the identity.
Renaming a note rewrites the slug and keeps the ULID, and because links are
`[[id]]` the rename cannot break anything.

The ULID suffix also sidesteps the Windows reserved-filename problem for free:
a note called "CON" or "NUL" becomes `CON_01HQ....md`, which is a legal name.
Slugging still strips the characters Windows rejects outright — `< > : " / \ | ? *`
and control characters — collapses whitespace to hyphens, and truncates to keep
paths short.

## Vault layout

```
<vault>/
  <title-slug>_<ULID>.md      notes, flat, no nesting
  attachments/                every attachment, ULID-prefixed
  trash/                      deleted notes, moved not unlinked
```

Notes stay flat, as the spec requires — hierarchy is frontmatter, not
directories. `attachments/` and `trash/` are sidecars, not note containers, so
the flat rule still holds.

## What lives where

| | |
| --- | --- |
| Page-level: `icon`, `cover`, `tags`, `title`, `parent`, `position` | YAML frontmatter |
| Block-level: callouts, maths, chemistry | in-body markdown syntax |

There is no third option for block-level things. A callout or an equation
occupies a position in the document, and frontmatter has no way to express
position. They need in-body syntax, which is what Phase 5 defines for maths.

## Writes are atomic

Never write in place. Write the full contents to a temporary file in the same
directory, fsync it, then rename over the target.

Rename within a directory is atomic on both NTFS and APFS, so a reader either
sees the whole old file or the whole new one — never a half-written note. The
same-directory part matters: a rename across volumes is a copy, and copies are
not atomic.

A crash mid-save leaves a stray temp file and the previous note intact. That is
the correct failure: the worst case loses the last edit, never the note.

## External edits

The vault is a folder of plain files, so something else will eventually change
one — a sync client, a text editor, a script.

| In-app buffer | On-disk change | Behaviour |
| --- | --- | --- |
| Clean | Any | Reload silently |
| Dirty | Any | Prompt; the user picks |

Silently reloading over unsaved work would lose it, and prompting on every
external change when nothing is at stake would be noise. The dirty flag is the
only thing that distinguishes them.

## Deletion

Deleted notes move to `trash/`, keeping their filename. Nothing is unlinked.

Emptying the trash is a separate, explicit act. A notes app that permanently
destroys a research log on a stray keystroke is not trustworthy, and the
filesystem gives us the undo for free.

## Attachments

One `attachments/` directory, filenames prefixed with a fresh ULID:

```
attachments/01HQ3M8K4R_ribbon-sem.png
```

The prefix guarantees uniqueness without a lookup, keeps insertion order
sortable, and preserves enough of the original name to be recognisable. Notes
reference attachments by relative path, so the vault stays portable — moving
the folder does not break anything.
