import { useSyncExternalStore } from "react";

/**
 * id → title for every note in the vault, plus the click handler for
 * navigating to one.
 *
 * A module store rather than React context because the consumer is a
 * ProseMirror node view. Node views are mounted by the editor, not by our
 * component tree, so props and context do not reach them.
 */

let titles = new Map<string, string>();
let navigate: (id: string) => void = () => {};
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

/** Called whenever the note list is refreshed. */
export function setTitles(next: Iterable<readonly [string, string]>) {
  titles = new Map(next);
  emit();
}

/** Registered once by the app so a link can be followed. */
export function setNavigate(handler: (id: string) => void) {
  navigate = handler;
}

export function followWikiLink(id: string) {
  navigate(id);
}

/**
 * Notes whose titles match `query`, for the `[[` autocomplete.
 *
 * Title-only and in-memory rather than going through the full-text index: the
 * list is already here, it has to keep up with every keystroke, and someone
 * reaching for a link is thinking of a note's name, not its contents.
 */
export function searchTitles(
  query: string,
  limit: number,
): { id: string; title: string }[] {
  const q = query.trim().toLowerCase();
  const all = [...titles].map(([id, title]) => ({ id, title }));
  const matches = q
    ? all.filter((n) => n.title.toLowerCase().includes(q))
    : all;
  return matches.sort((a, b) => a.title.localeCompare(b.title)).slice(0, limit);
}

/**
 * Replace `[[id]]` with the target's title in a plain-text excerpt.
 *
 * Search results and backlink previews come from the raw markdown, so without
 * this they show a 26-character ULID where a person expects a note name. An id
 * with no note is left as-is rather than blanked — the text really is in the
 * file, and hiding it would misrepresent the note.
 */
export function linksAsTitles(text: string): string {
  return text.replace(/\[\[([0-9A-Z]{26})\]\]/g, (whole, id: string) => {
    const title = titles.get(id);
    return title ? title : whole;
  });
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * The title for a note id, or undefined when no such note exists.
 *
 * Undefined is meaningful: the note was deleted or the id was mistyped, and
 * the link should render as dangling rather than silently disappear.
 *
 * The snapshot is a string, not an object — returning a fresh object here
 * would make `useSyncExternalStore` re-render forever.
 */
export function useNoteTitle(id: string): string | undefined {
  return useSyncExternalStore(
    subscribe,
    () => titles.get(id),
    () => undefined,
  );
}
