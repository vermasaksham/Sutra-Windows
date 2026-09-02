import { useSyncExternalStore } from "react";
import type { NoteSummary, SourceMeta, StyledCitation } from "../vault/api";

/**
 * Which citation style is in force.
 *
 * A module store rather than a prop, because the style is needed at the two
 * ends of the app that are furthest apart: a citation node deep inside the
 * editor, and the bibliography in the context panel. Threading one string
 * through every component between them would be a worse cost than this.
 *
 * The value is the app's copy of a setting Rust owns. It is set once at start
 * and again whenever the settings panel saves, and nothing here writes to the
 * config file — so there is exactly one source of truth and this is a mirror
 * of it.
 */

let style = "";
const listeners = new Set<() => void>();

export function setCitationStyle(next: string) {
  const trimmed = next.trim();
  if (trimmed === style) return;
  style = trimmed;
  for (const listener of listeners) listener();
}

export function currentStyle(): string {
  return style;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useCitationStyle(): string {
  return useSyncExternalStore(subscribe, currentStyle, () => "");
}

/**
 * How this source reads in the given style, if the library has told us.
 *
 * Absent is the normal case for a source imported before the style was chosen,
 * or one the library could not render — and absent means *fall back*, never
 * "show nothing". A citation that reads "Zhou et al., 2019" when it should say
 * "(3)" is imperfect; a citation that reads nothing is a broken draft.
 */
export function styledFor(
  meta: SourceMeta | null | undefined,
  style: string,
): StyledCitation | null {
  if (!meta || !style) return null;
  const found = meta.styled?.[style];
  if (!found) return null;
  // A cached entry with neither half is not an answer.
  return found.citation || found.bib ? found : null;
}

/** One line of a bibliography, and where it came from. */
export type BibliographyEntry = {
  /** The source note, so the line can be clicked through to. */
  id: string;
  title: string;
  /** The rendered entry, or the plain description when unstyled. */
  text: string;
  /** False when this is the fallback rather than the library's own rendering,
   *  so the UI can say so instead of implying a style was applied. */
  styled: boolean;
};

/**
 * The reference list for a note.
 *
 * In citation order — the order the note records them — rather than
 * alphabetical, because a numeric style like ACS or Nature numbers by first
 * appearance and re-sorting would put "(1)" second. A style that wants
 * alphabetical order is the library's business at export time, not a decision
 * to make here on data we have already flattened to strings.
 *
 * Deduplicated by source: citing one paper on three pages is three pieces of
 * evidence and one reference.
 */
export function bibliography(
  citedIds: string[],
  sources: NoteSummary[],
  style: string,
): BibliographyEntry[] {
  const byId = new Map(sources.map((s) => [s.id, s]));
  const seen = new Set<string>();
  const out: BibliographyEntry[] = [];

  for (const id of citedIds) {
    if (seen.has(id)) continue;
    seen.add(id);
    const source = byId.get(id);
    if (!source) continue;

    const rendered = styledFor(source.source, style);
    if (rendered?.bib) {
      out.push({ id, title: source.title, text: rendered.bib, styled: true });
      continue;
    }
    out.push({
      id,
      title: source.title,
      text: describe(source),
      styled: false,
    });
  }
  return out;
}

/**
 * The fallback line: everything the vault knows, in a conventional order.
 *
 * Not a citation style, and not pretending to be one. It exists so a note
 * whose sources have never been rendered still has a usable reference list.
 */
function describe(source: NoteSummary): string {
  const meta = source.source;
  return [
    meta?.authors,
    meta?.year && `(${meta.year})`,
    source.title,
    meta?.container,
    meta?.doi && `doi:${meta.doi}`,
  ]
    .filter(Boolean)
    .join(" ");
}

/**
 * Split a rendered entry into plain and emphasised runs.
 *
 * The stored string is markdown, because that is what a note is and what
 * pasting into one should carry — but a reference list that shows its
 * asterisks looks like a bug rather than a citation. So the string keeps the
 * markdown and the panel renders it.
 *
 * Only single-asterisk emphasis, which is all Zotero's output produces: a
 * journal or book title. An unmatched asterisk is left as itself rather than
 * swallowing the rest of the line.
 */
export function emphasisRuns(
  text: string,
): Array<{ text: string; emphasis: boolean }> {
  const runs: Array<{ text: string; emphasis: boolean }> = [];
  let rest = text;

  while (rest.length > 0) {
    const open = rest.indexOf("*");
    if (open === -1) {
      runs.push({ text: rest, emphasis: false });
      break;
    }
    const close = rest.indexOf("*", open + 1);
    if (close === -1) {
      // No partner: this is punctuation, not markup.
      runs.push({ text: rest, emphasis: false });
      break;
    }
    if (open > 0) runs.push({ text: rest.slice(0, open), emphasis: false });
    const inner = rest.slice(open + 1, close);
    if (inner.length > 0) runs.push({ text: inner, emphasis: true });
    rest = rest.slice(close + 1);
  }

  return runs.filter((run) => run.text.length > 0);
}
