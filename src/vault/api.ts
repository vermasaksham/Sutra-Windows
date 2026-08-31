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

/** The nine kinds a note can be. Never asked for before writing. */
export type NoteType =
  | "standard"
  | "literature"
  | "idea"
  | "question"
  | "experiment"
  | "project"
  | "meeting"
  | "task"
  | "daily";

export const NOTE_TYPES: ReadonlyArray<{ value: NoteType; label: string }> = [
  { value: "standard", label: "Note" },
  { value: "literature", label: "Literature" },
  { value: "idea", label: "Idea" },
  { value: "question", label: "Research question" },
  { value: "experiment", label: "Experiment" },
  { value: "project", label: "Project" },
  { value: "meeting", label: "Meeting" },
  { value: "task", label: "Task" },
  { value: "daily", label: "Daily" },
];

export type NoteSummary = {
  id: string;
  type: NoteType;
  title: string;
  /** Vault-relative directory, `/`-separated. "" is the root. */
  folder: string;
  position: number;
  tags: string[];
  icon: string | null;
  cover: string | null;
  /** The opening prose, markers stripped, for the list to show. */
  excerpt: string;
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
  create: (title: string, folder: string | null = null) =>
    invoke<NoteDoc>("create_note", { title, folder }),
  /** Move a note into another folder. Nothing that links to it changes. */
  move: (id: string, folder: string) =>
    invoke<NoteSummary>("move_note", { id, folder }),
  /** A new, empty, untitled note in the Inbox. No decisions required. */
  capture: () => invoke<NoteDoc>("capture"),
  setType: (id: string, noteType: NoteType) =>
    invoke<NoteSummary>("set_note_type", { id, noteType }),
  save: (id: string, title: string, body: string) =>
    invoke<NoteSummary>("save_note", { id, title, body }),
  remove: (id: string) => invoke<void>("delete_note", { id }),
  /** Replace a note's page-level metadata. Send the complete desired state. */
  setMeta: (
    id: string,
    icon: string | null,
    cover: string | null,
    tags: string[],
  ) => invoke<NoteSummary>("set_note_meta", { id, icon, cover, tags }),
  /** Opens the native file picker and copies the result into the vault,
   *  beside the note that will reference it. Resolves the vault-relative
   *  reference to put in the markdown. */
  attach: (folder: string | null = null) =>
    invoke<string | null>("attach_file", { folder }),
};

/** What reorganising a flat vault into folders would do. */
export type MigrationPlan = {
  /** `[from, to]`, vault-relative. */
  moves: Array<[string, string]>;
  /** Notes whose chain of parents was deeper than folders go. */
  flattened: string[];
  /** Files whose frontmatter would not parse. Left exactly where they are. */
  skipped: string[];
};

export const migrationApi = {
  needed: () => invoke<boolean>("migration_needed"),
  plan: () => invoke<MigrationPlan>("migration_plan"),
  /** Copies every note first. Resolves the number of files moved. */
  run: () => invoke<number>("migrate_vault"),
};

/** Two tags that look like they were meant to be one. Offered, never applied. */
export type TagSuggestion = {
  from: string;
  fromCount: number;
  into: string;
  intoCount: number;
  /** Shown verbatim, so the reader can judge rather than trust. */
  reason: string;
};

/** One note's tags before a retag, which is what makes the retag undoable. */
export type TagChange = { id: string; previous: string[] };
export type Retag = { changed: TagChange[] };

export const tagsApi = {
  /** Every tag as written, with how many notes carry it. */
  list: () => invoke<Record<string, number>>("list_tags"),
  similar: () => invoke<TagSuggestion[]>("similar_tags"),
  /** Rename across the vault, or merge into an existing tag. Same operation. */
  retag: (from: string, to: string) => invoke<Retag>("retag", { from, to }),
  undo: (changed: TagChange[]) => invoke<number>("undo_retag", { changed }),
};

export const foldersApi = {
  list: () => invoke<string[]>("list_folders"),
  /** Create a folder, and any missing parents. Resolves its normalised path. */
  create: (folder: string) => invoke<string>("create_folder", { folder }),
};

export type SearchHit = { id: string; title: string; excerpt: string };
export type Backlink = { id: string; title: string; excerpt: string };

export const indexApi = {
  search: (query: string) => invoke<SearchHit[]>("search_notes", { query }),
  backlinks: (id: string) => invoke<Backlink[]>("backlinks", { id }),
  /**
   * Throw the index away and rebuild it from the markdown files. Always safe,
   * by design. Not yet reachable from the UI — it belongs with the Phase 6
   * error handling, as the answer to "search looks wrong".
   */
  reindex: () => invoke<number>("reindex"),
};

export type Reference = {
  key: string;
  title: string;
  creators: string;
  year: string | null;
  itemType: string;
  doi: string | null;
};

export const exportApi = {
  /** Write the note as .docx. Opens a save dialog in Rust; resolves the chosen
   *  file's name, or null if cancelled. */
  docx: (document: unknown) =>
    invoke<string | null>("export_docx", { document }),
};

export const zoteroApi = {
  /** Search the running Zotero. Rejects with a sentence worth showing if it
   *  is not running or the local API is switched off. */
  search: (query: string) => invoke<Reference[]>("zotero_search", { query }),
  byKeys: (keys: string[]) => invoke<Reference[]>("zotero_by_keys", { keys }),
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
