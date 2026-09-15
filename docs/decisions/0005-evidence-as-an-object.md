# Evidence as an object, and Interpretation as one too

Status: **decided for v0.5.** Follows the audit in
[v0.5 — what Evidence is today](../design/v0.5-evidence-audit.md), which found
that neither Evidence nor Interpretation can currently be pointed at, and set
out the shapes available. This records which were chosen and why.

Two decisions, taken together because the second is what the first is _for_:
evidence that cannot be pointed at cannot be traced to.

## Evidence lives on the Source note it came from

**Decision: an `evidence:` block on the Source note. Notes reference a record
by its `eid`; the record itself has exactly one home.**

Until now an Evidence record lived inside the frontmatter of whichever note
cited it. That is the whole reason one quote cannot be shared: `sources:` is a
key in one note, so there is nowhere a record can be that is not inside exactly
one note.

Three shapes were available and two were rejected outright:

**Copy the record into both notes.** What happens today. Two files carry the
same `eid` with independently editable text, so editing the quote in one leaves
the vault disagreeing with itself about what the paper says. This is the second
source of truth the brief forbids, and it is the specific failure that makes
provenance worthless — a file that no longer knows which words are the author's
is not a record of anything.

**One note owns it; the other holds a reference.** No duplication, but it makes
an ordinary note authoritative over another note's provenance. Archive the
owning note — a perfectly reasonable thing to do to an old draft — and a
literature note silently loses the quote its argument rests on. Provenance must
not depend on another note's lifetime.

**A home of its own**, which is what is chosen. The remaining question was
_where_, and the answer is the Source note rather than a note per quote:

- Evidence belongs to the paper, not to the reader's filing. Two people reading
  one paper, or one person reading it twice a year apart, are working on the
  same object; the Source note is already the place they share.
- The quote sits beside the bibliographic record that gives it meaning. A page
  number means something next to a DOI and very little on its own.
- One file per paper rather than one per sentence. A paper read closely
  produces thirty quotations, and thirty notes in the note list is a filing
  system nobody asked for.

This is the same move the project already made for Source. A paper is cited
rather than written, so it became its own note; evidence is quoted rather than
written, so it belongs with the thing quoted.

**The cost, stated plainly.** Source notes grow, and two notes capturing from
one paper now write to the same file. That is a concurrent-write path, and the
vault already has the machinery for it — `write_atomic`, and the sync-client
tests that rewrite files underneath a write in progress. Those tests must be
extended to concurrent evidence appends rather than assumed to cover it.

## Inline evidence stays valid, forever

**Decision: an `eid` is owned by exactly one place — inline in a note, or by a
record on a Source note. Never both. No migration runs.**

Every `sources:` entry already in a vault keeps working, unchanged and
unmigrated. Promoting one to a shared record is something a researcher does
when they want to reuse it, not something that happens to their files while
they are not looking.

This is what keeps the two-sources-of-truth problem from being reintroduced by
the fix for it: at no point does a quote exist in two writable places. A
completeness check reports a duplicated `eid` if one ever appears; it does not
pick a winner, because there is no way to know which copy the researcher meant.

## An Interpretation is a body block with an id

**Decision: interpretation stays prose under its heading, and gains an
identified block carrying the evidence it rests on.**

Today `voiceRules.ts` recognises a heading — "My interpretation" — and the text
below it is paragraphs with no identity. There is nothing for "this reading
rests on E1, E2 and E7" to attach to.

The alternatives both separate the id from the prose:

- **A frontmatter list**, mirroring `sources:`. The id then lives in the
  frontmatter and the words live in the body, joined by a heading — and a
  heading is exactly the thing a writer rewrites while thinking. The link
  breaks on an edit that should be free.
- **A note per interpretation.** Most consistent, and far too heavy: a
  paragraph of thinking becomes a file, and reading a note means opening five
  others.

Keeping the id with the text it names is what makes the link survive editing,
and it keeps markdown the thing being written rather than a rendering of
metadata stored elsewhere. The cost is new body syntax that has to parse
predictably and round-trip byte-for-byte, which is the same bar the citation
and wikilink syntaxes already meet — see
[0001 — citation and link syntax](0001-citation-and-link-syntax.md).

## What is deliberately not decided here

The exact spelling of the evidence block, the interpretation block, and the
index tables that make an `eid` findable. Those are format, and they belong in
a format document that can be revised without reopening these two questions.

Nothing here is verified. It is a decision record; what it claims about the
current code comes from the audit, which read the code rather than ran it.
