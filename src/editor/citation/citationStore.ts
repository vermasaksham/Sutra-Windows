import { useSyncExternalStore } from "react";
import { zoteroApi, type Reference } from "../../vault/api";

/**
 * Zotero item key → reference, for rendering `[@KEY]` as a label.
 *
 * The same shape as the wikilink title store, and for the same reason: a node
 * view is mounted by ProseMirror, not by our component tree, so props and
 * context do not reach it.
 *
 * The difference is where the data comes from. Note titles are already in
 * memory; references live in Zotero, so a key that has not been seen is
 * fetched. Requests are batched into one call per tick, because a document
 * with thirty citations should not make thirty round trips.
 */

const references = new Map<string, Reference>();
/** Keys already looked up and genuinely absent, so we stop asking. */
const missing = new Set<string>();
const pending = new Set<string>();
const listeners = new Set<() => void>();
let scheduled = false;

function emit() {
  for (const listener of listeners) listener();
}

function flush() {
  scheduled = false;
  const keys = [...pending];
  pending.clear();
  if (keys.length === 0) return;

  zoteroApi
    .byKeys(keys)
    .then((found) => {
      for (const reference of found) references.set(reference.key, reference);
      // Anything asked for and not returned does not exist in the library.
      for (const key of keys) {
        if (!references.has(key)) missing.add(key);
      }
      emit();
    })
    .catch(() => {
      // Zotero unreachable. Leave the keys unresolved rather than marking them
      // missing — the reference may well exist, and the citation should say
      // "unresolved" rather than claim it was deleted.
      emit();
    });
}

function request(key: string) {
  if (references.has(key) || missing.has(key) || pending.has(key)) return;
  pending.add(key);
  if (!scheduled) {
    scheduled = true;
    queueMicrotask(flush);
  }
}

/** Called when a reference is picked, so it renders without a round trip. */
export function remember(reference: Reference) {
  references.set(reference.key, reference);
  missing.delete(reference.key);
  emit();
}

/** Every reference currently resolved, for the bibliography. */
export function resolved(keys: string[]): Reference[] {
  return keys
    .map((key) => references.get(key))
    .filter((r): r is Reference => r !== undefined);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export type CitationState =
  | { status: "loading" }
  | { status: "missing" }
  | { status: "found"; reference: Reference };

export function useCitation(key: string): CitationState {
  const reference = useSyncExternalStore(
    subscribe,
    () => references.get(key),
    () => undefined,
  );
  const gone = useSyncExternalStore(
    subscribe,
    () => missing.has(key),
    () => false,
  );

  if (reference) return { status: "found", reference };
  if (gone) return { status: "missing" };
  request(key);
  return { status: "loading" };
}

/** Format a reference the way it appears inline. */
export function label(reference: Reference): string {
  const who = reference.creators || "Unknown";
  return reference.year ? `${who}, ${reference.year}` : who;
}
