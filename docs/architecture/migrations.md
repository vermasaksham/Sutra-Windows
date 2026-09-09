# How Sutra changes a vault it did not write

Frozen at v0.3.

A migration rewrites files the user did not ask to have rewritten, in bulk,
in a folder holding their research. So the contract is deliberately heavy, and
the same for every migration:

1. **Detect** from the vault's own content — never from a stored version marker.
2. **Plan** the whole change without performing any of it.
3. **Preview** the plan to the user, including what it will not touch.
4. **Back up** every markdown file first.
5. **Apply**, in an order that makes an interruption resumable.
6. **Verify** — rebuild the index from the files, so what the app shows is what
   is actually on disk.

Nothing may skip step 4, and one of the two migrations below did skip it for two
releases. That is why the contract is written down here rather than assumed.

## Why the version is inferred, not stored

There is no `.sutra/version` file, and this is a decision rather than an
omission. A version marker has to be written, and writing one into every
existing vault the first time it is opened is exactly the silent modification
this design refuses. Worse, a marker can be wrong — restored from a backup,
merged badly, copied from another vault — and then it is a claim about the vault
that disagrees with the vault.

So each migration carries its own detector, and the detector reads content:

| Migration               | The vault needs it when                   |
| ----------------------- | ----------------------------------------- |
| Layout (v0.1 → v0.2)    | any note's frontmatter claims a `parent:` |
| Citations (v0.1 → v0.2) | any note body contains a `[@ZOTEROKEY]`   |

A vault that needs neither is a current vault. That is the whole version check,
and it cannot disagree with the files.

## The registry

| Migration     | Detector           | Plans ahead                      | Needs network    | Backs up | Idempotent | Held by                                             |
| ------------- | ------------------ | -------------------------------- | ---------------- | -------- | ---------- | --------------------------------------------------- |
| **Layout**    | `needs_migration`  | `migration_plan`                 | No               | Yes      | Yes        | `migrating_a_second_time_moves_nothing`             |
| **Citations** | `legacy_citations` | key counts, shown before running | Yes, Zotero once | Yes      | Yes        | `migrating_citations_a_second_time_changes_nothing` |

**Layout** turns a `parent:` claim in frontmatter into a real folder, because
location became the filesystem's job and two sources of truth for it had already
drifted. It plans a list of moves, names the notes whose chain was too deep to
represent, and names the files whose frontmatter would not parse — those are left
exactly where they are.

**Citations** turns `[@ZOTEROKEY]` into `[@ULID]`, pointing each citation at a
source note in the vault. It is the one migration that needs the network, once:
the keys came from Zotero and only Zotero knows what they stand for. After it,
the vault never needs Zotero to read its own citations again — which is the
entire point of doing it. Keys Zotero cannot answer for are left alone rather
than deleted, and the migration can be run again when it can.

## What makes each one resumable

Both are idempotent, and both are tested for it, because an interrupted
migration is indistinguishable from a completed one that is asked to run again.

**Layout** renames every file before rewriting any frontmatter, so an
interruption leaves files in their new homes still claiming their old parents —
the state a second run knows how to finish. Two rules make that true, and both
took a bug to find:

- A note that claims **no** parent is not the migration's business; it keeps the
  folder it is in. Deriving its folder from an empty chain of claims produced the
  vault root, so a single unmigrated note was enough to make the plan propose
  flattening every organised note in the vault.
  (`a_note_with_no_claim_is_left_in_the_folder_it_is_in`)
- When the chain of claims runs out at an ancestor that has already been
  migrated, the chain hangs off **that ancestor's real folder**, not off the
  root. Otherwise a resumed run moves a note back out of the folder the same
  migration just put it in.
  (`a_migration_interrupted_half_way_finishes_where_it_left_off`)

**Citations** rewrites each note independently, and a key it has already
rewritten is no longer in the legacy form, so a second pass finds nothing.

## Adding the third one

Write a detector that reads content and answers "does this vault need it".
Write a planner that returns what would change without changing anything. Call
`Vault::back_up` first. Make it idempotent and add the test that proves it.
Rebuild the index afterwards rather than patching it — a migration touching most
of the vault is precisely when it is worth proving the index is derived.
