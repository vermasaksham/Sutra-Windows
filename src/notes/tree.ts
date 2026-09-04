import type { NoteSummary } from "../vault/api";

/**
 * The folder tree, assembled from the paths Rust reports.
 *
 * Rust sends a flat list of folders and a flat list of notes, each note
 * carrying the folder it sits in. Nesting is this side's business, because it
 * is a display concern: the same two lists could equally be drawn flat.
 *
 * Folders that hold notes but were never listed as directories in their own
 * right still appear — a note at `Research/Sb2Se3/Cp.md` implies both
 * `Research` and `Research/Sb2Se3`, and inferring them is cheaper than
 * requiring the two lists to agree.
 */

export type FolderNode = {
  /** Vault-relative, `/`-separated. "" is the root. */
  path: string;
  /** The last segment, which is what the rail shows. */
  name: string;
  depth: number;
  children: FolderNode[];
  /** Notes directly in this folder, not in its descendants. */
  notes: NoteSummary[];
  /** Notes here and everywhere below, which is what a folder's count means. */
  total: number;
};

export function buildFolders(
  folders: string[],
  notes: NoteSummary[],
): FolderNode {
  const root: FolderNode = {
    path: "",
    name: "",
    depth: -1,
    children: [],
    notes: [],
    total: notes.length,
  };
  const byPath = new Map<string, FolderNode>([["", root]]);

  // `ensure` walks down from the root creating what is missing, so a path
  // arriving before its parent still lands in the right place.
  const ensure = (path: string): FolderNode => {
    const existing = byPath.get(path);
    if (existing) return existing;

    const cut = path.lastIndexOf("/");
    const parent = ensure(cut === -1 ? "" : path.slice(0, cut));
    const node: FolderNode = {
      path,
      name: path.slice(cut + 1),
      depth: parent.depth + 1,
      children: [],
      notes: [],
      total: 0,
    };
    parent.children.push(node);
    byPath.set(path, node);
    return node;
  };

  for (const folder of folders) if (folder !== "") ensure(folder);
  for (const note of notes) ensure(note.folder).notes.push(note);

  // Totals roll up, so a parent's count includes everything beneath it.
  const rollUp = (node: FolderNode): number => {
    node.total =
      node.notes.length + node.children.reduce((sum, c) => sum + rollUp(c), 0);
    return node.total;
  };
  for (const child of root.children) rollUp(child);

  const sort = (node: FolderNode) => {
    node.children.sort((a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
    );
    node.children.forEach(sort);
  };
  sort(root);

  return root;
}

/** Depth-first, so the rail can render one flat list of rows. */
export function flattenFolders(
  root: FolderNode,
  collapsed: Set<string>,
): FolderNode[] {
  const rows: FolderNode[] = [];
  const walk = (nodes: FolderNode[]) => {
    for (const node of nodes) {
      rows.push(node);
      if (!collapsed.has(node.path)) walk(node.children);
    }
  };
  walk(root.children);
  return rows;
}

/** The notes in a folder and everything below it, newest first. */
export function notesUnder(
  notes: NoteSummary[],
  folder: string | null,
): NoteSummary[] {
  const matching =
    folder === null
      ? notes
      : notes.filter(
          (n) => n.folder === folder || n.folder.startsWith(`${folder}/`),
        );
  return [...matching].sort((a, b) => b.updated.localeCompare(a.updated));
}

/** `Research/Sb2Se3` -> [["Research","Research"], ["Sb2Se3","Research/Sb2Se3"]] */
export function folderCrumbs(folder: string): Array<[string, string]> {
  if (folder === "") return [];
  const parts = folder.split("/");
  return parts.map((name, i) => [name, parts.slice(0, i + 1).join("/")]);
}
