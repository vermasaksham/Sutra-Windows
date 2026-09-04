import type { NoteSummary } from "../vault/api";

/**
 * The tag tree, assembled from the flat tags Rust stores.
 *
 * Tags nest on slashes, so `research/materials/sb2se3` is three levels. Only
 * the leaf is ever written to a note; the levels above it are implied, and
 * inferring them here is what lets the rail show a tree without the vault
 * having to keep a registry of tags that would then need pruning.
 *
 * A consequence worth stating: there is no such thing as an unused tag in this
 * design. A tag exists exactly while some note carries it, so section 11's
 * "unused-tag detection" has nothing to detect — deleting the last note with a
 * tag removes the tag, and no cleanup step is needed.
 */

export type TagNode = {
  /** The full path, e.g. `research/materials`. What filtering uses. */
  path: string;
  /** The last segment, which is what the rail shows. */
  name: string;
  depth: number;
  children: TagNode[];
  /** Notes carrying exactly this tag. */
  own: number;
  /** Notes carrying this tag or anything beneath it — what the count means. */
  total: number;
};

export function buildTagTree(notes: NoteSummary[]): TagNode[] {
  const roots: TagNode[] = [];
  const byPath = new Map<string, TagNode>();

  const ensure = (path: string): TagNode => {
    const existing = byPath.get(path);
    if (existing) return existing;

    const cut = path.lastIndexOf("/");
    const node: TagNode = {
      path,
      name: path.slice(cut + 1),
      depth: cut === -1 ? 0 : path.split("/").length - 1,
      children: [],
      own: 0,
      total: 0,
    };
    byPath.set(path, node);
    if (cut === -1) roots.push(node);
    else ensure(path.slice(0, cut)).children.push(node);
    return node;
  };

  // A note counts once towards each level of each of its tags, but only once
  // overall per level — two tags under `research` must not count the note twice.
  for (const note of notes) {
    const levels = new Set<string>();
    for (const tag of note.tags) {
      ensure(tag).own += 1;
      const parts = tag.split("/");
      for (let i = 0; i < parts.length; i += 1) {
        levels.add(parts.slice(0, i + 1).join("/"));
      }
    }
    for (const path of levels) ensure(path).total += 1;
  }

  const sort = (nodes: TagNode[]) => {
    nodes.sort((a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
    );
    nodes.forEach((n) => sort(n.children));
  };
  sort(roots);
  return roots;
}

/** Depth-first, so the rail can render one flat list of rows. */
export function flattenTags(
  roots: TagNode[],
  collapsed: Set<string>,
): TagNode[] {
  const rows: TagNode[] = [];
  const walk = (nodes: TagNode[]) => {
    for (const node of nodes) {
      rows.push(node);
      if (!collapsed.has(node.path)) walk(node.children);
    }
  };
  walk(roots);
  return rows;
}

/**
 * Does this note carry the tag, or anything beneath it?
 *
 * Selecting `research` in the rail should find the notes filed under
 * `research/materials/sb2se3`, or the tree is decoration.
 */
export function taggedWith(note: NoteSummary, tag: string): boolean {
  return note.tags.some((t) => t === tag || t.startsWith(`${tag}/`));
}

/**
 * Existing tags worth offering while someone types a new one.
 *
 * Matching on the squashed form as well as the literal one is the point:
 * someone typing "thermalcond" should be shown `thermal-conductivity` and take
 * it, which is how a second spelling never gets created in the first place.
 * Preventing the duplicate beats detecting it afterwards.
 *
 * `all` must arrive most-used first, because ties keep its order. When a vault
 * already contains both spellings of a tag, the one people actually use has to
 * be the one offered first — otherwise this feature spreads the duplicate it
 * exists to prevent.
 */
export function suggestTags(
  draft: string,
  all: string[],
  already: string[],
  limit = 6,
): string[] {
  const needle = squash(draft);
  const taken = new Set(already);
  // Nothing typed yet: offer the vault's most-used tags rather than nothing.
  //
  // The caller passes them usage-sorted, so this is the shortlist a person
  // would have picked from anyway — and an empty box that only responds once
  // you have guessed the first letter of a tag you already have is a box that
  // makes you retype tags you thought you were reusing.
  if (needle === "") {
    return all.filter((tag) => !taken.has(tag)).slice(0, limit);
  }
  return all
    .filter((tag) => !taken.has(tag) && squash(tag).includes(needle))
    .sort((a, b) => {
      // Something that starts with what was typed beats something that merely
      // contains it. Everything else is a tie, and Array.sort is stable, so
      // ties fall back to the caller's order — which is by usage.
      const aStarts = squash(a).startsWith(needle);
      const bStarts = squash(b).startsWith(needle);
      if (aStarts === bStarts) return 0;
      return aStarts ? -1 : 1;
    })
    .slice(0, limit);
}

/** Only the letters and digits, matching what Rust compares tags by. */
function squash(tag: string): string {
  return tag.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
}
