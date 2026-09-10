# What Sutra costs as a vault gets large

Frozen at v0.3.

This is the answer to one architectural question — **which operations get slower
as the vault grows?** — because that is the question a storage contract can be
frozen against. An operation whose cost tracks the size of the vault is a design
decision that has to be defensible. An operation that quietly started tracking it
is a bug.

Reproduce with:

```
cargo test --bins -- --ignored --nocapture baseline_at_1k
cargo test --bins -- --ignored --nocapture baseline_at_10k
cargo test --bins -- --ignored --nocapture baseline_at_50k
```

The harness is `src-tauri/src/benchmarks.rs`. It asserts as well as prints:
per-note operations are checked against a ceiling that does **not** scale with
the vault, and whole-vault reads against a **per-note** budget. That distinction
is the test — it is what separates linear from quadratic.

## How to read these numbers

**They are from an unoptimised build.** `cargo test` without `--release` runs
debug rusqlite, debug serde and debug everything else; the shipping Windows
binary is substantially faster. These are the numbers to compare against each
other, not the numbers a user sees. Measured on one Linux dev machine, notes
spread across 96 folders, bodies of a few hundred words.

## The measurements

50,000 notes is far past what this app was built for — a PhD's worth of research
notes is thousands, not tens of thousands. It is in the table because a shape
only shows up when you push it.

| Operation                   | 1,000   | 10,000  | 50,000  | Shape                     |
| --------------------------- | ------- | ------- | ------- | ------------------------- |
| open a note                 | 71 µs   | 88 µs   | 125 µs  | flat                      |
| save an edit                | 1.27 ms | 0.89 ms | 1.34 ms | flat                      |
| reindex one saved note      | 2.05 ms | 1.65 ms | 2.26 ms | flat                      |
| one note's backlinks        | 97 µs   | 114 µs  | 121 µs  | flat                      |
| search, rare term           | 168 µs  | 168 µs  | 212 µs  | flat                      |
| open the index              | 1.1 ms  | 1.0 ms  | 1.4 ms  | flat                      |
| rename (moves the file)     | 1.08 ms | 1.21 ms | 2.46 ms | grows with the folder     |
| move between folders        | 126 µs  | 263 µs  | 983 µs  | grows with the folder     |
| create a note               | 2.0 ms  | 4.8 ms  | 15.8 ms | grows with the folder     |
| search, term in every note  | 2.5 ms  | 16.8 ms | 96 ms   | grows with matches        |
| related notes               | 6.0 ms  | 17.4 ms | 78 ms   | grows with the corpus     |
| **launch** (open the vault) | 81 ms   | 747 ms  | 4.18 s  | linear — reads every file |
| the startup listing         | 96 ms   | 758 ms  | 4.15 s  | linear                    |
| every tag in the vault      | 87 ms   | 713 ms  | 3.93 s  | linear                    |
| rebuild the whole index     | 364 ms  | 3.19 s  | 18.5 s  | linear                    |
| the research overview       | 104 ms  | 689 ms  | 3.42 s  | linear                    |

## The three shapes, and why each is allowed

**Flat.** Reading, saving and reindexing one note, and asking for its backlinks,
cost the same at 50,000 notes as at 1,000. They have to: they are what a person
does continuously, and a cost that grew with the vault would make the app worse
the longer it was used. Every one of these is a path lookup or an indexed SQLite
statement. The benchmark asserts a fixed ceiling for each, so a change that
introduces a scan fails the build rather than shipping.

Two of these were **not** flat before v0.3, and finding that is what this
benchmark was for. Reindexing a saved note took 9.6 ms at ten thousand notes and
one note's backlinks took 23.2 ms, because both looked rows up in the FTS table
through a column that had no index. See
[index-audit.md](index-audit.md#notes_fts-notes_vocab).

**Grows with the folder, not the vault.** Creating, renaming and moving a note all
have to pick a filename nothing else in the destination folder is using, and they
do it by reading that folder. So the cost is proportional to how many notes are in
_that folder_ — which in this benchmark is `notes / 96`, and so appears to track
the vault.

This is a deliberate trade and it is the reason a Sutra filename is human-readable
rather than a ULID. The alternative — a cached set of names per directory — would
be a cache that can be wrong about a folder people sync with OneDrive, and a
wrong answer here means overwriting a note. A directory read is the only answer
that cannot be stale.

Its consequence is worth stating plainly: **bulk-creating notes is quadratic.**
Building the 50,000-note vault took thirteen minutes, and the per-note cost rose
from 5.7 ms to 19.0 ms as the folders filled. Creating notes by hand never
notices — 19 ms is one keystroke's worth of work in a folder holding five hundred
notes — but an importer that writes thousands of notes into one folder would, and
it would need a different filename strategy. No importer exists, and this is
recorded rather than fixed.

**Linear — reads every file.** Launching Sutra, listing every tag, rebuilding the
index and drawing the research overview all read the whole vault, on purpose.
`Vault::open` walks the vault to build the id-to-path map, so the first note a
person opens does not pay for the scan. The index is rebuildable _because_ it is
rebuilt from exactly this walk, and that property is worth more than a faster
launch.

At a realistic size — a few thousand notes — these are tens to hundreds of
milliseconds and nobody notices. At 50,000 notes launch takes four seconds in a
debug build. If a vault that large ever became a real case, the fix is to
populate the path map lazily, not to keep a cache of the filesystem.

## The two to watch

Both are already visible in the table and neither is a bug:

**Search for a word that is in every note** grows with the number of matches,
because FTS5 must rank all of them before the limit can drop any. 96 ms at fifty
thousand notes is fine; it is the number that would break first if the corpus grew
again.

**Related notes** grows with the corpus, because the terms it treats as
distinctive are weighed against how many notes use them. It is bounded by design
(a dozen terms, forty notes each) and the growth is in the weighing, not the
gathering.
