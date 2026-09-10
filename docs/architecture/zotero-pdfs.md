# Zotero PDFs, and who owns them

Frozen at v0.3.

> **Clarified at v0.4, not weakened.** Ownership and access are different
> things: Sutra may read a Zotero-managed PDF, and reading transfers nothing. The
> file stays externally owned, and Sutra never moves, renames, modifies, deletes
> or duplicates it. See
> [`../decisions/0003-pdf-ownership-and-access.md`](../decisions/0003-pdf-ownership-and-access.md).

## The boundary

> Zotero stores the literature. Sutra stores what the researcher thinks about
> the literature.

Sutra does **not** copy Zotero's PDFs. It records, on the source note:

- `zotero` — the item key
- `pdf` — the attachment's title, so the note can say a PDF exists while Zotero
  is closed
- enough bibliographic fact (`authors`, `year`, `container`, `doi`,
  `citation_key`, `styled`) that the source note remains a usable citation with
  Zotero uninstalled

Opening the paper is a `zotero://select/library/items/<key>` link, which is
Zotero's own documented mechanism.

## Why not copy them

Duplicating a library of several thousand PDFs into a synced research vault
makes the vault enormous, makes Zotero's copy and Sutra's copy diverge the
first time an annotation is added, and creates a question nobody can answer:
which of the two is the paper? The boundary exists to make that question
impossible.

## When Zotero is unavailable

Everything above is already in the note. A vault opened on a machine with no
Zotero shows every source, every cached citation in every style previously
rendered, every recorded quote and page. What it cannot do is render a _new_
style or fetch fresh metadata, and it says so rather than degrading silently.

Losing Zotero costs you the library. It does not cost you your research
relationships — those are ULIDs in your own markdown.

## Optional archival copies — designed, not built

A future "keep an archival copy in Sutra" option is anticipated. If it is ever
built, the record must be unambiguous about ownership:

```yaml
source:
  zotero: ABCD1234
  pdf: Zhou et al. - 2019 - Quasi-1D Sb2Se3.pdf
  archive: # present only when a copy was made
    path: Library/.attachments/01H…_zhou-2019.pdf
    sha256: 5019ec68…
    zotero_attachment: WXYZ5678
    original_filename: Zhou et al. - 2019.pdf
    owner: sutra-archive # never just "sutra"
```

`owner` is explicit so that no future reader has to infer which copy is
authoritative. `sha256` is what lets a copy be checked against the original
rather than assumed current. Neither field exists in v0.3, and `pdf` remaining
a plain string is why the shape above is additive rather than a change.

**Not implemented in v0.3.** Documented so that when it is, it lands as a new
optional key rather than a redefinition of an existing one.
