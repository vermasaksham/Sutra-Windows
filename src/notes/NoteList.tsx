import { useEffect, useMemo, useRef, useState } from "react";
import { linksAsTitles } from "../editor/wikilink/titleStore";
import { buildTree, type TreeNode } from "./tree";
import type { NoteSummary, SearchHit } from "../vault/api";

/**
 * The middle column: which note.
 *
 * Bear's list, with Sutra's model underneath. Bear's notes are flat and this
 * vault's are a tree — `parent` and `position` live in the frontmatter — so the
 * rows nest, and a row is Bear's: title, the opening line beneath it, a date
 * along the edge. The excerpt comes from Rust with the markdown markers already
 * stripped; when a search is running it is FTS5's snippet instead, with the
 * matched words marked.
 *
 * Searching flattens the list. There is no useful hierarchy in a result set,
 * and pretending otherwise would hide matches inside collapsed parents.
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
  depth: number;
  children: number;
};

type Props = {
  notes: NoteSummary[];
  hits: SearchHit[] | null;
  query: string;
  onQuery: (query: string) => void;
  /** Names what is being listed, e.g. "All notes" or a tag. */
  heading: string;
  /** False while a tag or a search has already narrowed the set: a result list
   *  has no useful hierarchy, and nesting it would hide rows inside collapsed
   *  parents that are not themselves in the set. */
  nested: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: (parent: string | null) => void;
  onDelete: (id: string) => void;
  /** Focusing the search field is a global shortcut, so App drives it. */
  focusSearch: number;
};

export default function NoteList({
  notes,
  hits,
  query,
  onQuery,
  heading,
  nested,
  selectedId,
  onSelect,
  onCreate,
  onDelete,
  focusSearch,
}: Props) {
  // Collapsed rather than expanded, so the default is everything open. A
  // research vault is mostly shallow, and hiding notes by default hides work.
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (focusSearch === 0) return;
    searchRef.current?.focus();
    searchRef.current?.select();
  }, [focusSearch]);

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const searching = hits !== null;
  const rows = useMemo(
    () =>
      hits
        ? hitRows(hits, notes)
        : nested
          ? treeRows(notes, collapsed)
          : flatRows(notes),
    [hits, notes, nested, collapsed],
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

      <p className="px-3.5 pb-1 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
        {searching ? `${rows.length} found` : heading}
      </p>

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
              nested={nested}
              collapsed={collapsed.has(row.id)}
              onToggle={() => toggle(row.id)}
              onSelect={() => onSelect(row.id)}
              onCreate={() => onCreate(row.id)}
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
  nested,
  collapsed,
  onToggle,
  onSelect,
  onCreate,
  onDelete,
}: {
  row: ListRow;
  active: boolean;
  nested: boolean;
  collapsed: boolean;
  onToggle: () => void;
  onSelect: () => void;
  onCreate: () => void;
  onDelete: () => void;
}) {
  return (
    <li
      className="group relative"
      // Indent by depth. Margin on the row rather than a nested <ul> keeps
      // every row the same height and the hit targets aligned.
      style={{ marginInlineStart: nested ? `${row.depth * 10}px` : undefined }}
    >
      <div
        className={[
          "flex items-start rounded-lg transition-colors duration-150 ease-out",
          active ? "bg-row-active" : "hover:bg-row-hover",
        ].join(" ")}
      >
        <button
          type="button"
          onClick={onToggle}
          aria-label={
            row.children > 0
              ? collapsed
                ? `Expand ${row.title}`
                : `Collapse ${row.title}`
              : undefined
          }
          aria-expanded={row.children > 0 ? !collapsed : undefined}
          tabIndex={row.children > 0 ? 0 : -1}
          className="grid size-5 shrink-0 place-items-center pt-2 text-ink-muted"
        >
          {nested && row.children > 0 && <Chevron open={!collapsed} />}
        </button>

        <button
          type="button"
          onClick={onSelect}
          className="min-w-0 flex-1 py-1.5 pr-1 text-left"
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
        </button>

        <span className="flex shrink-0 items-center pt-1.5 opacity-0 transition-opacity duration-150 ease-out group-hover:opacity-100 focus-within:opacity-100">
          {nested && (
            <button
              type="button"
              onClick={onCreate}
              aria-label={`New note inside ${row.title || "Untitled"}`}
              title="New nested note"
              className="grid size-5 place-items-center rounded text-ink-muted hover:text-accent"
            >
              <Plus />
            </button>
          )}
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

function treeRows(notes: NoteSummary[], collapsed: Set<string>): ListRow[] {
  const rows: ListRow[] = [];
  const flatten = (nodes: TreeNode[]) => {
    for (const node of nodes) {
      rows.push({
        id: node.id,
        title: node.title,
        icon: node.icon,
        excerpt: node.excerpt,
        marked: false,
        updated: node.updated,
        depth: node.depth,
        children: node.children.length,
      });
      if (!collapsed.has(node.id)) flatten(node.children);
    }
  };
  flatten(buildTree(notes));
  return rows;
}

/** Most recently touched first — the order a filtered set wants. */
function flatRows(notes: NoteSummary[]): ListRow[] {
  return [...notes]
    .sort((a, b) => b.updated.localeCompare(a.updated))
    .map((note) => ({
      id: note.id,
      title: note.title,
      icon: note.icon,
      excerpt: note.excerpt,
      marked: false,
      updated: note.updated,
      depth: 0,
      children: 0,
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
    depth: 0,
    children: 0,
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

function Plus() {
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
      <path d="M12 5v14M5 12h14" />
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

function Chevron({ open }: { open: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-3 transition-transform duration-150 ease-out"
      style={{ transform: open ? "rotate(90deg)" : "none" }}
      aria-hidden
    >
      <path d="M9 6l6 6-6 6" />
    </svg>
  );
}
