import { useEffect, useState } from "react";
import { useCitation } from "../editor/citation/citationStore";

/** Every `[@KEY]` in a note's markdown, in order of first appearance. */
export function citationKeys(body: string): string[] {
  const keys: string[] = [];
  for (const match of body.matchAll(/\[@([A-Z0-9]{8})\]/g)) {
    const key = match[1]!;
    if (!keys.includes(key)) keys.push(key);
  }
  return keys;
}

function Entry({ itemKey }: { itemKey: string }) {
  const state = useCitation(itemKey);

  if (state.status === "loading") {
    return <li className="text-sm text-ink-muted">Looking up {itemKey}…</li>;
  }
  if (state.status === "missing") {
    return (
      <li className="text-sm text-highlight">
        Not in Zotero: <code className="font-mono">{itemKey}</code>
      </li>
    );
  }

  const { reference } = state;
  return (
    <li className="text-sm text-ink-soft">
      <span className="text-ink">{reference.title}</span>
      {reference.creators && <> — {reference.creators}</>}
      {reference.year && <> ({reference.year})</>}
      {reference.doi && (
        <>
          {" "}
          <span className="font-mono text-xs text-ink-muted">
            doi:{reference.doi}
          </span>
        </>
      )}
    </li>
  );
}

/**
 * The works cited in this note.
 *
 * Built from the markdown rather than from editor state, so it is the same
 * list whatever produced the file — including a note edited entirely outside
 * Sutra.
 *
 * Absent rather than empty when nothing is cited: unlike backlinks, which are
 * a standing feature of every note, a bibliography on a note with no citations
 * is just a heading taking up space.
 */
export default function Bibliography({ body }: { body: string }) {
  const [keys, setKeys] = useState<string[]>([]);

  // Recomputed as the body changes, but not on every keystroke — scanning is
  // cheap, re-rendering a list is not.
  useEffect(() => {
    const timer = setTimeout(() => setKeys(citationKeys(body)), 300);
    return () => clearTimeout(timer);
  }, [body]);

  if (keys.length === 0) return null;

  return (
    <section className="mt-10 border-t border-border pt-4">
      <h2 className="mb-2 text-xs font-semibold tracking-wide text-ink-muted uppercase">
        References ({keys.length})
      </h2>
      <ul className="flex flex-col gap-1.5">
        {keys.map((key) => (
          <Entry key={key} itemKey={key} />
        ))}
      </ul>
    </section>
  );
}
