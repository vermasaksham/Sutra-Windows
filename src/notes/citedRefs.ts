/**
 * Every `[@ref]` in a note's markdown, in order of first appearance.
 *
 * Two shapes share the syntax and length tells them apart: 26 characters is a
 * source note in the vault, 8 is a Zotero item key from before sources were
 * notes. Both are returned — the second kind still has to be shown while any
 * remain, and hiding them would make the migration look like it had already
 * happened.
 */
export function citedRefs(body: string): string[] {
  const found: string[] = [];
  for (const match of body.matchAll(/\[@([0-9A-Z]{8}|[0-9A-Z]{26})\]/g)) {
    const ref = match[1]!;
    if (!found.includes(ref)) found.push(ref);
  }
  return found;
}
