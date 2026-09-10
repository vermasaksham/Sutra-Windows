# How a chapter is assembled

Status: **decided for v0.4.** Two questions the v0.3 freeze deliberately left
open, answered before anything was built, because both are visible in the vault
format and the format is frozen.

## The problem

A thesis is not a note. It is a deliberate sequence of notes, in an order the
author chose, which changes as the argument changes. v0.3 built the export path
to take an ordered list of note bodies precisely so this decision could be made
afterwards without touching the export code — but the decision itself had to be
made deliberately, not arrived at mid-implementation.

Two questions, and they are separable:

1. Where does the order live?
2. Does editing the assembled chapter edit the notes?

## Where the order lives: `sequence:` in a chapter note

**Decision: a note of `type: chapter`, whose frontmatter holds an ordered list
of ULIDs.**

```yaml
---
id: 01HQ3M8K2P0000000000000001
type: chapter
title: 3. Growth of Sb2Se3 thin films
sequence:
  - 01HQ3M8K2P00000000000000A1
  - 01HQ3M8K2P00000000000000B1
  - 01HQ3M8K2P00000000000000C1
---
```

This was chosen because it needs no new invariant. Every frozen contract holds
as it is:

- **A chapter is a note**, so it is synced, backed up, versioned, searchable and
  readable as plain text like everything else. Nothing about a thesis's
  structure lives anywhere the vault does not reach.
- **Order is frontmatter**, which is already where structured facts about a note
  live, and already hand-editable. Reordering a chapter is a text edit.
- **Identity is still the ULID**, so a note can be renamed or moved into a
  different folder and its place in the chapter is untouched. This is the whole
  reason the order is a list of ids rather than of paths or titles.
- **Nothing new goes in SQLite.** The index gains a derived answer to "which
  chapters include this note", rebuildable from the frontmatter like every other
  row.

A note may appear in more than one chapter, and the same note may appear at two
places in one chapter. Neither is forbidden, because neither is Sutra's business
to forbid — a methods note genuinely does belong to two chapters.

### The two alternatives, and why not

**A saved view with an ordering.** Tempting, because the view machinery exists.
Rejected: a chapter is a sequence the author decided, not a result a query
produced. `sequence:` is not derivable from any query, so it would have to be
stored — and "a view queries, never stores" is a frozen invariant. Bending it
here is how a view stops being a question and becomes a database.

**Folder position ordering.** The cheapest option: notes already carry
`position:`, and a folder is already an order. Rejected for one reason that is
fatal rather than inconvenient — a file lives in exactly one directory, so a note
could belong to exactly one chapter, and reordering a chapter would move files
on disk. Chapter membership is not where a note lives.

## Editing: a chapter is a read-only assembly

**Decision: the chapter shows and exports its notes in order. Editing happens in
the notes.**

The invariant most at risk in this whole feature is that a note is the single
place its text can be changed. Writing through from an assembled chapter would
create a second one, and with it a second set of answers to what a save
conflicts with, what undo undoes, and which `updated` timestamp moves.

It is also the smaller thing to get right first, and the decision is
reversible in the direction that matters: adding write-through later changes the
editor, not the file format. Making the format assume write-through and then
retreating would not be reversible at all.

The cost is real and worth stating: drafting a chapter means moving between its
notes rather than typing down one continuous page. If that turns out to be the
wrong trade in practice, the thing to revisit is this paragraph, not
`sequence:`.

## What this does not decide

**Transclusion syntax.** Nothing embeds one note's body inside another note's
prose. A chapter references notes by id in frontmatter; it does not inline them
into markdown, and no `![[id]]`-style syntax exists.

**Numbering and cross-references.** How "see Chapter 3" or "Figure 2.4" is
written and resolved is untouched here.

**Whether a chapter can contain a chapter.** `sequence:` holds note ids and a
chapter is a note, so the format permits it and nothing yet reads it that way.
