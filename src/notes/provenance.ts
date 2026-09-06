import type { Citation } from "../vault/api";

/**
 * Where a note's two records of what it cites disagree.
 *
 * A citation exists twice in a note, on purpose. `[@ref]` in the body is the
 * mark in the sentence; a `sources:` entry in frontmatter is the provenance
 * record — the page, the source's own words, the kind of evidence. Neither is
 * derived from the other, because neither can be: a marker cannot know which
 * page a claim came from, and a record cannot know which sentence it supports.
 *
 * So they can drift, and until v0.2.1 only one direction of drift was visible.
 * A source recorded and then cut from the prose stayed in the reference list,
 * numbered, cited by nothing.
 *
 * This reports both directions and changes nothing. Deleting a provenance
 * record because no sentence currently mentions it would throw away a page
 * number and a transcribed quote to tidy up a list — and a half-written
 * paragraph is the most ordinary reason for the two to disagree. The
 * researcher decides.
 */
export type Divergence = {
  /** Recorded in `sources:`, but no `[@ref]` for it in the body. */
  uncited: string[];
  /** Cited in the body, but with no provenance record. */
  unrecorded: string[];
};

export function divergence(
  citations: readonly Pick<Citation, "id">[],
  inlineRefs: readonly string[],
): Divergence {
  const inline = new Set(inlineRefs);
  const recorded = new Set(citations.map((c) => c.id));

  return {
    // Deduplicated, because a note can carry two records for one source — the
    // same paper cited at two different pages is a normal thing to do, and
    // reporting it twice would read as two problems.
    uncited: [...recorded].filter((id) => !inline.has(id)),
    unrecorded: inlineRefs.filter(
      (ref, i) => !recorded.has(ref) && inlineRefs.indexOf(ref) === i,
    ),
  };
}

/** Whether there is anything to say at all. */
export function isConsistent(found: Divergence): boolean {
  return found.uncited.length === 0 && found.unrecorded.length === 0;
}

/**
 * One sentence naming what disagrees.
 *
 * Counts rather than names, because the names are listed underneath and a
 * heading that repeats them reads twice as long and says no more.
 */
export function summarise(found: Divergence): string {
  const parts: string[] = [];
  if (found.uncited.length > 0) {
    parts.push(
      found.uncited.length === 1
        ? "1 recorded source is not cited in this note"
        : `${found.uncited.length} recorded sources are not cited in this note`,
    );
  }
  if (found.unrecorded.length > 0) {
    parts.push(
      found.unrecorded.length === 1
        ? "1 citation in the text has no provenance record"
        : `${found.unrecorded.length} citations in the text have no provenance record`,
    );
  }
  return `${parts.join(". ")}.`;
}
