import { useEffect, useMemo, useRef, useState } from "react";
import { linksAsTitles } from "../editor/wikilink/titleStore";
import {
  zoteroApi,
  type NoteSummary,
  type Reference,
  type SearchHit,
} from "../vault/api";

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
  /** Make a literature note from a reference-manager item. */
  onLiteratureNote: (key: string) => void;
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
  /**
   * Set when the column is showing a saved view rather than a place.
   *
   * A view has no location, so there is nothing to create a note "in" — the
   * new-note button becomes the way to change the query instead, which is the
   * only thing that changes what is listed here.
   */
  view?: {
    /** The query in English, from Rust, so there is one rendering of it. */
    description: string;
    truncated: boolean;
    ignored: number;
    onEdit: () => void;
  } | null;
};

/** Rows added per step. Enough that scrolling rarely reaches the end. */
const PAGE = 200;

export default function NoteList({
  notes,
  hits,
  query,
  onLiteratureNote,
  onQuery,
  heading,
  showFolders,
  selectedId,
  onSelect,
  onDelete,
  onCreate,
  focusSearch,
  view,
}: Props) {
  const searchRef = useRef<HTMLInputElement>(null);

  // How many rows are in the DOM. A vault reaches thousands of notes over a
  // PhD, and rendering every row cost about twelve DOM nodes each — sixty
  // thousand nodes for five thousand notes, and several seconds before the
  // window appeared at all.
  //
  // Grown on scroll rather than windowed by height, deliberately. Rows are not
  // a uniform height — a note may or may not have an excerpt or a folder line —
  // and height-based virtualisation with a guessed row height puts rows in the
  // wrong place, which is a worse failure than rendering fewer of them.
  const [shown, setShown] = useState(PAGE);
  const sentinel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (focusSearch === 0) return;
    searchRef.current?.focus();
    searchRef.current?.select();
  }, [focusSearch]);

  const searching = hits !== null;

  // The library is searched alongside the vault, because "have I read anything
  // about this?" and "have I written anything about this?" are the same
  // question asked twice, and answering only the second is how a paper gets
  // read for a third time. Debounced and guarded like every other live search;
  // a closed Zotero simply yields nothing and is not an error here.
  const [papers, setPapers] = useState<Reference[]>([]);
  useEffect(() => {
    const text = query.trim();
    if (text === "") {
      setPapers([]);
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      zoteroApi
        .search(text)
        .then((found) => live && setPapers(found.slice(0, 5)))
        .catch(() => live && setPapers([]));
    }, 250);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [query]);
  const rows = useMemo(
    () => (hits ? hitRows(hits, notes) : listRows(notes)),
    [hits, notes],
  );

  // Back to the first page whenever this becomes a different list — a new
  // search, another folder, another tag. Without it, narrowing a search would
  // keep however many pages the previous list had grown to.
  const firstId = rows[0]?.id;
  useEffect(() => setShown(PAGE), [rows.length, firstId]);

  useEffect(() => {
    const target = sentinel.current;
    if (!target) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        setShown((n) => (n >= rows.length ? n : n + PAGE));
      }
    });
    observer.observe(target);
    return () => observer.disconnect();
  }, [rows.length]);

  const visible = rows.length > shown ? rows.slice(0, shown) : rows;

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
              className="shrink-0 text-ink-muted transition-colors hover:text-ink"
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
        {!searching &&
          (view ? (
            <button
              type="button"
              onClick={view.onEdit}
              className="shrink-0 text-[0.6875rem] text-ink-muted transition-colors hover:text-accent"
            >
              Edit query
            </button>
          ) : (
            <button
              type="button"
              onClick={onCreate}
              aria-label="New note here"
              title="New note here"
              className="grid size-4 shrink-0 place-items-center rounded text-ink-muted transition-colors hover:text-accent"
            >
              <Plus />
            </button>
          ))}
      </div>

      {view && !searching && (
        <div className="px-3.5 pb-2">
          <p className="text-xs text-ink-muted">{view.description}</p>
          {view.truncated && (
            <p className="pt-1 text-xs text-highlight">
              Stopped at the limit — there may be more. Narrow the query, or
              raise the limit.
            </p>
          )}
          {view.ignored > 0 && (
            <p className="pt-1 text-xs text-highlight">
              {view.ignored === 1
                ? "One condition in this view was written by a newer Sutra and could not be applied, so these results are wider than the query asks for. The file still holds it."
                : `${view.ignored} conditions in this view were written by a newer Sutra and could not be applied, so these results are wider than the query asks for. The file still holds them.`}
            </p>
          )}
        </div>
      )}

      {rows.length === 0 && papers.length > 0 ? (
        <p className="px-3.5 py-2 text-sm text-ink-muted">
          Nothing written about this yet — but your library has something.
        </p>
      ) : rows.length === 0 ? (
        <p className="px-3.5 py-2 text-sm text-ink-muted">
          {searching
            ? "No matches."
            : query !== ""
              ? "Searching…"
              : view
                ? "Nothing matches this view."
                : "Nothing here yet."}
        </p>
      ) : null}

      <div className="min-h-0 flex-1 overflow-y-auto pb-2">
        {rows.length > 0 && (
          <>
            {/* Only labelled when there is something to tell it apart from.
                A heading over the only group on screen is noise. */}
            {papers.length > 0 && <GroupLabel>Notes</GroupLabel>}
            <ul className="px-1.5">
              {visible.map((row) => (
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
            {/* Crossing this asks for the next page. It sits inside the scroll
                container, so the container's own scrolling drives it and no
                scroll listener is needed. */}
            {visible.length < rows.length && (
              <div ref={sentinel} aria-hidden className="h-8" />
            )}
          </>
        )}

        {papers.length > 0 && (
          <>
            <GroupLabel>In your Zotero library</GroupLabel>
            <ul className="px-1.5">
              {papers.map((paper) => (
                <PaperRow
                  key={paper.key}
                  paper={paper}
                  onLiteratureNote={() => onLiteratureNote(paper.key)}
                />
              ))}
            </ul>
          </>
        )}
      </div>
    </div>
  );
}

function GroupLabel({ children }: { children: React.ReactNode }) {
  return (
    <p className="px-3.5 pt-2 pb-1 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
      {children}
    </p>
  );
}

/**
 * A paper from the library, in the note list but never disguised as a note.
 *
 * Section 18 asks that source results be clearly distinguished from notes, and
 * the distinction here is structural rather than decorative: a different group
 * with its own heading, a different border, and an action that says what it
 * would create. Nothing about this row can be mistaken for something already
 * written — because nothing has been.
 */
function PaperRow({
  paper,
  onLiteratureNote,
}: {
  paper: Reference;
  onLiteratureNote: () => void;
}) {
  const detail = [paper.creators, paper.year, paper.container]
    .map((part) => part?.trim())
    .filter((part): part is string => !!part)
    .join(" · ");

  return (
    <li className="mb-1 rounded-lg border border-dashed border-highlight/50 px-2.5 py-2">
      <p className="text-sm text-ink">{paper.title}</p>
      <p className="mt-0.5 truncate text-xs text-ink-muted">
        {detail || "No author or year in Zotero"}
      </p>
      <button
        type="button"
        onClick={onLiteratureNote}
        className="mt-1.5 text-xs text-highlight transition-opacity hover:opacity-80"
      >
        Read it into a literature note →
      </button>
    </li>
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
          "flex items-start rounded-lg transition-colors",
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

        <span className="flex shrink-0 items-center pt-1.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
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
