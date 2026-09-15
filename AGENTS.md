# AGENTS.md — working on Sutra

Written for a coding agent picking this project up with no access to the
conversation that built it. Everything here is checkable against the repository;
where something is a claim rather than a fact, it says which.

**Read `docs/architecture/invariants.md` before changing anything.** It is the
shortest document that will stop you breaking something expensive.

## What this is

**Sutra** (सूत्र, "thread") — a local-first, block-based notes application for
one materials-chemistry PhD researcher. Notion's writing experience, with plain
markdown on disk as the source of truth, LaTeX maths and chemical equations as
first-class citizens, and Zotero as the bibliographic authority.

It is a personal project with one user. That shapes real decisions: one
installer rather than two, no fleet deployment, no telemetry, no accounts.

## Stack

| Layer    | Choice                                         |
| -------- | ---------------------------------------------- |
| Shell    | Tauri v2                                       |
| Backend  | Rust (`src-tauri/src/`)                        |
| Frontend | React + TypeScript + Vite (`src/`)             |
| Styling  | Tailwind CSS v4                                |
| Editor   | TipTap (ProseMirror)                           |
| Maths    | KaTeX + mhchem                                 |
| Index    | SQLite (rusqlite, FTS5)                        |
| Tests    | `cargo test`, Vitest, Playwright               |
| Target   | Windows 10+ primarily; macOS kept working free |

Rust owns the filesystem, frontmatter, the SQLite index, search, backlinks, file
watching, PDF reading and export. React owns everything visual, editor state,
navigation, and markdown ↔ editor conversion. **They meet only at Tauri
commands — no filesystem paths and no SQL cross that boundary.**

Markdown parsing/serialisation lives in the **frontend** via `@tiptap/markdown`,
not in Rust. Rust reads and writes note files as opaque text plus frontmatter.

## Source-of-truth rules

These are not style preferences. Breaking one is a vault-format break.

1. **Markdown files are the truth. SQLite is a disposable index.** Nothing is
   written to the database that does not already exist in a `.md` file. Delete
   the database and the app rebuilds it, losing nothing.
2. **Identity is a ULID inside the file** — never the filename, path or title.
   A note can be renamed and moved without any link changing.
3. **A note's folder is where its file is**, not something the file claims. v0.1
   stored it in a `parent:` key; that drifted and is dead.
4. **Zotero is the bibliographic authority.** Sutra does not parse author names,
   invent citation keys, or reimplement CSL. It asks, and caches the answer _as
   a copy of an answer_.
5. **Source → Evidence → Interpretation → Question stays explicit.** See
   invariants; this is the whole point of the application.
6. **No silent rewriting, merging, classification, citation creation or evidence
   invention.** Disagreements are reported; the researcher decides.
7. **Provenance must survive moves, renames, index rebuilds and export.**

## Invariants

`docs/architecture/invariants.md` lists eighteen, each with the test that holds
it. The five most expensive to get wrong:

- **Citation ≠ evidence ownership.** A `[@ref]` in prose is a _use_ of evidence.
  Deleting the sentence must not delete the page number and transcribed quote.
  It looks like tidiness. It is data loss.
- **Evidence ≠ Interpretation.** What the paper says (`quote`) and what the
  researcher thought (`comment`) are separate fields and never merge. The Zotero
  annotation importer exists largely to preserve this split.
- **`page` ≠ `page_index`.** `page` is the number printed on the paper and is
  what a citation carries; `page_index` is the nth page of the file. A paper
  offprinted from 431 has `page_index: 1` and `page: "431"`. **Absent beats
  invented** — see ADR 0004.
- **An `eid` has exactly one home.** Inline in one note, or in one Source note's
  `evidence:` list — never both. This is what keeps shared evidence from
  becoming two editable copies of one quotation.
- **Per-note cost ≠ vault size.** Opening, saving, reindexing and backlinking one
  note must cost the same at 50,000 notes as at 1,000. `benchmarks.rs::assert_flat`.

## Build, test, lint

Run from the repository root. These are exactly what CI runs
(`.github/workflows/windows.yml`).

```bash
npm ci                          # install; the lockfile is authoritative

npx tsc --noEmit                # frontend types
npx prettier --check src        # frontend format (use --write to fix)
npm test                        # Vitest — 121 tests, 16 files
npm run e2e                     # Playwright — 88 tests, 14 files

cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml   # 479 tests, 6 ignored

npm run tauri:dev               # run the app
npm run tauri:build -- --bundles nsis   # the installer CI publishes
```

