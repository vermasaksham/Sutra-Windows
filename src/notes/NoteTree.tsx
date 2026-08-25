import { useState } from "react";
import type { NoteSummary } from "../vault/api";
import { buildTree, type TreeNode } from "./tree";

type Props = {
  notes: NoteSummary[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: (parent: string | null) => void;
  onDelete: (id: string) => void;
};

export default function NoteTree({
  notes,
  selectedId,
  onSelect,
  onCreate,
  onDelete,
}: Props) {
  // Collapsed rather than expanded, so the default is everything open. A
  // research vault is mostly shallow, and hiding notes by default hides work.
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const rows: TreeNode[] = [];
  const flatten = (nodes: TreeNode[]) => {
    for (const node of nodes) {
      rows.push(node);
      if (!collapsed.has(node.id)) flatten(node.children);
    }
  };
  flatten(buildTree(notes));

  return (
    <nav className="flex h-full w-64 shrink-0 flex-col border-r border-border">
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-xs font-semibold tracking-wide text-ink-muted uppercase">
          Notes
        </span>
        <button
          type="button"
          onClick={() => onCreate(null)}
          aria-label="New note"
          title="New note"
          className="grid size-6 place-items-center rounded-md text-ink-soft transition-colors duration-150 ease-out hover:bg-accent-bg hover:text-accent"
        >
          <Plus />
        </button>
      </div>

      {rows.length === 0 ? (
        <p className="px-3 py-2 text-sm text-ink-muted">
          No notes yet. Create one to begin.
        </p>
      ) : (
        <ul className="flex-1 overflow-y-auto px-1.5 pb-2">
          {rows.map((node) => {
            const active = node.id === selectedId;
            const hasChildren = node.children.length > 0;
            const isCollapsed = collapsed.has(node.id);
            return (
              <li key={node.id} className="group relative">
                <div
                  className={[
                    "flex items-center rounded-md transition-colors duration-150 ease-out",
                    active ? "bg-accent-bg" : "hover:bg-surface",
                  ].join(" ")}
                  // Indent by depth. Padding on the row rather than a nested
                  // <ul> keeps every row the same height and the hit targets
                  // aligned.
                  style={{ paddingInlineStart: `${node.depth * 12}px` }}
                >
                  <button
                    type="button"
                    onClick={() => hasChildren && toggle(node.id)}
                    aria-label={
                      hasChildren
                        ? isCollapsed
                          ? `Expand ${node.title}`
                          : `Collapse ${node.title}`
                        : undefined
                    }
                    aria-expanded={hasChildren ? !isCollapsed : undefined}
                    tabIndex={hasChildren ? 0 : -1}
                    className="grid size-5 shrink-0 place-items-center rounded text-ink-muted"
                  >
                    {hasChildren && <Chevron open={!isCollapsed} />}
                  </button>

                  <button
                    type="button"
                    onClick={() => onSelect(node.id)}
                    className={[
                      "min-w-0 flex-1 truncate py-1.5 pr-1 text-left text-sm",
                      active ? "text-accent" : "text-ink-soft hover:text-ink",
                    ].join(" ")}
                  >
                    {node.icon ? `${node.icon} ` : ""}
                    {node.title || "Untitled"}
                  </button>

                  <span className="flex shrink-0 items-center opacity-0 transition-opacity duration-150 ease-out group-hover:opacity-100 focus-within:opacity-100">
                    <button
                      type="button"
                      onClick={() => onCreate(node.id)}
                      aria-label={`New note inside ${node.title || "Untitled"}`}
                      title="New nested note"
                      className="grid size-5 place-items-center rounded text-ink-muted hover:text-accent"
                    >
                      <Plus small />
                    </button>
                    <button
                      type="button"
                      onClick={() => onDelete(node.id)}
                      aria-label={`Move ${node.title || "Untitled"} to trash`}
                      className="mr-1 grid size-5 place-items-center rounded text-ink-muted hover:text-highlight"
                    >
                      <Trash />
                    </button>
                  </span>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </nav>
  );
}

function Plus({ small }: { small?: boolean } = {}) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      className={small ? "size-3.5" : "size-4"}
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
