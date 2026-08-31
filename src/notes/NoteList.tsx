import { useEffect, useMemo, useRef } from "react";
import { linksAsTitles } from "../editor/wikilink/titleStore";
import type { NoteSummary, SearchHit } from "../vault/api";

/**
 * The middle column: which note.
 *
 * A flat list, always. The rail owns the folder tree, so by the time a note
 * reaches this column the question of where it lives has been answered — what
 * is left is choosing between the notes in that answer. Nesting them again here
 * would say the same thing twice and hide rows inside collapsed parents.
 *
 * A row is Bear's: title, the opening line beneath it, a date along the edge.
 * The excerpt comes from Rust with the markdown markers already stripped; when
 * a search is running it is FTS5's snippet instead, with the matched words
 * marked.
 */

export type ListRow = {
  id: string;
  title: string;
  icon: string | null;
  /** Plain text, unless `marked`. */
  excerpt: string;
  /** The excerpt carries FTS5's `<mark>` tags and needs the snippet renderer. */
  marked: boolean;
  updated: string | null;
  /** Shown when the list spans more than one folder, so a row says where it is. */
  folder: string;
};

type Props = {
  notes: NoteSummary[];
  hits: SearchHit[] | null;
  query: string;
  onQuery: (query: string) => void;
  /** Names what is being listed, e.g. "All notes" or a folder. */
  heading: string;
  /** True when rows can come from different folders, so each says where it is. */
  showFolders: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
  /** A new note in whatever this column is currently showing. */
  onCreate: () => void;
  /** Focusing the search field is a global shortcut, so App drives it. */
  focusSearch: number;
};

