# Handoff: Claude Code → OpenAI Codex, September 2026

A snapshot taken at the moment development moved from one agent to another.
`AGENTS.md` at the repository root is the working document; this is the
point-in-time record of where things stood, so a later reader can tell what was
true on 2026-09-15 from what has changed since.

Taken at `main` = `a7501c8`, PR #19 head = `04b33b7`.

## Current release

|                           |                                                             |
| ------------------------- | ----------------------------------------------------------- |
| Version in the five files | `0.4.0-rc.2`                                                |
| Latest release            | `v0.4.0-rc.2` — published 2026-09-14, **marked prerelease** |
| Installer                 | `Sutra_0.4.0-rc.2_x64-setup.exe`, 6.2 MB, unsigned          |
| Latest stable             | `v0.3.1`                                                    |

rc.2 exists to fix two bugs the owner found using rc.1 against a real Zotero
library:

- **Chemistry in titles.** Zotero stores a title's formatting as HTML, so a
  materials paper arrived as `Sb<sub>2</sub>Se<sub>3</sub> …` and was shown
  exactly like that, including in the file name on disk. `<sub>`/`<sup>` now
  become Unicode — **Sb₂Se₃**, **α-Sb₂O₃**, **Sb³⁺**, **10⁻⁴ S cm⁻¹** — rather
  than being flattened, because a subscript is the one part of a title whose
  meaning is carried entirely by its position. Applied at **display** as well as
  at import, so notes already in the vault read correctly without their files
  being rewritten.
- **"Source not in this vault" about a note that was in the vault.** Citations
  resolved against the _source-typed_ notes only, so a citation pointing at a
  note of any other type was reported as deleted. Now resolved against every
  note, with three distinguished states where there was one.

Also in rc.2: `Read text` → **`Read paper text`**, and reading mode says
**Extracted PDF text** above the paper's title.

## Active roadmap

**v0.4** is feature-complete and in release candidate. `docs/roadmap/v0.4.md`
carries the capability table; its one open question — resolving a
Zotero-managed PDF to a path — was settled on 2026-09-14 and the roadmap has
been corrected to say so.

**v0.5** makes research evidence a first-class, traceable object, along the
chain Source → Evidence → Interpretation → Research Question. The owner's brief
is referenced by section number throughout the commits and docs.

| Section                                                                          | State                                  |
| -------------------------------------------------------------------------------- | -------------------------------------- |
| §1 Evidence fields (`eid`, `zotero`, `page_index`, `origin`)                     | Built                                  |
| §2 Evidence types (claim/measurement/method/result/limitation/quote/observation) | Built                                  |
| §3 "paper says" vs "I think"                                                     | Built, and enforced by the type system |
| §4 Capture workflow                                                              | Built in v0.4, extended in v0.5        |
| §5 Evidence browser                                                              | Built                                  |
| §6 Reuse across notes                                                            | Built                                  |
| §7 Interpretation → Evidence traceability                                        | **Not started**                        |
| §8 Research Questions                                                            | **Not started**                        |
| §9 Provenance completeness checks                                                | **Not started**                        |
| §10 Citation ↔ Evidence groundwork                                               | **Not started**                        |
| §11 Export provenance                                                            | **Not started**                        |

Out of scope for v0.5 and not to be started: OCR, embeddings, vector databases,
RAG, "Ask Sutra", AI summarisation, automatic claim extraction, automatic
evidence classification, automatic research-question generation, cloud sync,
collaboration, mobile, bulk annotation ingestion by default.

## Unfinished work

### PR #19 — v0.5 foundation. Open, green, deliberately unmerged.

- Branch `claude/sutra-project-setup-hy61uo`, head `04b33b7`, base `main`
- Both CI jobs green; `mergeable_state: clean`; no reviews
- https://github.com/vermasaksham/Sutra-Windows/pull/19

Eight commits: `8310fed` audit → `98303cb` ADR 0005 → `e4de4b3` slice 1 →
`8cdd015` vault format → `f80457c` slice 2 → `9edad85` slice 3a → `6d38061`
slice 3b → `04b33b7` slice 3c.

**It is unmerged on purpose.** It changes how notes are written and the owner
has not reviewed it. It should not be merged to make the repository look tidy.

Three decisions in it want a human's eye:

1. **Evidence lives on the Source note** — the owner chose this over a note per
   quote. The cost, stated rather than glossed: Source notes grow, and two notes
   capturing from one paper write to the same file.
2. **A shared record carries no `comment`.** This was not designed in; it fell
   out of asking who owns each field. A record belonging to the paper cannot
   hold one reader's opinion. It constrains everything §7 builds.
