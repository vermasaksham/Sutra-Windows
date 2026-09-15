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
through the app, and an id that arrives missing from a record still on disk is
restored rather than re-minted — see `set_citations`.

v0.5 adds three optional keys, absent on anything written before it:

```yaml
sources:
  - eid: 01HQ3M8K2P00000000000000EV
    id: 01HQ3M8K2P00000000000000SRC
    page: "431" # the number printed on the paper
    page_index: 1 # the nth page of the file
    quote: thermal conductivity decreases
    kind: measurement
    origin: selection # or: annotation, manual
    zotero: J938YE6Z # the item key as it was at capture
    captured: 2026-09-09T09:00:00Z
```

`page` and `page_index` are different facts and neither substitutes for the
other. A paper offprinted from page 431 has `page_index: 1` and `page: "431"`.
**The label is what a citation carries**, and where there is none the record
holds no label rather than an invented one — so text selected in the reading
pane records only `page_index`, because that is the only page fact a selection
has.

`origin` is written, never derived. `annotation` implies Zotero, but nothing
else distinguished text the app took out of a PDF from text a person typed, and
those differ in the way that matters most here. Absent on a record from before
v0.5, and nothing backfills it: a guess written into a provenance field is
indistinguishable from a fact once it is on disk.

`kind` changed meaning in v0.5. It used to name the kind of study a source
reported — "experimental", "computational", "theoretical", "review" — which is
a fact about the paper. It now names what the quoted sentence _is_: `claim`,
`measurement`, `method`, `result`, `limitation`, `quote`, `observation`. Absent
means unspecified. **An old value is kept and shown exactly as written and is
never translated:** "experimental" does not mean "measurement".

### Shared evidence (`evidence:` on a Source note)

New in v0.5, and **entirely optional**: every vault written before it, and
every record still written inline, stays valid with no migration.

One quotation used by two notes must not exist twice — two editable copies of
one `eid` is two answers to "what does the paper say". So a record that is to
be reused moves to the paper it came from, and the notes reference it:

```yaml
# On the Source note.
id: 01HQ3M8K2P00000000000000SRC
type: source
evidence:
  - eid: 01HQ3M8K2P00000000000000EV
    page: "431"
    page_index: 1
    quote: thermal conductivity decreases
    kind: measurement
    origin: selection
    annotation: ZAB12CD3
    colour: "#ffd400"
    captured: 2026-09-09T09:00:00Z
```

No `id:` on these entries: the source is the note the record is written on.

**A shared record carries no `comment`.** The quote is the paper's and the
comment is the reader's, and a record that belongs to the paper cannot hold one
reader's opinion — two people using one quotation do not share a view of it.
A Zotero annotation's comment therefore stays with the note that imported it,
never on the shared record. This is the "paper says" versus "I think" rule
applied to the one place the new storage could quietly break it.

A note referencing such a record names it in `sources:` and says where it
lives:

```yaml
# On the note doing the citing.
sources:
  - eid: 01HQ3M8K2P00000000000000EV
    id: 01HQ3M8K2P00000000000000SRC
    at: source # the record is on the Source note; this is a reference
    comment: only two samples # this reader's remark, and theirs alone
```

`at:` absent means the entry _is_ the record, which is every entry written
before v0.5 and every one still written inline. `at: source` means the content
lives on the Source note named by `id`.

**An `eid` has exactly one home.** Inline in one note, or in one Source note's
`evidence:` — never both, and never two of either. That invariant is what keeps
the fix for duplication from reintroducing it, and a completeness check reports
any breach rather than choosing a winner: there is no way to know which copy
the researcher meant.

A reference whose `eid` is on no Source note is reported, not repaired. The
quotation is not invented back, and the reference is not deleted — either would
destroy the only remaining evidence that something was there.

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
