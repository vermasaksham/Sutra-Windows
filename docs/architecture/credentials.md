# Where Sutra keeps secrets, and how that is checked

Frozen at v0.3.

Sutra holds two credentials, both belonging to services the user chose to
connect: a Zotero web API key and an AI provider API key. Neither is Sutra's,
and neither is research — so the design goal is not to protect them cleverly but
to keep them out of everywhere they do not belong.

## The three places a key can live

In order of preference. `resolve()` in `src-tauri/src/secrets.rs` reads them in
exactly this order, and the Settings panel names the one in effect so the user
can see it rather than assume it.

| Where                         | Chosen when                                                 | Written by Sutra |
| ----------------------------- | ----------------------------------------------------------- | ---------------- |
| An environment variable       | `ZOTERO_API_KEY` / `ANTHROPIC_API_KEY` is set and non-blank | Never            |
| The platform credential store | Windows Credential Manager, macOS Keychain                  | Yes              |
| The settings file             | No credential store, or one that refused                    | Yes, plain text  |

The environment variable is first because it is the only option that stores
nothing at all: a user who would rather Sutra never held their key has a way to
say so. It is also read-only — Sutra never sets one, so "unset it and the key is
gone" is true.

The settings file is last and is announced. On a platform with a credential
store that refuses, the user gets a warning naming the file, because a key
sitting in plain text is something a person should know about rather than
discover.

## What is not a place a key can live

The **vault** — never, under any of the three. The settings file is
`app_config_dir()/sutra.json` (`config_path`, `src-tauri/src/state.rs:611`) and
the search index is under `app_data_dir()` (`src-tauri/src/state.rs:298`); both
are OS application directories, outside the vault the user chose. That is the
whole reason for the separation: a vault is a folder people sync to OneDrive,
copy to a second laptop, zip up for a collaborator and back up to an external
disk. A credential must not be along for any of those rides.

This is verified by **inspection**, not by a test — the two paths above are the
only writers of a key, and neither takes a vault root. It is recorded here
because an inspected claim that nobody wrote down is an inspected claim that
stops being true.

## The three claims, and what holds each

| Claim                                                      | Held by                                                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Removing a key deletes the stored value, and the file copy | `removing_a_key_clears_the_plaintext_copy_too`, `an_empty_key_removes_the_stored_one`      |
| Blank input means remove, not store                        | `a_key_of_only_spaces_removes_rather_than_stores`                                          |
| Once the store takes a key, the plaintext copy is cleared  | `migration_only_clears_the_plaintext_after_the_store_took_it`                              |
| Nothing Sutra prints or reports repeats a key              | `a_warning_never_repeats_the_key`, `no_line_this_program_prints_interpolates_a_credential` |
| A key in the environment is used and never written down    | `the_environment_wins_and_stores_nothing`                                                  |
| The vault never contains a credential                      | inspection: `config_path` and the index path, above                                        |

`no_line_this_program_prints_interpolates_a_credential` is the unusual one: it
reads the crate's own source and fails if any `println!` or `eprintln!` line
mentions a key, secret, token or password. It exists because the failure it
guards against is not a design mistake but a human one — a debug line added
during an investigation and forgotten.

## The order that matters

Clearing a plaintext key happens **after** the credential store confirms it took
it, never before. The reverse order has a window in which the key exists
nowhere, and the user's next Zotero search fails with no way to explain why.
Held by `migration_only_clears_the_plaintext_after_the_store_took_it`.
