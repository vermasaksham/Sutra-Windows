import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * The entire surface Rust exposes.
 *
 * Notice what is absent: paths. Notes are addressed by id, the vault is a
 * display name, and the folder pickers open on the Rust side — so no filesystem
 * path ever exists in this half of the app.
 */

export type VaultInfo = { name: string };

export type NoteSummary = {
  id: string;
  title: string;
  parent: string | null;
  position: number;
  tags: string[];
  icon: string | null;
  /** RFC3339, e.g. 2026-08-21T11:02:00Z */
  updated: string;
};

/** A note plus its markdown body. `adopted` marks a file that had no
 *  frontmatter and was taken over on read. */
export type NoteDoc = NoteSummary & { body: string; adopted: boolean };

export const vaultApi = {
  /** Opens the native folder picker. Resolves null if the user cancelled. */
  pick: () => invoke<VaultInfo | null>("pick_vault"),
  /** The vault restored from the last session, if any. */
  current: () => invoke<VaultInfo | null>("current_vault"),
};

export const notesApi = {
  list: () => invoke<NoteSummary[]>("list_notes"),
  read: (id: string) => invoke<NoteDoc>("read_note", { id }),
  create: (title: string, parent: string | null = null) =>
    invoke<NoteDoc>("create_note", { title, parent }),
  save: (id: string, title: string, body: string) =>
    invoke<NoteSummary>("save_note", { id, title, body }),
  remove: (id: string) => invoke<void>("delete_note", { id }),
  /** Opens the native file picker and copies the result into the vault.
   *  Resolves the vault-relative reference to put in the markdown. */
  attach: () => invoke<string | null>("attach_file"),
};

export type VaultChanged = { changed: string[] };

/**
 * Subscribe to changes made to the vault from outside the app.
 *
 * Rust reports ids only; deciding what to do with each is this side's job.
 * Returns a promise of the unsubscribe function, or a no-op when there is no
 * Tauri host (i.e. `npm run dev` in a plain browser).
 */
export function onVaultChanged(
  handler: (changed: string[]) => void,
): Promise<UnlistenFn> {
  return listen<VaultChanged>("vault:changed", (event) =>
    handler(event.payload.changed),
  ).catch(() => () => {});
}