Counts above were true at `04b33b7`; treat them as a floor, not a target.

**In a sandbox without network**, add `--offline` to cargo commands. Playwright
needs a browser: this repo's CI installs one; a preinstalled Chromium can be
pointed at with `PLAYWRIGHT_CHROMIUM_PATH`.

**`clippy -D warnings` builds the non-test target too.** A helper used only by
tests fails the lint. Either use it in production code or do not add it yet.

## Release process

Full detail in `docs/releasing.md`. In short:

1. Bump the version in **five** files: `package.json`, `package-lock.json`,
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and the `sutra` entry in
   `src-tauri/Cargo.lock`. The lockfiles are not optional — `npm ci` and a
   `--locked` cargo build both refuse a lockfile that disagrees with its
   manifest.
2. Write `docs/releases/vX.Y.Z.md`. It becomes the release notes verbatim.
3. Push a tag `vX.Y.Z`, or dispatch the **Release** workflow with the tag as
   input. It runs the full Windows suite, builds the installer, and publishes.

The workflow **refuses to publish** unless `package.json`, `tauri.conf.json` and
`Cargo.toml` all agree with the tag. A tag containing `-` is published as a
prerelease — this matters: `updates.rs` asks GitHub for `releases/latest`, which
omits prereleases, so an unmarked release candidate would be offered to everyone
as an update to install.

**Known CI flake, not yours:** the Tauri bundler downloads NSIS and
`nsis_tauri_utils.dll` from `github.com/tauri-apps` at build time. That host
returned `504` twice on 2026-09-14 and the Installer job failed both times while
the code compiled fine. Re-run the job. A proposed cache step is on PR #18's
comment thread, unpushed.

## Current version and latest release