export default function NoteList({
  notes,
  hits,
  query,
  onQuery,
  heading,
  showFolders,
  selectedId,
  onSelect,
  onDelete,
  onCreate,
  focusSearch,
}: Props) {
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (focusSearch === 0) return;
    searchRef.current?.focus();
    searchRef.current?.select();
  }, [focusSearch]);

  const searching = hits !== null;
  const rows = useMemo(
    () => (hits ? hitRows(hits, notes) : listRows(notes)),
    [hits, notes],
  );

  return (
    <div className="flex h-full w-list shrink-0 flex-col border-r border-l border-border bg-canvas">
      <div className="px-2.5 pt-3 pb-2">
        <div className="flex items-center gap-1.5 rounded-lg bg-row-hover px-2 py-1.5 focus-within:ring-1 focus-within:ring-accent">
          <Magnifier />
          <input
            ref={searchRef}
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") onQuery("");
            }}
            placeholder="Search"
            aria-label="Search the vault"
            className="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-ink-muted"
          />
          {query !== "" && (
            <button
              type="button"
              onClick={() => onQuery("")}
              aria-label="Clear the search"
              className="shrink-0 text-ink-muted transition-colors duration-150 ease-out hover:text-ink"
            >
              <Cross />
            </button>
          )}
        </div>
      </div>

      <div className="flex items-center justify-between px-3.5 pb-1">
        <p className="min-w-0 truncate text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
          {searching ? `${rows.length} found` : heading}
        </p>
        {!searching && (
          <button
            type="button"
            onClick={onCreate}
            aria-label="New note here"
            title="New note here"
            className="grid size-4 shrink-0 place-items-center rounded text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
          >
            <Plus />
          </button>
        )}
      </div>

      {rows.length === 0 ? (
        <p className="px-3.5 py-2 text-sm text-ink-muted">
          {searching
            ? "No matches."
            : query !== ""
              ? "Searching…"
              : "Nothing here yet."}
        </p>
      ) : (
        <ul className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
          {rows.map((row) => (
            <Row
              key={row.id}
              row={row}
              active={row.id === selectedId}
              showFolder={showFolders}
              onSelect={() => onSelect(row.id)}
              onDelete={() => onDelete(row.id)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function Row({
  row,
  active,
  showFolder,
  onSelect,
  onDelete,
}: {
  row: ListRow;
  active: boolean;
  showFolder: boolean;
  onSelect: () => void;
  onDelete: () => void;
}) {
  return (
    <li className="group relative">
      <div
        className={[
          "flex items-start rounded-lg transition-colors duration-150 ease-out",
          active ? "bg-row-active" : "hover:bg-row-hover",
        ].join(" ")}
      >
        <button
          type="button"
          onClick={onSelect}
          className="min-w-0 flex-1 py-1.5 pr-1 pl-2.5 text-left"
        >
          <span className="flex items-baseline gap-1.5">
            <span
              className={[
                "min-w-0 flex-1 truncate text-sm",
                active ? "font-medium text-accent" : "text-ink",
              ].join(" ")}
            >
              {row.icon ? `${row.icon} ` : ""}
              {row.title || "Untitled"}
            </span>
            {row.updated && (
              <span className="shrink-0 text-[0.6875rem] tabular-nums text-ink-muted">
                {when(row.updated)}
              </span>
            )}
          </span>
          {row.excerpt !== "" &&
            (row.marked ? (
              // FTS5's snippet(), which wraps matched terms in <mark>. The text
              // is generated by SQLite from the note's own indexed body — not
              // user HTML routed back into the page — and everything but the
              // mark tags is escaped below.
              <span
                className="sutra-excerpt mt-0.5 block truncate text-xs text-ink-muted"
                dangerouslySetInnerHTML={{
                  __html: escapeExceptMark(linksAsTitles(row.excerpt)),
                }}
              />
            ) : (
              <span className="mt-0.5 block truncate text-xs text-ink-muted">
                {row.excerpt}
              </span>
            ))}
          {showFolder && row.folder !== "" && (
            <span className="mt-0.5 block truncate text-[0.6875rem] text-highlight">
              {row.folder}
            </span>
          )}
        </button>

        <span className="flex shrink-0 items-center pt-1.5 opacity-0 transition-opacity duration-150 ease-out group-hover:opacity-100 focus-within:opacity-100">
          <button
            type="button"
            onClick={onDelete}
            aria-label={`Move ${row.title || "Untitled"} to trash`}
            className="mr-1 grid size-5 place-items-center rounded text-ink-muted hover:text-accent"
          >
            <Trash />
          </button>
        </span>
      </div>
    </li>
  );
}

/** Rust already sorted the list; this only reshapes it into rows. */
function listRows(notes: NoteSummary[]): ListRow[] {
  return notes.map((note) => ({
    id: note.id,
    title: note.title,
    icon: note.icon,
    excerpt: note.excerpt,
    marked: false,
    updated: note.updated,
    folder: note.folder,
  }));
}

function hitRows(hits: SearchHit[], notes: NoteSummary[]): ListRow[] {
  const byId = new Map(notes.map((note) => [note.id, note]));
  return hits.map((hit) => ({
    id: hit.id,
    title: hit.title,
    icon: byId.get(hit.id)?.icon ?? null,
    excerpt: hit.excerpt,
    marked: true,
    updated: byId.get(hit.id)?.updated ?? null,
    folder: byId.get(hit.id)?.folder ?? "",
  }));
}

/**
 * A date the way a list wants one: short, and shorter the more recent it is.
 *
 * Undated rather than wrong if the frontmatter's timestamp will not parse —
 * these files are hand-editable, so it sometimes will not.
 */
function when(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const now = new Date();
  if (date.toDateString() === now.toDateString()) {
    return date.toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
    });
  }
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    year: date.getFullYear() === now.getFullYear() ? undefined : "2-digit",
  });
}

/**
 * Escape the excerpt, then restore only the `<mark>` tags FTS5 added.
 *
 * Note bodies are arbitrary text and may contain angle brackets — a note about
 * `<script>` tags must not be able to inject one by being searched for.
 */
export function escapeExceptMark(excerpt: string): string {
  return excerpt
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/&lt;mark&gt;/g, "<mark>")
    .replace(/&lt;\/mark&gt;/g, "</mark>");
}

function Magnifier() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      className="size-3.5 shrink-0 text-ink-muted"
      aria-hidden
    >
      <circle cx="11" cy="11" r="6" />
      <path d="m20 20-4.5-4.5" />
    </svg>
  );
}

function Cross() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      className="size-3.5"
      aria-hidden
    >
      <path d="M6 6l12 12M18 6L6 18" />
    </svg>
  );
}

function Trash() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      className="size-3.5"
      aria-hidden
    >
      <path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13" />
    </svg>
  );
}

function Plus() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      className="size-3"
      aria-hidden
    >
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}
