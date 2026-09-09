# What a Sutra vault is

A vault is a directory. Everything below describes what Sutra writes into one
and what it will tolerate reading back. The contract is deliberately generous
about input and conservative about output: Sutra reads more shapes than it
writes, so a vault edited by another tool, by hand, or by a newer version keeps
opening.

## Layout

```
Vault/
  Research/                     ← real directories, up to 4 deep
    Sb2Se3/
      Growth.md                 ← one note, one file
      .attachments/             ← hidden; figures owned by notes here
        01H…_dsc.png
  Library/                      ← convention for source notes, not a rule
  Inbox/                        ← where capture lands; an ordinary folder
  Views/                        ← saved queries, which are notes
  .sutra/                       ← Sutra's own; safe to delete
    trash/
    backups/
```

`.sutra/` holds nothing that is not derived from the markdown beside it or on
its way out. Deleting it loses the trash and old migration backups, nothing
else. The SQLite index is **not** here — it lives in the OS application data
directory, because an index is machine-local and a vault is a folder people
sync.

## A note file

```markdown
---
id: 01HQ3M8K2P0000000000000001
type: literature
title: Sb2Se3 growth
created: 2026-08-21T10:14:00Z
updated: 2026-09-09T09:00:00Z
tags: [sb2se3, cvt]
---

Prose. Links as [[01HQ…]], citations as [@01HQ…], maths as $E_g$.
```

### Required

| Field                | Meaning                                                                 |
| -------------------- | ----------------------------------------------------------------------- |
| `id`                 | ULID. The note's identity, forever. Never derived from anything else.   |
| `title`              | Display name. Not the filename, though the filename is derived from it. |
| `created`, `updated` | RFC 3339, whole seconds so a person can read them.                      |

A file with **no** frontmatter is still a note: Sutra adopts it, taking the
title from the filename and an id from the path, and writes real metadata the
first time it is saved. Dropping a plain `.md` into a vault is a supported way
to get it in.

### Optional

`type`, `tags`, `icon`, `cover`, `position`, `source`, `sources`,
`not_duplicates`, `view`. All default when missing.

### Source (`type: source`)

Bibliographic fact only, under a `source:` key: `authors`, `year`, `container`,
`doi`, `url`, `zotero`, `citation_key`, `abstract_text`, `item_type`, `added`,
`collections`, `pdf`, `styled`.

`styled` is a cache of Zotero's own rendering, keyed by CSL style id, so a
citation still reads correctly with Zotero closed. It is a copy of an answer,
never Sutra's own formatting.

`abstract_text` is the publisher's abstract or absent. It is never a generated
summary — a reader must be able to trust that everything under `source:` came
from the publisher or from the person typing.

### Evidence (`sources:`)

A list. Each entry is one reading of one source:

```yaml
sources:
  - eid: 01HQ3M8K2P00000000000000EV # this evidence's own identity
    id: 01HQ3M8K2P00000000000000SRC # which source note
    page: S12
    quote: thermal conductivity decreases
    kind: experimental
    captured: 2026-09-09T09:00:00Z
```

`eid` is new in v0.3 and is what makes a piece of evidence a thing that can be
referred to rather than an anonymous row. A v0.2 note has none; that is valid
and reading one does not add one. Ids are minted when a citation is written
through the app.

## Unknown fields

Tolerated, and preserved where practical. A view term this version cannot read
is kept verbatim and reported rather than dropped, so a vault opened in an
older build and saved again does not lose what a newer one wrote. Extend this
behaviour rather than adding strict validation.

## What is _not_ in the vault

API keys. They live in the platform credential store, or failing that in the
application config directory — never in the vault, because a vault gets synced,
shared and backed up, and a credential does not belong anywhere that happens
to it.

## Reading a vault without Sutra

Everything above is plain text. The two things that are not immediately legible
are `[[ULID]]` links and `[@ULID]` citations, which name notes by id rather
than by title. v0.3 writes `[[ULID|Title]]` for new links so the human half is
present; see `docs/decisions/0001-citation-and-link-syntax.md` for why the
citation form was left alone.
