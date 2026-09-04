import { useSyncExternalStore } from "react";
import { zoteroApi, type NoteSummary, type Reference } from "../../vault/api";
import { currentStyle, styledFor } from "../../notes/citationStyle";
import { sourceLabel } from "../../notes/sourceLabel";

/**
 * What a `[@ref]` in the text should read as.
 *
 * The same shape as the wikilink title store, and for the same reason: a node
 * view is mounted by ProseMirror, not by our component tree, so props and
 * context do not reach it.
 *
 * Two kinds of reference share this syntax, and length tells them apart.
 * Twenty-six characters is a source note in the vault, resolved from notes
 * already in memory — instant, and it works whether or not Zotero was ever
 * installed. Eight is a Zotero item key, which is how citations were written
 * before a source became a note; those still resolve, by asking Zotero, and
 * they are what the migration exists to remove.
 */

/** A ULID is 26 characters; a Zotero item key is 8. */
export function isSourceNote(ref: string): boolean {
  return ref.length === 26;
}

/** Source notes by id, kept current by the app. */
let sources = new Map<string, NoteSummary>();

/** Zotero references by item key, fetched on demand. */
const references = new Map<string, Reference>();
/** Keys already looked up and genuinely absent, so we stop asking. */
const missing = new Set<string>();
const pending = new Set<string>();
const listeners = new Set<() => void>();
let scheduled = false;

function emit() {
  for (const listener of listeners) listener();
}

export function setSources(all: NoteSummary[]) {
  sources = new Map(all.map((source) => [source.id, source]));
  emit();
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

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Sources in the vault matching what has been typed, newest-looking first.
 *
 * Matching on title and authors, and on the squashed forms of both, so a
 * half-remembered "zhou2019" finds "Zhou, Y. (2019)". An empty query returns
 * everything, which is what the `@` menu shows before a second character.
 */
export function vaultCandidates(query: string): VaultCandidate[] {
  const needle = squash(query);
  const all = [...sources.values()];
  const matching = needle
    ? all.filter((source) =>
        squash(
          `${source.title} ${source.source?.authors ?? ""} ${source.source?.year ?? ""}`,
        ).includes(needle),
      )
    : all;

  return matching.slice(0, 8).map((source) => ({
    kind: "source" as const,
    id: source.id,
    title: source.title,
    detail: fromSource(source).detail,
    zotero: source.source?.zotero ?? null,
  }));
}

type VaultCandidate = {
  kind: "source";
  id: string;
  title: string;
  detail: string;
  /** So the Zotero half of the menu can avoid offering it twice. */
  zotero: string | null;
};

/** Only letters and digits, so punctuation and spacing stop mattering. */
function squash(text: string): string {
  return text.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
}

/** What a citation resolved to, whichever kind it was. */
export type Cited = {
  /** "Zhou et al., 2019" — the vault's own label, used when the library has
   *  rendered nothing. */
  label: string;
  /** Exactly what the library rendered inline, or null when it has not.
   *
   *  Kept apart from `label` because choosing between them is `marker`'s job,
   *  and because a numeric style's rendering is the right *shape* with the
   *  wrong number in it: Zotero renders one item at a time and cannot know
   *  what else this note cites. */
  styled: string | null;
  /** The full title, for a tooltip. */
  title: string;
  /** A bibliography line: everything known about it. */
  detail: string;
  /** True for a Zotero key, which only resolves while Zotero is running. */
  legacy: boolean;
};

export type CitationState =
  | { status: "loading" }
  | { status: "missing"; legacy: boolean }
  | { status: "found"; cited: Cited };

/**
 * The open note's citation order, so a numeric style can say which number a
 * citation is.
 *
 * Kept here rather than passed down because a citation node view is mounted by
 * ProseMirror, not by our component tree — the same reason the titles and
 * sources live in module stores. It is the note's order, so it is replaced
 * wholesale whenever the note or its prose changes.
 */
let order: readonly string[] = [];

export function setCitationOrder(next: readonly string[]) {
  if (next.length === order.length && next.every((r, i) => r === order[i])) {
    return;
  }
  order = next;
  for (const listener of listeners) listener();
}

/** Where this reference sits, 1-based, or null if the note does not cite it. */
export function positionOf(ref: string): number | null {
  const at = order.indexOf(ref);
  return at === -1 ? null : at + 1;
}

export function useCitationPosition(ref: string): number | null {
  return useSyncExternalStore(
    subscribe,
    () => positionOf(ref),
    () => null,
  );
}

/**
 * Resolve one reference.
 *
 * All three subscriptions are unconditional: which one matters depends on the
 * reference, but React requires the same hooks in the same order on every
 * render, and a citation can change from one kind to the other under it — that
 * is exactly what the migration does.
 */
export function useCitation(ref: string): CitationState {
  const source = useSyncExternalStore(
    subscribe,
    () => sources.get(ref),
    () => undefined,
  );
  const reference = useSyncExternalStore(
    subscribe,
    () => references.get(ref),
    () => undefined,
  );
  const gone = useSyncExternalStore(
    subscribe,
    () => missing.has(ref),
    () => false,
  );

  if (isSourceNote(ref)) {
    // Nothing to fetch: a source note either is in the vault or is not.
    return source
      ? { status: "found", cited: fromSource(source) }
      : { status: "missing", legacy: false };
  }

  if (reference) return { status: "found", cited: fromReference(reference) };
  if (gone) return { status: "missing", legacy: true };
  request(ref);
  return { status: "loading" };
}

/**
 * Every reference that currently resolves, for a bibliography.
 *
 * Unresolved ones are simply absent rather than guessed at: a reference list
 * that invents an entry is worse than one that is short.
 */
export function resolved(refs: string[]): Cited[] {
  const out: Cited[] = [];
  for (const ref of refs) {
    if (isSourceNote(ref)) {
      const source = sources.get(ref);
      if (source) out.push(fromSource(source));
    } else {
      const reference = references.get(ref);
      if (reference) out.push(fromReference(reference));
    }
  }
  return out;
}

function fromSource(source: NoteSummary): Cited {
  const meta = source.source;
  // The library's own rendering when we have it, the vault's plain label
  // otherwise. Falling back rather than blanking is the rule everywhere the
  // styled forms are used: an unstyled citation is imperfect, an empty one is
  // a broken draft.
  const rendered = styledFor(meta, currentStyle());
  return {
    styled: rendered?.citation ?? null,
    label: sourceLabel(source),
    title: source.title,
    detail:
      rendered?.bib ??
      [
        meta?.authors,
        meta?.year && `(${meta.year})`,
        source.title,
        meta?.container,
        meta?.doi && `doi:${meta.doi}`,
      ]
        .filter(Boolean)
        .join(" "),
    legacy: false,
  };
}

function fromReference(reference: Reference): Cited {
  const who = reference.creators || "Unknown";
  return {
    // A Zotero item has never been through the style engine — that happens
    // when it becomes a source note — so there is nothing rendered to prefer.
    styled: null,
    label: reference.year ? `${who}, ${reference.year}` : who,
    title: reference.title,
    detail: [
      reference.creators,
      reference.year && `(${reference.year})`,
      reference.title,
      reference.container,
      reference.doi && `doi:${reference.doi}`,
    ]
      .filter(Boolean)
      .join(" "),
    legacy: true,
  };
}
