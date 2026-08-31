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

/** The kinds a note can be. Never asked for before writing. */
export type NoteType =
  | "standard"
  | "literature"
  | "idea"
  | "question"
  | "experiment"
  | "project"
  | "meeting"
  | "task"
  | "daily"
  | "source"
  | "view";

/**
 * What each kind is called, including the ones the picker does not offer.
 */
export const TYPE_LABELS: Record<NoteType, string> = {
  standard: "Note",
  literature: "Literature",
  idea: "Idea",
  question: "Research question",
  experiment: "Experiment",
  project: "Project",
  meeting: "Meeting",
  task: "Task",
  daily: "Daily",
  source: "Source",
  view: "View",
};

/**
 * The kinds the type picker offers, in order.
 *
 * `view` is missing on purpose. Every other kind is a way of describing a note
 * someone has written; a view is a saved query, and a note turned into one by
 * a dropdown would be a view with nothing to run. Views are made by saving a
 * query, which is the only way to get one that means anything.
 */
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
  { value: "source", label: "Source" },
];

/** What a source note records about the paper it stands for. */
export type SourceMeta = {
  authors?: string | null;
  year?: string | null;
  /** Journal, book or proceedings. */
  container?: string | null;
  doi?: string | null;
  url?: string | null;
  /** The Zotero item key it was imported from, so re-import updates it. */
  zotero?: string | null;
};

/**
 * One note citing one source, at one place in it.
 *
 * Lives in the citing note's own frontmatter, so it survives being copied to
 * another machine or read with none of this software installed.
 */
export type Citation = {
  /** The source note's ULID. Not a Zotero key. */
  id: string;
  /** A string: "S12", "6-8" and "iv" are all real page references. */
  page?: string | null;
  quote?: string | null;
  /** RFC3339. */
  captured?: string | null;
};

/** A note that cites a source, and where in it. */
export type CitingNote = { id: string; title: string; page: string | null };

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
  /** Present on a note of `type: source`. */
  source?: SourceMeta;
  /** The sources this note draws on. Absent when it draws on none. */
  sources?: Citation[];
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

export const sourcesApi = {
  /** Every source note in the vault. */
  list: () => invoke<NoteSummary[]>("list_sources"),
  create: (title: string, meta: SourceMeta) =>
    invoke<NoteDoc>("create_source", { title, meta }),
  setMeta: (id: string, meta: SourceMeta) =>
    invoke<NoteSummary>("set_source_meta", { id, meta }),
  /** Replace a note's citations. Send the complete desired list. */
  setCitations: (id: string, citations: Citation[]) =>
    invoke<NoteSummary>("set_citations", { id, citations }),
  /** Which notes cite this source, and where in it. */
  citing: (id: string) => invoke<CitingNote[]>("citing_notes", { id }),
  /** Copy a Zotero item into the vault as a source note. Updates rather than
   *  duplicates if that item is already here. */
  importZotero: (key: string) =>
    invoke<NoteSummary>("import_zotero_source", { key }),
};

/** What migrating the legacy citations did. */
export type CitationMigration = {
  /** Zotero key, and the source note it now points at. */
  migrated: Array<[string, string]>;
  /** Keys Zotero could not answer for. Left exactly as they were. */
  unresolved: string[];
  notesChanged: number;
};

export const legacyCitationsApi = {
  /** Zotero keys still in note bodies, with how many notes use each. */
  find: () => invoke<Record<string, number>>("legacy_citations"),
  /** Needs Zotero, once. After this the vault does not need it again. */
  migrate: () => invoke<CitationMigration>("migrate_citations"),
};

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
  /** Journal, book or proceedings — whatever it appeared in. */
  container: string | null;
  url: string | null;
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

// ---- saved views -------------------------------------------------------------

/**
 * One thing a note must, or must not, be.
 *
 * A union of single-key objects, mirroring the YAML a view note holds. Typed
 * rather than a query string on purpose: the UI can offer a real form, every
 * term compiles to something the index can answer, and there is no syntax for
 * anyone to get wrong.
 */
export type Condition =
  /** In exactly this folder. `""` is the top level of the vault. */
  | { in: string }
  /** In this folder or any folder beneath it. */
  | { under: string }
  /** Carries this tag or any tag beneath it: `method` finds `method/xrd`. */
  | { tag: string }
  | { type: NoteType }
  /** Cites this source note. */
  | { cites: string }
  /** Contains a wikilink to this note. */
  | { "links-to": string }
  /** Matches this full-text query. */
  | { text: string }
  /** Edited on or after this `YYYY-MM-DD`. */
  | { "updated-after": string }
  /** Edited before this `YYYY-MM-DD`. */
  | { "updated-before": string };

