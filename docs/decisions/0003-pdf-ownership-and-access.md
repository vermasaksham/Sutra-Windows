# Ownership is not access

Status: **decided for v0.4.** Clarifies the v0.3 Zotero PDF boundary. Does not
weaken it.

## What v0.3 froze, and what it did not say

`docs/architecture/zotero-pdfs.md` states the boundary:

> Zotero stores the literature. Sutra stores what the researcher thinks about
> the literature.

and that Sutra "does **not** copy Zotero's PDFs". Every word of that stands.

What it did not distinguish is **ownership** from **access**. Read as written, it
could be taken to forbid Sutra from ever opening a Zotero PDF at all — which was
never the point. The point was that there must never be two files either of which
could be "the paper", and never a question about which one is authoritative.

Reading a file creates no second copy and answers no such question.

## The distinction, stated

**Ownership** is the right to move, rename, modify, delete, or duplicate a file,
and the obligation to be the place it is kept. Zotero owns every PDF in its
storage. Sutra owns the files under a vault's `.attachments/`. This does not
change.

**Access** is permission to read bytes. Sutra may read a Zotero-managed PDF.

A Zotero PDF remains **externally owned while Sutra is reading it**, and remains
externally owned afterwards. Nothing about having read a file makes any part of
it Sutra's.

## What Sutra may and may not do with a Zotero-managed PDF

May:

- resolve its path on this machine, from the attachment record Zotero already
  exposes
- open it **read-only** and read its bytes
- extract text from those bytes
- keep a derived, disposable cache of that text outside the vault
- record, in the researcher's own note, evidence the researcher captured: a page
  and a quotation, with an `eid`

Must never:

- move, rename, modify, delete or duplicate it
- write anything at all inside Zotero's data directory
- copy it into the vault, by default or as a side effect of reading
- treat its bytes, or text extracted from them, as durable research content
- require it to be present for a source note, a citation or a bibliography to
  keep working

The last two are the ones that keep the boundary real rather than nominal.
Extracted text is derived from a file Sutra does not own, so it is not research
and does not belong in the vault; and a paper's bibliographic relationship to a
note is a ULID in the researcher's own markdown, which no missing file can break.

## How the prohibition is enforced

Not by discipline. Read access lives in one module whose only filesystem
operation against Zotero's directory is a read, and a test reads that module's
own source and fails if it contains a write, rename, or remove call. The same
mechanism as `no_line_this_program_prints_interpolates_a_credential`: the failure
being guarded against is a future edit made in good faith, and a person cannot be
relied on to notice.

## What this does not authorise

**Copying.** The archival-copy design sketched in `zotero-pdfs.md` — a
`source.archive` record with `owner: sutra-archive` and a `sha256` — is still
unbuilt and still requires explicit opt-in when it is built. Nothing in v0.4
copies a PDF anywhere.

**Writing annotations back.** Annotation import reads Zotero's annotations. It
does not create, edit or delete them. Zotero remains the only writer of its own
data.

**Depending on the file.** Every capability that reads a PDF degrades to a named,
visible unavailability when the file is not there, and nothing else changes.