3. **Captures no longer write a page label the app does not know.** Already
   visible in rc.2: the button says "PDF p. 2" and leaves the citation page
   blank.

### Nothing else is in flight

Working tree clean. No stashes. No unpushed commits. Local branches
`claude/sutra-cleanup-v03` and `claude/sutra-v04-spec` are fully merged;
`v05-local` holds pre-cherry-pick duplicates of commits now on PR #19.

## Recent important PRs

| PR               | What it settled                                                                  |
| ---------------- | -------------------------------------------------------------------------------- |
| **#19** _(open)_ | v0.5 foundation — Evidence becomes an object with one home                       |
| **#18**          | Sutra 0.4.0-rc.2 — version bump and release notes                                |
| **#17**          | Zotero markup in titles read as chemistry; citations resolved against every note |
| **#16**          | Sutra 0.4.0-rc.1 — the first candidate for reading a real library                |
| **#15**          | `ZoteroPdf::locate` — the path rule, built on an observed real response          |
| **#14**          | The five parts of v0.4 tracked separately; the vault-PDF question settled        |
| **#13**          | The reading pane: select a sentence, and it becomes evidence                     |
| **#12**          | v0.4 backend foundation, and the reading workflow proposed                       |

## Verified against real user data

Exercised against the researcher's own Zotero library, vault and machine:

- The Zotero **account** connection, including the key in Windows Credential
  Manager and the user id resolved from it
- **Search and import** — source notes carrying real item keys and metadata
- **Citation rendering and the bibliography**, including IEEE — which is how the
  hexadecimal-entity bug was found
- **Literature notes, duplicate detection, inline maths, Word export**
- **Reading a Zotero-managed PDF**: the attachment found from the item key, the
  data directory read from `prefs.js`, the `imported_file` path rule, and the
  paper's text extracted and shown page by page. Seen 2026-09-14 on one paper
  (_ACS Appl. Nano Mater._ 2023), whose title, authors and abstract came through
  under "P. 1".

## Fixture-only — implemented, never met real data

- Zotero **annotation reading** (a TCP stub returning canned JSON)
- **`linked_file`** resolution — no such attachment exists in the real library
- The **"no text layer"** path (a hand-built PDF with no text)
- The **extracted-text cache** and its fingerprint invalidation
- **Evidence capture from an annotation** (an in-memory vault)
- The **reading pane**, every state (a fake Tauri backend)
- **Password-protected detection** — has never met an encrypted PDF. The match
  is on a debug string from a crate that is not a direct dependency and **may
  simply never fire**. The fallback is safe; the specific message is a claim
  without evidence.
- **The whole of v0.5.** Storage, browser and share control have not touched a
  real vault.

Four smoke tests were requested after rc.1 and never reported back: annotation
import, a moved PDF, Zotero closed mid-read, and `linked_file`. That is why they
sit here rather than above.

## The exact next task

**Review PR #19 and decide whether it merges.** Nothing else should start on top
of it.

It is green and mergeable; what it needs is a judgement on the three decisions
listed under _Unfinished work_ above, because they constrain everything after.
The most consequential is the second — a shared record carrying no `comment` is
the storage-level form of "paper says" vs "I think", and §7's interpretation
blocks are built on top of it.

**After that, in order:**

1. **§7 — interpretation blocks.** ADR 0005 decided an Interpretation is a body
   block with an id, so the id stays with the prose it names and survives the
   heading being rewritten. This introduces **new markdown body syntax**, which
   has to parse predictably and round-trip byte for byte — the same bar the
   citation and wikilink syntaxes already meet (ADR 0001). It was held back
   deliberately so it would not land on an unreviewed base.
2. §9 completeness checks — deterministic, read-only, report-never-repair. No
   model change needed, so it can run in parallel with §7 if useful.
3. §8 Research Questions, §10 citation↔evidence, §11 export provenance.

**Before §7, one cheap fix is worth taking:** `set_citations` restoring a dropped
`eid` is in place, but §7 is what makes the id genuinely load-bearing — an
interpretation that says it rests on `E7` is a broken argument if that id is ever
silently re-minted. Re-read `an_evidence_id_dropped_in_transit_is_restored_rather_than_reminted`
before touching that path.

**Separately, and not part of v0.5:** the `SourceMeta` IPC field-name mismatch
(`citation_key` vs `citationKey`) is a real bug with a known cause, documented in
`src/vault/api.ts`. It is a two-line fix and deserves its own PR.
