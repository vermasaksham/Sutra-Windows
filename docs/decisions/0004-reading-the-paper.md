# Reading the paper: extraction, cache, annotations

Status: **decided and implemented for v0.4.** The resolution section below was
pending a real Zotero response; that response has since been observed and the
section records what it said.

Follows [0003 — ownership is not access](0003-pdf-ownership-and-access.md),
which decided that Sutra may read a Zotero-managed PDF and owns nothing by
doing so. This decides _how_ it reads one, what it keeps, and what it does
when it cannot.

## Extraction runs in a child process

**Decision: re-invoke this same binary with a hidden argument, extract there,
return JSON on stdout.**

Extraction is the one place Sutra runs a large third-party parser over a file it
did not produce and cannot validate first — and which, being a publisher's PDF,
was frequently not produced correctly either. `pdf-extract` panics on some of
them. The release profile sets `panic = "abort"`, so a panic anywhere in the
application's process is fatal to the window someone is writing in.

`catch_unwind` is not an option: `abort` means there is nothing to catch. A
thread does not help either, for the same reason. A child process is the only
containment that actually contains, and it is total — a panicking parser takes
down a process whose entire job was that one file.

Re-invoking the same binary rather than shipping a second one: no extra artifact
to build, sign, version or keep in step, and no possibility of the two drifting
apart. `main` checks argv before Tauri starts; a normal launch falls straight
through.

Failure is deliberately one path, not two. The child reports everything —
unreadable file, parser error, bad arguments — as a non-zero exit with a line on
stderr, never as JSON. A panic produces exactly the same thing from the parent's
point of view, so there is no "clean failure" branch that a panic could slip
past.

## Page-aware, and what a page number means

**Decision: `extract_text_from_mem_by_pages`, numbered from 1, and Zotero's
page label preferred over anything Sutra computes.**

A quotation without a page is not evidence, so whole-document extraction is not
usable here even though it is cheaper.

Two different things are both called "the page", and conflating them would put
wrong numbers in citations:

|              | What it is                                          | Where it comes from            |
| ------------ | --------------------------------------------------- | ------------------------------ |
| **Position** | The nth page of the file                            | Extraction, 1-based            |
| **Label**    | The number printed on the page — "S12", "iv", "431" | Zotero's `annotationPageLabel` |

A paper offprinted from page 431 of a volume has a first page whose position is
1 and whose label is 431. **The label is what goes in a citation**, so where
Zotero supplies one it wins, and where it does not the evidence records no page
rather than an invented one. Sutra does not compute labels from a PDF's page
tree: that is a parsing problem with wrong answers, and a wrong page number in a
thesis is worse than an absent one.

## Cache: disposable, out of the vault, out of the index

**Decision: its own directory in app data, keyed by path, invalidated by
fingerprint.**

Three constraints decide this jointly, and each rules out somewhere it could
otherwise have gone:

- **Not in the vault.** Extracted text is derived from a file Sutra does not own.
  Putting it in the researcher's folder would add bytes they did not write and
  cannot correct, and would put a Zotero PDF's contents inside Sutra's boundary —
  which 0003 forbids in spirit even though the file itself is not copied.
- **Not in the notes index.** That index is rebuildable from the markdown _by
  definition_. Text from a PDF is in no markdown, so storing it there would make
  the index hold something unreconstructable and break the one architectural rule
  this application has.
- **Therefore its own place, in app data.** Deleting the whole thing costs one
  re-extraction and loses nothing. `clear_pdf_text_cache` exists so that is
  something the app can do rather than something this document asserts.

**Invalidation is by fingerprint, not by age.** An entry records the file's
length and modification time as they were when it was read; if the file no longer
matches, the entry is ignored. Annotating a paper in Zotero rewrites the file, so
this is ordinary use rather than a theoretical case.

Length and mtime rather than a content hash: a thesis's PDFs are gigabytes, and
hashing them all would be the slowest thing the app does to answer a question as
narrow as "has this changed". A file edited in place without its length or mtime
moving would defeat it. That is a knowing trade, and it is safe because the cost
of being wrong is one stale extraction of a disposable derivative — never a lost
note.

Every failure mode of the cache is a miss: no entry, unreadable entry, an entry
from an older shape, a changed file. A cache that can fail the operation it was
meant to speed up is worse than no cache.

## Annotations become evidence, and the two kinds of words stay apart

