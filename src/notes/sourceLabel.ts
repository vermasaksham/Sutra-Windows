import type { NoteSummary } from "../vault/api";

/**
 * How a source reads inline: "(Zhou et al., 2019)".
 *
 * Composed from what the source note records rather than by a citation-style
 * engine. Getting APA or Vancouver exactly right is the exporter's problem and
 * a deep one; what an inline marker has to do is let the reader recognise which
 * paper is meant without leaving the sentence.
 *
 * Authors are stored as written — "Zhou, Y.; Wang, L." — because parsing names
 * is a famously bad idea. Only the leading surname is taken, which is the one
 * part of that format anyone can rely on.
 */
export function sourceLabel(source: NoteSummary): string {
  const parts: string[] = [];
  const lead = leadAuthor(source.source?.authors ?? null);
  if (lead) parts.push(lead);

  const year = source.source?.year?.trim();
  if (year) parts.push(year);

  // No authors and no year leaves the title, which is better than an empty
  // pair of brackets and is what a hand-written source usually has.
  if (parts.length === 0) return source.title || "Untitled source";
  return parts.join(", ");
}

function leadAuthor(authors: string | null): string | null {
  const written = authors?.trim();
  if (!written) return null;

  // Semicolons separate authors in the format Zotero writes; a comma inside one
  // separates surname from initials.
  const several = written.includes(";");
  const first = written.split(";")[0]!.trim();
  const surname = first.split(",")[0]!.trim() || first;
  if (!surname) return null;
  return several ? `${surname} et al.` : surname;
}
