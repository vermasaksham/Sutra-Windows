# What is verified, and by what

A standing record, kept because "tested" and "known to work on the researcher's
machine" are different claims and collapsing them is how a feature ships broken
while every test is green.

Three levels, and nothing is moved up a level without the evidence that level
names:

| Level        | Means                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------- |
| **Real use** | Exercised against the researcher's own Zotero library, vault and machine, and seen to work                      |
| **Fixtures** | Covered by automated tests against stubs, hand-built files and fake backends. Correct as far as the fixtures go |
| **Neither**  | Implemented, compiled, and never run against anything                                                           |

**Fixtures are not weaker tests.** A stub is exact, repeatable and runs in CI;
a real library is none of those. What fixtures cannot tell you is whether the
shape you built the stub from is the shape the real thing sends — which is the
one question that matters at an external boundary, and the reason the Zotero
resolver was left unimplemented rather than guessed.

## Real use

Seen working against the real library and vault:

- The Zotero **account** connection, including the key in Windows Credential
  Manager and the user id resolved from it
- **Search and import** — source notes carrying real item keys and real metadata
- **Citation rendering and the bibliography**, including IEEE, which is how the
  hexadecimal-entity bug was found
- Literature notes, duplicate detection, inline maths, Word export
- **Reading a Zotero-managed PDF**: the attachment found from the item key, the
  data directory read from `prefs.js`, the `imported_file` path rule, and the
  paper's own text extracted and shown page by page. Seen on 2026-09-14, on one
  paper (`ACS Appl. Nano Mater.` 2023), whose title, authors and abstract came
  through under "P. 1"

## Fixtures only

Implemented and covered by automated tests; **not** yet exercised against the
real library:

| Path                                                          | Fixture it is tested against     |
| ------------------------------------------------------------- | -------------------------------- |
| Zotero **annotation** reading                                 | a TCP stub returning canned JSON |
| "No text layer" — a scanned paper                             | a hand-built PDF with no text    |
| The **extracted-text cache** and its fingerprint invalidation | temporary files                  |
| **Evidence capture** from an annotation                       | an in-memory vault               |
| The **reading pane**, every state                             | a fake Tauri backend             |
| **Password-protected** detection                              | none — see below                 |

Two of these deserve naming individually, because the gap is larger than
"untested against real data":

- **Password-protected detection has never met an encrypted PDF.** `is_encrypted`
  matches the debug form of an error from a crate that is not a direct
  dependency. No test constructs an encrypted file, so the match itself is
  unexercised: it may simply never fire. The fallback is safe — such a file
  reports as an ordinary parser failure, which is what it did before the check
  existed — but the specific message is a claim without evidence.
- **Extraction has never met a publisher's PDF.** The hand-built fixture is a
  well-formed document. The reason extraction runs in a child process at all is
  that real PDFs are frequently _not_ well-formed, and none of that has been
  exercised.

## Neither

- **`PlatformStore`** — the real Windows Credential Manager — is exercised by no
  test. Only the trait contract is, through `MemoryStore`. Carried from the v0.3
  freeze audit, where it was already recorded.

## The Zotero PDF resolver

Implemented. Its branches are at different levels and are reported separately:

| Branch                               | Level        | Note                                                                                                                                                                                                           |
| ------------------------------------ | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `imported_file`                      | **Real use** | The live response was observed — `linkMode: imported_file`, `filename` present, **`path` absent** — and a real paper has since been opened through the rule `<dataDir>/storage/<key>/<filename>`, text and all |
| `imported_url`                       | **Fixtures** | Documented as the same storage layout, resolved the same way. Not observed                                                                                                                                     |
| `linked_file`                        | **Fixtures** | Implemented to documented semantics, automated-test covered, **not verified against the real library.** Stays here until such an attachment exists there to try, however well the tests pass                   |
| `linked_file` under a base directory | **Fixtures** | Reported rather than resolved — the base-directory preference has not been read or verified, so no location is invented                                                                                        |
| `linked_url`                         | **Fixtures** | A bookmark; resolves to "no file", which is not a failure                                                                                                                                                      |
| Unknown link mode                    | **Fixtures** | Named verbatim and reported                                                                                                                                                                                    |
| Zotero **data directory**            | **Real use** | `extensions.zotero.dataDir` from `prefs.js`, else `~/Zotero`. Read from the real profile — a paper resolved through it                                                                                         |

`imported_file` has now been through both halves: the response shape was
observed, and a real paper was opened through the path rule built on it. That
second half is what the distinction existed for — knowing Zotero sends
`filename` and no `path` never proved that joining them to `storage/<key>/`
finds the file.

`linked_file` is where `imported_file` used to be, and stays there: implemented
to the documented semantics and covered by tests, with no such attachment in the
real library to try it against.

## How to use this file

When a path moves up a level, move it here in the same commit as the evidence.
When something is reported as working, say which level it is at. "Tested" on its
own is the word this file exists to stop.