**Decision: read annotations as children of the _attachment_; map highlighted
text to `quote` and the researcher's comment to a new `comment` field; never
merge them.**

Zotero hands over a highlight and the remark written on it in one object. If
they arrive in a note as one string, the Source-versus-Interpretation invariant
is gone and no later reader can recover which words were the author's. That is
the single thing this import must not get wrong, and
`an_imported_annotation_keeps_the_authors_words_out_of_the_readers` is the test
that says so.

Three fields are added to an evidence entry, all optional and all skipped when
absent, exactly as `eid` was in v0.3 — a v0.3 note has none of them and is read
and written back byte for byte unchanged:

| Field        | Holds                        | Why                                                                     |
| ------------ | ---------------------------- | ----------------------------------------------------------------------- |
| `annotation` | Zotero's annotation key      | Makes import idempotent, and lets a researcher find the highlight again |
| `colour`     | `#ffd400`, as Zotero gave it | **Preserved, never interpreted** — see below                            |
| `comment`    | The researcher's remark      | Kept, and kept _out_ of `quote`                                         |

**On colour.** Many researchers colour-code their highlights and many do not,
and the scheme is personal, undeclared, and inconsistent even within one person.
So the colour is recorded, because the researcher chose it and discarding it
loses something real — and **nothing is derived from it**: no evidence kind, no
filter, no ranking, no meaning. Reading a private code as data would be
inventing provenance, which is the one thing this app must never do.

**Import is additive and idempotent.** Evidence already on a note is untouched:
Zotero does not become the authority on what a researcher has recorded in their
own note, and an annotation deleted in Zotero does not delete a quotation
somebody built a paragraph on. Running the import again adds only marks made
since, matched by annotation key — so evidence typed by hand, which has no key,
is never treated as a duplicate of anything.

An annotation carrying neither highlighted text nor a comment records nothing: a
page with no content is not evidence.

## Resolution: implemented, on a verified response shape

**The response that settled it**, observed against a live library:

```
itemType:    attachment
linkMode:    imported_file
key:         J938YE6Z
filename:    <present>
path:        <absent>
contentType: application/pdf
```

The absence of `path` is the load-bearing part. It is why a missing path on an
imported file is **not** treated as missing metadata: for that mode there is
nothing to miss.

**`imported_file` and `imported_url`** resolve to
`<dataDir>/storage/<key>/<filename>`. The filename is used **exactly as given** —
never slugged, re-cased or re-encoded, because it names a file another program
owns and the only correct transformation of it is none. A name carrying a
separator or `..` is _refused_ rather than rewritten: rewriting would be guessing
at what the library meant, and being wrong means reading a file outside the
library. (`imported_url` uses the same documented layout and has not been
observed.)

**`linked_file`** uses the record's absolute `path`. Where Zotero writes
`attachments:` — a path relative to the library's linked-attachments base
directory — that is **reported rather than resolved**, because the base
directory is a preference that has not been read or verified and resolving it
against the data directory would be inventing a location.

**`linked_url`** is a bookmark: no file, which is the same answer as a source
with no PDF and not a failure. **Any other mode** is named verbatim and
reported.

**The data directory** comes from `extensions.zotero.dataDir` in the profile's
`prefs.js`, and `~/Zotero` — Zotero's own default — when it is not set. The
preference is a JavaScript string, so its doubled backslashes are undoubled;
every path on the shipping platform has several.

The distinction is kept in `docs/architecture/verification.md`: observing the
response shape did not by itself verify the path rule. A real `imported_file`
paper has since been opened through `storage/<key>/<filename>`, so that branch
is now verified in real use. `linked_file` remains fixture-only because no such
attachment has been available in the real library.

## Failure is always named

Nothing here returns silence. Every outcome is text, a named unavailability, or
an error carrying a sentence a person can act on:

| What happened                  | What the researcher is told                      |
| ------------------------------ | ------------------------------------------------ |
| Zotero-managed PDF             | Sutra cannot find it yet, and why — not "no PDF" |
| File is not there              | Which file                                       |
| Parser crashed                 | Which file, and what it said before it died      |
| Took too long                  | Which file, and that it was stopped after 120s   |
| Bigger than 512 MB             | Its size and the limit                           |
| Extraction worked, pages empty | **No text layer** — a state, not a failure       |

The last is the one OCR would address. v0.4 does not do OCR, so the useful thing
is to name the state precisely rather than let it read as a broken file.
