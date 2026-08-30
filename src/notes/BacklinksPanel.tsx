import { linksAsTitles } from "../editor/wikilink/titleStore";
import type { Backlink } from "../vault/api";

/**
 * Notes that link here.
 *
 * Always rendered, even when empty — an absent panel reads as "this feature is
 * broken", whereas an empty one reads as "nothing links here yet", which is
 * the actual fact.
 *
 * Not printed, though: it is a way of getting somewhere else, and on paper
 * there is nowhere else to get to.
 */
export default function BacklinksPanel({
  backlinks,
  onSelect,
}: {
  backlinks: Backlink[];
  onSelect: (id: string) => void;
}) {
  return (
    <section className="sutra-no-print mt-16 border-t border-border pt-4">
      <h2 className="mb-2 text-xs font-semibold tracking-wide text-ink-muted uppercase">
        Linked from {backlinks.length > 0 && `(${backlinks.length})`}
      </h2>
      {backlinks.length === 0 ? (
        <p className="text-sm text-ink-muted">
          Nothing links here yet. Type <code className="font-mono">[[</code> in
          another note to make a link.
        </p>
      ) : (
        <ul className="flex flex-col gap-1">
          {backlinks.map((link) => (
            <li key={link.id}>
              <button
                type="button"
                onClick={() => onSelect(link.id)}
                className="w-full rounded-lg border border-border bg-surface px-3 py-2 text-left transition-colors duration-150 ease-out hover:border-accent"
              >
                <span className="block truncate text-sm text-ink">
                  {link.title || "Untitled"}
                </span>
                <span className="block truncate text-xs text-ink-muted">
                  {linksAsTitles(link.excerpt)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
