import type { ChapterUse } from "../vault/api";
import { displayTitle } from "./titleText";

/**
 * The chapters this note is part of.
 *
 * A note assembled into a chapter has no idea it has been, because the claim
 * lives in the chapter's frontmatter rather than in the note's — which is right,
 * since the same note belongs to two chapters and neither owns it. This is the
 * reverse lookup that makes that arrangement liveable: without it, moving a
 * paragraph would mean opening every chapter to find out what it affects.
 *
 * Hidden when empty. Unlike backlinks, most notes in most vaults are in no
 * chapter at all, and a permanent "not in any chapter" would be noise on every
 * note to serve the few that are.
 */
export default function ChapterUses({
  uses,
  onSelect,
}: {
  uses: ChapterUse[];
  onSelect: (id: string) => void;
}) {
  if (uses.length === 0) return null;

  return (
    <section>
      <h2 className="mb-2 text-xs font-semibold tracking-wide text-ink-muted uppercase">
        In {uses.length === 1 ? "a chapter" : `${uses.length} chapters`}
      </h2>
      <ul className="flex flex-col gap-1">
        {uses.map((use) => (
          <li key={`${use.id}:${use.position}`}>
            <button
              type="button"
              onClick={() => onSelect(use.id)}
              className="w-full rounded-lg border border-border bg-surface px-3 py-2 text-left transition-colors hover:border-accent"
            >
              <span className="block truncate text-sm text-ink">
                {displayTitle(use.title) || "Untitled chapter"}
              </span>
              <span className="block text-xs text-ink-muted tabular-nums">
                {/* One-based, because "note 1 of 12" is how a person counts. */}
                note {use.position + 1} of {use.of}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