/**
 * The key a condition is written under — the thing that says which it is.
 *
 * `keyof` over a union gives the keys they *share*, which here is none. The
 * conditional distributes over the members first, so this is the union of
 * every key instead.
 */
type KeyOfEach<T> = T extends unknown ? keyof T : never;
export type ConditionKind = KeyOfEach<Condition> & string;

/** Every kind, in the order the editor offers them. */
export const CONDITION_KINDS: ReadonlyArray<{
  kind: ConditionKind;
  label: string;
}> = [
  { kind: "under", label: "In folder (and below)" },
  { kind: "in", label: "In folder (exactly)" },
  { kind: "tag", label: "Tagged" },
  { kind: "type", label: "Of type" },
  { kind: "text", label: "Mentioning" },
  { kind: "cites", label: "Cites source" },
  { kind: "links-to", label: "Links to note" },
  { kind: "updated-after", label: "Edited since" },
  { kind: "updated-before", label: "Untouched since" },
];

export type ViewSort = "recent" | "stale" | "title" | "folder";

export const VIEW_SORTS: ReadonlyArray<{ value: ViewSort; label: string }> = [
  { value: "recent", label: "Recently edited first" },
  { value: "stale", label: "Least recently edited first" },
  { value: "title", label: "By title" },
  { value: "folder", label: "By folder" },
];

/**
 * A saved query, exactly as it sits in a view note's frontmatter.
 *
 * Terms Rust could not read come back as-is and are passed back unchanged on
 * save, so a view written by a newer build is never quietly edited by an older
 * one. That is why the arrays are `unknown[]` at the edges and narrowed where
 * they are used.
 */
export type ViewQuery = {
  /** All of these must hold. */
  all?: Condition[];
  /** At least one of these must hold. Empty means no such requirement. */
  any?: Condition[];
  /** None of these may hold. */
  none?: Condition[];
  sort?: ViewSort;
  limit?: number;
};

export type ViewResult = {
  notes: NoteSummary[];
  /** The query in English, for the header above the results. */
  description: string;
  /** The limit was reached, so there may be more. */
  truncated: boolean;
  /** Terms this build could not read and left out of the results. */
  ignored: number;
};

export const viewsApi = {
  /** Every view note in the vault. */
  list: () => invoke<NoteSummary[]>("list_views"),
  /** The query a view note holds, or null if it holds none. */
  read: (id: string) => invoke<ViewQuery | null>("read_view", { id }),
  /**
   * Run a query. Takes the query rather than a view's id, so a query being
   * edited previews live and evaluating one touches no file at all.
   */
  run: (query: ViewQuery) => invoke<ViewResult>("run_view", { query }),
  create: (title: string, query: ViewQuery) =>
    invoke<NoteDoc>("create_view", { title, query }),
  save: (id: string, query: ViewQuery) =>
    invoke<NoteSummary>("save_view", { id, query }),
};

/** Which kind of condition this is, and the value it carries. */
export function conditionKind(condition: Condition): ConditionKind {
  return Object.keys(condition)[0] as ConditionKind;
}

export function conditionValue(condition: Condition): string {
  return String(Object.values(condition)[0] ?? "");
}

/** A condition of `kind` carrying `value`. */
export function condition(kind: ConditionKind, value: string): Condition {
  return { [kind]: value } as Condition;
}

// ---- the context panel -------------------------------------------------------

/** A note near the open one, and the line saying why. */
export type RelatedNote = {
  id: string;
  title: string;
  folder: string;
  /**
   * Why this is here, in one lowercase fragment: "cites Zhou 2019 too",
   * "shares #sb2se3 and 4 distinctive words".
   *
   * Written in Rust from the same reasons that produced the ranking, so the
   * explanation cannot drift from the thing it explains. Never empty.
   */
  reason: string;
  /** The sum of those reasons. Not shown; useful when tuning. */
  score: number;
};

export const contextApi = {
  /**
   * Notes near this one.
   *
   * The body is sent rather than re-read in Rust, so the panel reflects what
   * is on screen — including edits autosave has not written yet. Asking what
   * is near the open note must not depend on whether it has been saved.
   */
  related: (id: string, body: string, limit = 5) =>
    invoke<RelatedNote[]>("related_notes", { id, body, limit }),
  /** The other notes in this note's folder. */
  siblings: (id: string, limit = 8) =>
    invoke<NoteSummary[]>("folder_neighbours", { id, limit }),
};
