> **Superseded in part, 30 August 2026.**
>
> Decisions 1 and 6 below have changed. Notes no longer live in one flat
> directory under `title_ULID.md` names: they live in real nested folders, at
> most four deep, under clean human filenames with no id in them. The id moved
> entirely into frontmatter, which is what lets a note be moved without any
> link changing — see `moving_a_note_preserves_every_relationship` in
> `vault.rs`. The trash moved to `.sutra/trash/`, and attachments to a hidden
> `.attachments/` beside the notes that use them.
>
> Everything else here still holds: markdown is the source of truth, the index
> is disposable, conflicts prompt if dirty and reload if clean, and writes are
> atomic.

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

|                                                                    |                         |
| ------------------------------------------------------------------ | ----------------------- |
| Page-level: `icon`, `cover`, `tags`, `title`, `parent`, `position` | YAML frontmatter        |
| Block-level: callouts, maths, chemistry                            | in-body markdown syntax |

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

| In-app buffer | On-disk change | Behaviour              |
| ------------- | -------------- | ---------------------- |
| Clean         | Any            | Reload silently        |
| Dirty         | Any            | Prompt; the user picks |

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

## Who converts markdown

Rust reads and writes the file and splits off the frontmatter. It does not
interpret the body — to the storage layer the body is opaque text.

Conversion between markdown and editor state happens in the frontend, through
`@tiptap/markdown`. This is a deliberate departure from the original plan, which
assigned markdown parsing and serialisation to Rust.

The reason is Phase 5. Maths and chemical equations have to survive the round
trip losslessly. With conversion in the editor, a maths node declares its own
`parseMarkdown` and `renderMarkdown` alongside its schema, so the syntax for
`$$…$$` and `\ce{…}` exists in exactly one place. Converting through Rust would
require an intermediate representation that both sides agree on precisely, and
every node type would need implementing twice — with the fidelity risk landing
exactly where the requirement is strictest.

Measured before adopting, across every block type currently supported:

|                                                            |          |
| ---------------------------------------------------------- | -------- |
| Round-trip stable (a second save produces identical bytes) | 12 of 12 |
| Byte-identical to the source on the first pass             | 11 of 12 |

The exception is tables, which normalise their column padding once and are
stable afterwards. Files do not churn on repeated saves.

Rust still reads raw markdown for the Phase 4 index — pulling out search text
and `[[id]]` links — but that is extraction from text, not re-serialisation, and
it carries no fidelity requirement.

### Escaping in the serialised markdown

The serialiser escapes characters that would otherwise be markdown syntax. This
is lossless and stable — the text reads back exactly as typed, and a second save
produces identical bytes — but the raw file is noisier than what was typed:

| Typed   | On disk     | Reads back as |
| ------- | ----------- | ------------- |
| `[001]` | `\[001\]`   | `[001]`       |
| `a_b`   | `a\_b`      | `a_b`         |
| `<=>`   | `&lt;=&gt;` | `<=>`         |

Worth knowing for a vault full of Miller indices. Nothing is lost, and nothing
churns, but someone opening the file in another editor will see the backslashes.

The `<=>` case looks alarming given mhchem uses it, but chemistry is unaffected:
in Phase 5 `\ce{…}` lives inside a maths node that declares its own
`renderMarkdown`, so its content never passes through the prose text escaper.
That is the same property that made this architecture the right choice.

### Postscript: what this bought in Phase 5

The maths nodes proved the argument. `\ce{Sb2Se3 + 3I2 <=> 2SbI3 + 3Se}` is
exactly the string the prose serialiser would have mangled — `<=>` into
`&lt;=&gt;`, every `\` doubled — and it round-trips byte-identically, because
the node owns its own `renderMarkdown` and the text never reaches the escaper.

Ten round-trip cases, all lossless, including formulas full of `_`, `*` and
`{}`; and `$5 and $7` in prose still produces no maths nodes at all.

### Known serialiser normalisations

Two inputs are rewritten once on first save and stable thereafter. Both render
identically before and after, so nothing is lost, but the file on disk differs
from what was typed:

| Typed    | On disk                                                                |
| -------- | ---------------------------------------------------------------------- |
| `\$5`    | `$5` — the escape is dropped, since the serialiser has no rule for `$` |
| `$a$$b$` | `$a\n$$b$` — two adjacent inline formulas with no separator            |

The second is a genuine mis-parse rather than a normalisation: the block rule
claims the middle `$$`. Tightening the pattern to catch it risks breaking
ordinary display maths, and `$a$$b$` is not something anyone writes, so it is
recorded rather than fixed.