|                           |                                                                                                                               |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Version in the five files | `0.4.0-rc.2`                                                                                                                  |
| Latest release            | [`v0.4.0-rc.2`](https://github.com/vermasaksham/Sutra-Windows/releases/tag/v0.4.0-rc.2), published 2026-09-14, **prerelease** |
| Latest stable             | `v0.3.1`                                                                                                                      |
| `main` at handoff         | `a7501c8` — "Sutra 0.4.0-rc.2 (#18)"                                                                                          |

The installer is unsigned; SmartScreen warns on first run. Code signing needs a
certificate the owner must buy — see `docs/releasing.md`.

## Implementation state

**v0.3 — shipped and frozen.** Vault format, migrations, recovery, performance
at 50k notes, the Zotero bibliographic integration (two providers behind one
trait, CSL styling, bibliography, Word export), chapters, views, duplicates.
`docs/architecture/v0.3-freeze-audit.md` records what state each invariant was
frozen in.

**v0.4 — feature-complete, in release-candidate.** Reading a paper and taking
evidence out of it: child-process PDF extraction, page-aware mapping, a
disposable out-of-vault text cache, Zotero annotation import, selection →
evidence capture, and a named failure for all seven states a PDF can be in.
`docs/roadmap/v0.4.md` has the capability table.

**v0.5 — foundation built, unmerged.** See _Unfinished work_.

## Verification levels — read this before claiming anything works

`docs/architecture/verification.md` is a standing record with three levels:
**Real use** (exercised against the researcher's own library and machine),
**Fixtures** (automated tests against stubs), **Neither**.

Keep the distinction. It exists because "tested" and "known to work on the
researcher's machine" are different claims, and collapsing them is how a feature
ships broken with every test green.

**Real use** includes: the Zotero account connection, search and import, citation
rendering and the bibliography, literature notes, duplicate detection, inline
maths, Word export, and reading a Zotero-managed `imported_file` PDF end to end.

**Fixtures only** includes: Zotero annotation _reading_, the "no text layer"
path, the extracted-text cache, evidence capture from an annotation, the whole
reading pane, `linked_file` resolution, and password-protected detection —
which has **never met an encrypted PDF** and may simply never fire.

**All of v0.5 is fixtures.** Nothing on that branch has touched a real vault.

## Active roadmap

`docs/roadmap/v0.4.md` is the v0.4 record. v0.5's scope came from the owner as a
written brief; its decisions are captured in **ADR 0005** and
`docs/design/v0.5-evidence-audit.md`. The brief's numbered sections are
referenced throughout as §1–§16.

v0.5 makes research evidence a first-class, traceable object. Done: §1 (evidence
fields), §2 (evidence types), §3 (quote/comment split), §5 (Evidence browser),
§6 (reuse across notes). Not started: §7 interpretation blocks, §8 Research
Questions, §9 provenance completeness checks, §10 citation↔evidence links,
§11 export provenance.

**Out of scope for v0.5, explicitly:** OCR, embeddings, vector databases, RAG,
"Ask Sutra", AI paper summarisation, automatic claim extraction, automatic
evidence classification, automatic research-question generation, cloud sync,
collaboration, mobile, and bulk annotation ingestion by default.

## Unfinished work — exact locations

### PR #19 — v0.5 foundation (open, green, **not merged**)

- **Branch:** `claude/sutra-project-setup-hy61uo`
- **Head:** `04b33b7`
- **Base:** `main` at `a7501c8`
- **CI:** both jobs green; `mergeable_state: clean`; no reviews
- **URL:** https://github.com/vermasaksham/Sutra-Windows/pull/19

Eight commits, each standing alone and readable in order:

| Commit    | What                                                                     |
| --------- | ------------------------------------------------------------------------ |
| `8310fed` | Audit of what Evidence is today, and the architectural conflict          |
| `98303cb` | ADR 0005 — where Evidence lives, what an Interpretation is               |
| `e4de4b3` | Slice 1 — the `eid` made load-bearing; three fields; page-provenance fix |
| `8cdd015` | The v0.5 vault format for shared evidence                                |
| `f80457c` | Slice 2 — storage: one quotation, one home; schema 9 → 10                |
| `9edad85` | Slice 3a — the evidence query                                            |
| `6d38061` | Slice 3b — the Evidence browser                                          |
| `04b33b7` | Slice 3c — the share control                                             |

**It is deliberately unmerged.** It changes how notes are written and the owner
has not reviewed it. Do not merge it to tidy the repository. Three things in it
were judgement calls that deserve a human's eye:

1. Evidence lives on the **Source note** (the owner chose this over a note per
   quote). Cost: Source notes grow, and two notes capturing from one paper write
   to the same file.
2. **A shared record carries no `comment`.** This fell out of asking who owns
   each field and is a constraint on everything §7 builds.
3. Captures **no longer write a page label** the app does not know. This is a
   visible behaviour change already shipped in rc.2.

### Not started

§7 interpretation blocks, §8 Research Questions, §9 completeness checks, §10
citation↔evidence links, §11 export provenance. §7 introduces new markdown body
syntax and was held back on purpose until PR #19 is reviewed — starting it would
make the whole thing reviewable only as one lump.

### Stale local branches (safe to delete)

`claude/sutra-cleanup-v03`, `claude/sutra-v04-spec` are fully merged into
`main`. `v05-local` holds pre-cherry-pick duplicates of five commits now on PR
#19 — content-identical, nothing unique. None exist on the remote.

## Known bugs and limitations

| Thing                                                                                                                                                                                                                                                                                              | Status                                                                                                                                                                                                 |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **`SourceMeta` field names do not cross the IPC boundary.** The struct has no `rename_all`, so Rust sends `citation_key`, `abstract_text`, `item_type` while the frontend reads `citationKey`, `abstractText`, `itemType`. The citation key and abstract therefore never appear in Source details. | **Real bug, unfixed, documented in `src/vault/api.ts`.** Proved by serialising the struct. Left alone because it is not v0.5's, and fixing it inside an unrelated release is how a fix goes unnoticed. |
| Installer is unsigned                                                                                                                                                                                                                                                                              | Needs a certificate; owner's purchase                                                                                                                                                                  |
| No OCR                                                                                                                                                                                                                                                                                             | A scanned paper stays a scan, and says so                                                                                                                                                              |
| Numeric citations numbered per note                                                                                                                                                                                                                                                                | Not across a thesis                                                                                                                                                                                    |
| Vault-PDF attachment UI                                                                                                                                                                                                                                                                            | Backend complete, no control to reach it. Recorded as possible v0.4.x                                                                                                                                  |
| `linked_file` resolution                                                                                                                                                                                                                                                                           | Implemented and fixture-tested, never seen real data                                                                                                                                                   |
| Password-protected PDF detection                                                                                                                                                                                                                                                                   | Match may never fire; fallback is safe                                                                                                                                                                 |
| Auto-update                                                                                                                                                                                                                                                                                        | Wired, but never offers a prerelease                                                                                                                                                                   |
| Three positional `SELECT`s feed one row mapper in `index.rs`                                                                                                                                                                                                                                       | Adding a column before `updated` silently returns **empty results** rather than erroring. Bit us at schema 10. Add columns at the end, or fix all three.                                               |

### Unreported smoke tests

These were asked for after rc.1 and never came back: Zotero annotation import
against the real library, a PDF that has moved since Zotero recorded it, Zotero
closed mid-read, and `linked_file`. They remain fixtures-only for that reason.

## Read these first, in this order

1. `docs/architecture/invariants.md` — the eighteen separations, each with its test
2. `docs/architecture/vault-contract.md` — what is in a note file, and what is not
3. `docs/architecture/verification.md` — what is actually known to work
4. `docs/decisions/0005-evidence-as-an-object.md` — the newest and most active decision
5. `docs/design/v0.5-evidence-audit.md` — the audit behind it, with the migration risk list
6. `docs/decisions/0004-reading-the-paper.md` — extraction, cache, page semantics
7. `docs/decisions/0003-pdf-ownership-and-access.md` — why no PDF is ever copied in
8. `docs/roadmap/v0.4.md` — what v0.4 was for, and what it was not
9. `docs/releasing.md` — the five version files and the two blocked things
10. `docs/manual.md` — the user-facing manual

ADRs 0001 (citation and link syntax) and 0002 (chapter assembly) matter when
touching those areas.

## Do not modify casually

| Path                                        | Why                                                                                                                                                                                      |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/src/frontmatter.rs`              | Defines the on-disk vault format. Every field is `skip_serializing_if` so old notes round-trip byte for byte. Adding a key is fine; changing or removing one is a vault break.           |
| `src-tauri/src/pdfread.rs`                  | The read-only boundary around externally-owned PDFs. Guarded by `the_read_boundary_contains_no_write`, which scans the production half of the file for write calls. ADR 0003 governs it. |
| `src-tauri/src/index.rs`                    | Schema and migrations. Bumping `SCHEMA_VERSION` forces a rebuild — fine, it is derived — but see the positional-`SELECT` trap above.                                                     |
| `src-tauri/src/pdftext.rs`                  | Runs a third-party parser in a **child process** because `panic = "abort"` makes a panic fatal to the window. `catch_unwind` is not an option here.                                      |
| `.github/workflows/release.yml`             | Refuses to publish on a version/tag mismatch, and marks prereleases. Both guards exist because of specific incidents.                                                                    |
| `docs/architecture/invariants.md`           | A change here is a change to what the app promises.                                                                                                                                      |
| `src-tauri/Cargo.lock`, `package-lock.json` | Version-bumped by hand during a release; CI refuses a lockfile that disagrees with its manifest.                                                                                         |
| `e2e/vault.ts`                              | The fake Tauri backend every e2e test runs against. It must mirror what Rust actually does, including field names — two bugs have hidden in the gap.                                     |

## Migration and backward compatibility

**No migration runs for v0.4 or v0.5.** Every key added since v0.3 is
`#[serde(default, skip_serializing_if = ...)]`, so a note written by an older
build is read and written back byte for byte. This is asserted, not assumed —
see `a_v0_4_record_gains_no_v0_5_keys_by_being_read_and_written` and
`a_note_with_no_shared_evidence_gains_no_evidence_key`.

Rules to preserve:

- **Additive keys only.** New frontmatter keys must be optional and skipped when
  absent, so an older build reading a newer vault loses nothing.
- **Unknown values are kept verbatim**, never dropped or translated. An evidence
  `kind` from a newer build, a view term this version cannot read, an unknown
  `origin` — all survive a round trip. v0.5 changed what `kind` is _about_ and
  deliberately did **not** translate old values: "experimental" does not mean
  "measurement".
- **An entry with no `at:` is the record itself** — every citation written before
  v0.5 and every one still written inline.
- Where a migration _is_ needed, `docs/architecture/migrations.md` has the
  six-step contract: detect, plan, preview, back up, apply, verify. Migrations
  are idempotent; an interrupted run must be indistinguishable from a finished
  one asked to run again.

## Working style this repository expects

Visible in every commit message and doc, and worth continuing:

- **Reproduce before fixing.** Several tests here were checked failing-first
  against the old behaviour, and say so.
- **Say which verification level a claim is at.** Never call something verified
  because a fixture passes.
- **Report negative results.** Unfinished, unverified and broken things are
  written down rather than quietly left.
- **Comments explain why, not what.** The codebase is dense with reasoning about
  decisions that took a bug to find.
