import type { NoteSummary } from "../vault/api";

export type TreeNode = NoteSummary & { children: TreeNode[]; depth: number };

/**
 * Assemble the flat list Rust returns into a tree.
 *
 * Done here rather than in SQL so no query shape crosses the boundary; Rust
 * hands over rows with `parent` and `position` and the shape is our business.
 *
 * Two things the vault can contain that a naive build would choke on, because
 * these files are hand-editable:
 *
 * - A `parent` pointing at a note that does not exist. Those are treated as
 *   roots rather than dropped — the note is real and must stay reachable.
 * - A parent cycle (A's parent is B, B's parent is A). Any note not reached
 *   from a root is surfaced at the top level, so a cycle costs correct nesting
 *   but never hides a note or hangs the app.
 */
export function buildTree(notes: NoteSummary[]): TreeNode[] {
  const byId = new Map<string, TreeNode>();
  for (const note of notes) {
    byId.set(note.id, { ...note, children: [], depth: 0 });
  }

  const roots: TreeNode[] = [];
  for (const node of byId.values()) {
    const parent = node.parent ? byId.get(node.parent) : undefined;
    if (parent && parent.id !== node.id) parent.children.push(node);
    else roots.push(node);
  }

  // Walk from the roots, assigning depth. Anything unvisited is in a cycle.
  const visited = new Set<string>();
  const assign = (nodes: TreeNode[], depth: number) => {
    for (const node of nodes) {
      if (visited.has(node.id)) continue;
      visited.add(node.id);
      node.depth = depth;
      node.children.sort(compare);
      assign(node.children, depth + 1);
    }
  };
  roots.sort(compare);
  assign(roots, 0);

  for (const node of byId.values()) {
    if (!visited.has(node.id)) {
      node.depth = 0;
      node.children = [];
      roots.push(node);
    }
  }

  roots.sort(compare);
  return roots;
}

function compare(a: NoteSummary, b: NoteSummary) {
  return (
    a.position - b.position ||
    a.title.localeCompare(b.title, undefined, { sensitivity: "base" })
  );
}

/** The chain from the root down to `id`, for breadcrumbs. */
export function pathTo(notes: NoteSummary[], id: string | null): NoteSummary[] {
  if (!id) return [];
  const byId = new Map(notes.map((n) => [n.id, n]));
  const path: NoteSummary[] = [];
  // A cycle in `parent` would loop forever, so remember where we have been.
  const seen = new Set<string>();

  let current = byId.get(id);
  while (current && !seen.has(current.id)) {
    seen.add(current.id);
    path.unshift(current);
    current = current.parent ? byId.get(current.parent) : undefined;
  }
  return path;
}
