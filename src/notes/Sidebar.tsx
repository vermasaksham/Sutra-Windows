import { useState } from "react";
import { MOD, shortcut } from "../platform";
import ThemeToggle from "../components/ThemeToggle";
import { buildFolders, flattenFolders, type FolderNode } from "./tree";
import { buildTagTree, flattenTags, type TagNode } from "./tags";
import type { NoteSummary } from "../vault/api";

/**
 * The left rail: what to look at, rather than which note.
 *
 * Separate groups, deliberately not one hierarchy — a folder is where a note
 * lives, a tag is what it is about, a view is a question asked of the whole
 * vault, and none of the three may be presented as the same kind of thing.
 * A note sits in exactly one folder, carries any number of tags, and appears
 * in a view without belonging to it at all.
 *
 * The Inbox is pinned above the folder tree even though it is an ordinary
 * directory. It is where captures land, so it is the one folder that has a job
 * rather than a meaning, and burying it among the others would make capture
 * cost a decision again.
 */
export default function Sidebar({
  vaultName,
  notes,
  folders,
  views,
  activeFolder,
  activeTag,
  activeView,
  onSelectFolder,
  onSelectTag,
  onSelectView,
  onCapture,
  onNewFolder,
  onNewView,
  onManageTags,
}: {
  vaultName: string;
  notes: NoteSummary[];
  folders: string[];
  views: NoteSummary[];
  activeFolder: string | null;
  activeTag: string | null;
  activeView: string | null;
  onSelectFolder: (folder: string | null) => void;
  onSelectTag: (tag: string | null) => void;
  onSelectView: (id: string | null) => void;
  onCapture: () => void;
  onNewFolder: (parent: string | null) => void;
  onNewView: () => void;
  onManageTags: () => void;
}) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [tagsCollapsed, setTagsCollapsed] = useState<Set<string>>(new Set());
  // The Inbox is pinned above, so it must not also appear in the tree.
  const tree = buildFolders(
    folders.filter((f) => f !== INBOX && !f.startsWith(`${INBOX}/`)),
    notes.filter(
      (n) => n.folder !== INBOX && !n.folder.startsWith(`${INBOX}/`),
    ),
  );
  const inboxCount = notes.filter(
    (n) => n.folder === INBOX || n.folder.startsWith(`${INBOX}/`),
  ).length;
  const rows = flattenFolders(tree, collapsed);
  const tagRows = flattenTags(buildTagTree(notes), tagsCollapsed);

  const toggler =
    (set: (fn: (prev: Set<string>) => Set<string>) => void) => (path: string) =>
      set((prev) => {
        const next = new Set(prev);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
  const toggle = toggler(setCollapsed);
  const toggleTag = toggler(setTagsCollapsed);

  return (
    <nav
      aria-label="Collections"
      className="flex h-full w-rail shrink-0 flex-col bg-rail"
    >
      <div className="flex items-center gap-2 px-3 pt-3 pb-2">
        <Star />
        <span className="min-w-0 flex-1 truncate text-sm font-semibold text-ink">
          {vaultName}
        </span>
        <button
          type="button"
          onClick={onCapture}
          aria-label="Capture a note to the Inbox"
          title={`Capture to Inbox (${shortcut(MOD, "N")})`}
          className="grid size-6 shrink-0 place-items-center rounded-md text-ink-muted transition-colors duration-150 ease-out hover:bg-row-hover hover:text-accent"
        >
          <Plus />
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto pb-2">
        <div className="px-1.5">
          <Row
            label="All notes"
            count={notes.length}
            active={activeFolder === null && activeTag === null && !activeView}
            onClick={() => onSelectFolder(null)}
          />
          <Row
            label="Inbox"
            count={inboxCount}
            emphasis={inboxCount > 0}
            active={activeFolder === INBOX && activeTag === null && !activeView}
            onClick={() => onSelectFolder(INBOX)}
          />
        </div>

        <div className="flex items-center justify-between px-3 pt-4 pb-1">
          <p className="text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
            Folders
          </p>
          <button
            type="button"
            onClick={() => onNewFolder(activeFolder)}
            aria-label="New folder"
            title="New folder"
            className="grid size-4 place-items-center rounded text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
          >
            <Plus small />
          </button>
        </div>

        {rows.length === 0 ? (
          <p className="px-3 pb-1 text-xs text-ink-muted">
            No folders yet. Notes live at the top level.
          </p>
        ) : (
          <ul aria-label="Folders" className="px-1.5">
            {rows.map((node) => (
              <li key={node.path}>
                <FolderRow
                  node={node}
                  active={
                    activeFolder === node.path &&
                    activeTag === null &&
                    !activeView
                  }
                  collapsed={collapsed.has(node.path)}
                  onToggle={() => toggle(node.path)}
                  onSelect={() => onSelectFolder(node.path)}
                />
              </li>
            ))}
          </ul>
        )}

        <div className="flex items-center justify-between px-3 pt-4 pb-1">
          <p className="text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
            Views
          </p>
          <button
            type="button"
            onClick={onNewView}
            aria-label="New view"
            title="New view"
            className="grid size-4 place-items-center rounded text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
          >
            <Plus small />
          </button>
        </div>

        {views.length === 0 ? (
          <p className="px-3 pb-1 text-xs text-ink-muted">
            A saved question about the vault, run fresh every time.
          </p>
        ) : (
          <ul aria-label="Views" className="px-1.5">
            {views.map((view) => (
              <li key={view.id}>
                <button
                  type="button"
                  onClick={() =>
                    onSelectView(activeView === view.id ? null : view.id)
                  }
                  aria-current={activeView === view.id ? "true" : undefined}
                  className={[
                    "flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-sm transition-colors duration-150 ease-out",
                    activeView === view.id
                      ? "bg-row-active font-medium text-accent"
                      : "text-ink-soft hover:bg-row-hover hover:text-ink",
                  ].join(" ")}
                >
                  <span className="shrink-0 text-ink-muted" aria-hidden>
                    {view.icon ?? <Lens />}
                  </span>
                  <span className="min-w-0 flex-1 truncate">{view.title}</span>
                </button>
              </li>
            ))}
          </ul>
        )}

        {tagRows.length > 0 && (
          <>
            <div className="flex items-center justify-between px-3 pt-4 pb-1">
              <p className="text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
                Tags
              </p>
              <button
                type="button"
                onClick={onManageTags}
                aria-label="Manage tags"
                title="Rename, merge and tidy tags"
                className="text-[0.6875rem] text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
              >
                Manage
              </button>
            </div>
            <ul aria-label="Tags" className="px-1.5">
              {tagRows.map((node) => (
                <li key={node.path}>
                  <TagRow
                    node={node}
                    active={activeTag === node.path && !activeView}
                    collapsed={tagsCollapsed.has(node.path)}
                    onToggle={() => toggleTag(node.path)}
                    onSelect={() =>
                      onSelectTag(activeTag === node.path ? null : node.path)
                    }
                  />
                </li>
              ))}
            </ul>
          </>
        )}
      </div>

      <div className="p-2">
        <ThemeToggle />
      </div>
    </nav>
  );
}

function FolderRow({
  node,
  active,
  collapsed,
  onToggle,
  onSelect,
}: {
  node: FolderNode;
  active: boolean;
  collapsed: boolean;
  onToggle: () => void;
  onSelect: () => void;
}) {
  const hasChildren = node.children.length > 0;
  return (
    <div
      className={[
        "flex items-center rounded-md transition-colors duration-150 ease-out",
        active
          ? "bg-row-active text-accent"
          : "text-ink-soft hover:bg-row-hover hover:text-ink",
      ].join(" ")}
      // Indent by depth on the row rather than with a nested list, so every
      // row is the same height and the hit targets stay aligned.
      style={{ paddingInlineStart: `${node.depth * 10}px` }}
    >
      <button
        type="button"
        onClick={onToggle}
        aria-label={
          hasChildren
            ? collapsed
              ? `Expand ${node.name}`
              : `Collapse ${node.name}`
            : undefined
        }
        aria-expanded={hasChildren ? !collapsed : undefined}
        tabIndex={hasChildren ? 0 : -1}
        className="grid size-4 shrink-0 place-items-center rounded text-ink-muted"
      >
        {hasChildren && <Chevron open={!collapsed} />}
      </button>
      <button
        type="button"
        onClick={onSelect}
        aria-current={active ? "true" : undefined}
        className="min-w-0 flex-1 truncate py-1 pr-1 text-left text-sm"
      >
        {node.name}
      </button>
      <span className="shrink-0 pr-2 text-xs tabular-nums text-ink-muted">
        {node.total || ""}
      </span>
    </div>
  );
}

/** The folder captures land in. Rust owns the name; this has to agree with it. */
export const INBOX = "Inbox";

function Row({
  label,
  count,
  active,
  hash,
  emphasis,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  hash?: boolean;
  /** Draws attention while there is something waiting to be filed. */
  emphasis?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "true" : undefined}
      className={[
        "flex w-full items-center rounded-md py-1 pr-2 text-left text-sm transition-colors duration-150 ease-out",
        active
          ? "bg-row-active font-medium text-accent"
          : "text-ink-soft hover:bg-row-hover hover:text-ink",
      ].join(" ")}
    >
      {/*
        Stands in for the chevron a folder or tag row carries, so every label
        in the rail begins on the same vertical line. Without it these rows
        started 8px to the left of the tree below them, which reads as the
        list being crooked rather than as two kinds of row.
      */}
      <span className="size-4 shrink-0" aria-hidden />
      {hash && (
        <span className="mr-0.5 text-highlight" aria-hidden>
          #
        </span>
      )}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <span
        className={[
          "shrink-0 pl-1 text-xs tabular-nums",
          emphasis && !active ? "text-accent" : "text-ink-muted",
        ].join(" ")}
      >
        {count}
      </span>
    </button>
  );
}

