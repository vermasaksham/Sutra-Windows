# Link and citation syntax in the markdown

Status: **decided for v0.2.1**, with the migration deferred to v0.3.

## The problem

Sutra writes cross-references as `[[01HQ3M8K2P0000000000000001]]` and citations
as `[@01HQ3M8K2P0000000000000001]`. Both hold a ULID because identity has to
survive a rename — that part is not in question and is not changing.

The cost is portability. Open the vault in Obsidian, VS Code or a text editor on
a phone and the prose reads fine while every reference is an opaque 26-character
string. The v0.2 audit called this the sharpest drift from "durable,
human-understandable files".

## Links: `[[ULID|Title]]`

**Decision: parse it in v0.2.1, write it in v0.3.**

The id stays first and stays authoritative. Everything after the pipe is
display text with no power to resolve anything, so a stale title can never send
a reader to the wrong note — the title shown is still looked up from the id at
render time, exactly as it is today.

Checked before choosing it:

- **Sutra's own parser.** `links::extract` takes everything before the first
  `|`; `WIKILINK` in `WikiLink.ts` matches `[[ULID]]` and `[[ULID|anything]]`
  and captures only the id. Both are tested against the piped form.
- **Obsidian.** `[[target|alias]]` is its documented alias syntax and displays
  the right-hand side, so a Sutra vault opened there shows note names rather
  than ULIDs. This is the main reason the pipe won over alternatives like
  `[[ULID]](Title)` or a trailing comment.
- **Duplicate titles.** They cause no ambiguity, because titles never resolve.
  Two notes called "Growth" produce two links carrying the same display text
  and different ids, which is correct.

Writing it is a rewrite of every note in the vault, so it needs a preview, a
backup and a way back — the same treatment the `parent:` migration got. That is
v0.3 work. Reading it a release early is what makes that migration safe: a vault
edited by a newer build, or by hand in Obsidian, must not read as broken text
in v0.2.1.

## Citations: unchanged, and here is why

**Decision: `[@ULID]` stays exactly as it is.**

The v0.2 audit proposed `[@ULID|@citekey]`. That was investigated and
**rejected**. Pandoc's manual states the rule verbatim:

> Unless a citation key starts with a letter, digit, or `_`, and contains only
> alphanumerics and single internal punctuation characters (`:.#$%&-+?<>~/`),
> it must be surrounded by curly braces, which are not considered part of the
> key.

`|` is not in that set. `[@01HQ…|@zhou2019]` would terminate the key at the
ULID and leave `|@zhou2019]` as literal text in the output — a citation for a
key that does not exist, followed by visible punctuation. It is worse than what
we have.

Three further findings, which together are why nothing needs to change:

1. **The current syntax is already valid Pandoc.** A ULID starts with a digit
   and is alphanumeric throughout, so `[@01HQ3M8K2P0000000000000001]` parses
   cleanly as a citation. It simply does not _resolve_, because no bibliography
   is keyed that way.
2. **That is a bibliography problem, not a syntax problem.** Sutra can export a
   CSL JSON bibliography whose `id` for each entry is the source note's ULID.
   Pandoc then resolves every existing citation in every existing note with no
   change to a single file. This is the v0.3 direction.
3. **Better BibTeX keys are not stable enough to be identity.** A BBT key is
   derived from author and year and changes when either is corrected; Zotero
   only has one at all when BBT is installed, and `SourceMeta.citation_key` is
   `None` otherwise. Making the citation depend on it would reintroduce exactly
   the breakage the ULID exists to prevent. It stays recorded on the source note
   as a _fact about the library_, and is what the CSL export will use for its
   human-facing key where one exists.

So the interoperability goal is met by generating the bibliography, not by
changing what is written in the researcher's files. That satisfies all five
criteria the brief listed — immutable identity, human readability (via the
generated bibliography and the v0.3 link titles), reliable Zotero association,
clean export, and Pandoc interoperability — without a body rewrite.

## What was ruled out

| Option                      | Why not                                                                |
| --------------------------- | ---------------------------------------------------------------------- |
| `[@ULID\|@citekey]`         | `\|` is not a legal Pandoc key character; breaks Pandoc outright.      |
| `[@{ULID}]` (braced)        | Legal, but the braces buy nothing: a bare ULID is already a legal key. |
| Switch to `[@citekey]`      | BBT keys are unstable and often absent. Loses identity.                |
| Rewrite citations in v0.2.1 | This is a stabilisation release. No silent body rewriting.             |
