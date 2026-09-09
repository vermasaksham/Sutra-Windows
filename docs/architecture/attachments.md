# Attachment ownership

Frozen at v0.3.

## Two kinds, and only one of them is Sutra's

**Sutra-owned.** Figures, images, spreadsheets, instrument output, a PDF the
researcher attached by hand. Copied into the vault, stored in a hidden
`.attachments/` directory beside the note that uses them, referenced from
markdown by a vault-relative path.

**Externally owned.** Zotero's PDFs. Sutra records the attachment's _title_ and
offers a `zotero://select` link. It does not copy the file. See
`zotero-pdfs.md`.

Never ambiguous: if the bytes are inside the vault, Sutra owns them; if they
are in Zotero's storage, Zotero does.

## `.attachments/` is storage, not hierarchy

Hidden, and excluded from the note namespace by one rule: a path component
beginning with `.` is Sutra's, not the user's. It never appears in the folder
tree, cannot be created as a folder, and cannot be read as a note. It is an
implementation detail of where figures live, and it must stay one — the moment
it shows up in the research hierarchy it has become a semantic folder the user
has to think about.

## Ownership is derived, not declared

An attachment is **owned by a note when exactly one note references it.**

Nothing records ownership; it is computed by scanning note bodies at the moment
it matters. That is deliberate. A stored owner is a second source of truth that
can disagree with the references, and this codebase has already paid for one of
those (`parent:`). The cost is a vault scan on move and delete — operations a
person performs by hand, one note at a time.

## Behaviour

| Event                        | What happens                                                                                                                                                                       |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Import**                   | Copied to `<folder>/.attachments/<ULID>_<slug>.<ext>`. The ULID prefix means two files with one name never collide.                                                                |
| **Rename note**              | Nothing. The file is renamed; the attachment path does not mention the note's name.                                                                                                |
| **Move note**                | Owned attachments move to the destination's `.attachments/`, and that note's own references are retargeted. `updated` is _not_ stamped — moving is not editing.                    |
| **Move, shared attachment**  | Left exactly where it is. Moving it would fix one note by breaking another. Both references keep resolving.                                                                        |
| **Trash note**               | Owned attachments go to `.sutra/trash/` beside the note. Moved, never unlinked, so they can be dragged back.                                                                       |
| **Trash, shared attachment** | Left alone.                                                                                                                                                                        |
| **Collision on move**        | A name already present in the destination gets a fresh ULID prefix rather than overwriting.                                                                                        |
| **Missing file**             | The reference is left saying what it says. Nothing is invented and nothing is removed — a dangling reference is information, and rewriting it would hide that the picture is gone. |
| **Unreferenced file**        | Left alone, for ever. There is no garbage collection. A file nothing points at may still be something the researcher put there.                                                    |

## What is deliberately absent

**No garbage collection.** Not sweeping, not on a timer, not on demand. The
failure mode of an over-eager collector is deleting a figure, and no amount of
tidiness is worth that. Orphans accumulate; disk is cheap and research is not.

**No stable attachment id.** Considered for v0.3 and rejected: the reference in
the markdown _is_ the identity, it is human-legible, and it works when read
outside Sutra. An id would need a registry, and a registry is durable state
about files that would have to live somewhere — reintroducing exactly the
second source of truth this design keeps removing.