/**
 * One level of the tag tree.
 *
 * Shaped like a folder row on purpose — nesting reads the same whether it is
 * location or classification — but the hash keeps the two from being mistaken
 * for one hierarchy, which is the whole point of separating them.
 */
function TagRow({
  node,
  active,
  collapsed,
  onToggle,
  onSelect,
}: {
  node: TagNode;
  active: boolean;
  collapsed: boolean;
  onToggle: () => void;
  onSelect: () => void;
}) {
  const hasChildren = node.children.length > 0;
  return (
    <div
      className={[
        "flex items-center rounded-md transition-colors duration-150 ease-out",
        active
          ? "bg-row-active text-accent"
          : "text-ink-soft hover:bg-row-hover hover:text-ink",
      ].join(" ")}
      style={{ paddingInlineStart: `${node.depth * 10}px` }}
    >
      <button
        type="button"
        onClick={onToggle}
        aria-label={
          hasChildren
            ? collapsed
              ? `Expand ${node.path}`
              : `Collapse ${node.path}`
            : undefined
        }
        aria-expanded={hasChildren ? !collapsed : undefined}
        tabIndex={hasChildren ? 0 : -1}
        className="grid size-4 shrink-0 place-items-center rounded text-ink-muted"
      >
        {hasChildren && <Chevron open={!collapsed} />}
      </button>
      <button
        type="button"
        onClick={onSelect}
        aria-current={active ? "true" : undefined}
        className="flex min-w-0 flex-1 items-center gap-0.5 py-1 pr-1 text-left text-sm"
      >
        <span className="text-highlight" aria-hidden>
          #
        </span>
        <span className="min-w-0 flex-1 truncate">{node.name}</span>
      </button>
      <span
        className="shrink-0 pr-2 text-xs tabular-nums text-ink-muted"
        title={`${node.total} notes with this tag or one beneath it`}
      >
        {node.total}
      </span>
    </div>
  );
}

/**
 * The application's mark: the icon's shooting star, reduced to what survives at
 * 16px — a sparkle and three lines of trail behind it. Any more detail turns to
 * mud at this size.
 */
function Star() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="size-4 shrink-0 text-accent"
      aria-hidden
    >
      <path
        fill="currentColor"
        d="M8.6 2.2l1.7 4.1 4.1 1.7-4.1 1.7-1.7 4.1-1.7-4.1L2.8 8l4.1-1.7z"
      />
      <g
        fill="none"
        stroke="currentColor"
        strokeWidth={1.7}
        strokeLinecap="round"
      >
        <path d="M12.4 11.6a10 10 0 0 1 7.4 7.4" />
        <path d="M9.6 14.4a7 7 0 0 1 5.2 5.2" />
        <path d="M6.8 17.2a4 4 0 0 1 3 3" />
      </g>
    </svg>
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
      className={small ? "size-3" : "size-4"}
      aria-hidden
    >
      <path d="M12 5v14M5 12h14" />
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

/** A view's default mark: a saved question, drawn as a lens. */
function Lens() {
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
      <circle cx="11" cy="11" r="6" />
      <path d="M15.5 15.5L20 20" />
    </svg>
  );
}
