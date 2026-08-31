import { useState } from "react";
import SourcePicker, { describe } from "./SourcePicker";
import type { Citation, NoteSummary } from "../vault/api";

/**
 * What this note draws on, and where in it.
 *
 * The provenance record from section 5, shown where it is written: in the
 * note's own frontmatter. Each entry can carry a page and the source's own
 * words, because "Zhou 2019 says X" is not traceable and "Zhou 2019, p. 6,
 * 'thermal conductivity decreases…'" is.
 *
 * The quote sits in its own field rather than in the prose on purpose. It is
 * the one piece of text in the note that is not the author's, and keeping it
 * structurally separate is the same argument as the three voices, applied to
 * the part a reader is most likely to paraphrase without meaning to.
 */
export default function SourcesPanel({
  citations,
  sources,
  onChange,
  onOpen,
  onReport,
}: {
  citations: Citation[];
  /** Every source note in the vault, for resolving ids to titles. */
  sources: NoteSummary[];
  onChange: (citations: Citation[]) => void;
  onOpen: (id: string) => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [picking, setPicking] = useState(false);
  const byId = new Map(sources.map((s) => [s.id, s]));

  const update = (index: number, patch: Partial<Citation>) =>
    onChange(citations.map((c, i) => (i === index ? { ...c, ...patch } : c)));

  return (
    <section className="mt-10">
      <div className="flex items-baseline justify-between">
        <h2 className="text-xs font-semibold tracking-wide text-ink-muted uppercase">
          Sources {citations.length > 0 && `(${citations.length})`}
        </h2>
        <button
          type="button"
          onClick={() => setPicking(true)}
          className="sutra-no-print text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
        >
          + source
        </button>
      </div>

      {citations.length === 0 ? (
        <p className="sutra-no-print mt-2 text-sm text-ink-muted">
          Nothing cited yet. Adding a source records what it says and where, so
          a claim here can be traced back to the page it came from.
        </p>
      ) : (
        <ul className="mt-2 flex flex-col gap-2">
          {citations.map((citation, index) => {
            const source = byId.get(citation.id);
            return (
              <li
                key={`${citation.id}:${index}`}
                className="rounded-lg border border-border bg-surface px-3 py-2"
              >
                <div className="flex items-baseline gap-2">
                  {source ? (
                    <button
                      type="button"
                      onClick={() => onOpen(citation.id)}
                      className="min-w-0 flex-1 truncate text-left text-sm text-accent"
                    >
                      {source.title}
                    </button>
                  ) : (
                    // The source note is gone — deleted, or not synced yet. The
                    // citation still says what it said, which is the point of
                    // keeping the quote in this file rather than in the source.
                    <span className="min-w-0 flex-1 truncate text-sm text-highlight">
                      Source not in this vault
                    </span>
                  )}
                  <label className="sutra-no-print shrink-0 text-xs text-ink-muted">
                    p.{" "}
                    <input
                      value={citation.page ?? ""}
                      onChange={(event) =>
                        update(index, { page: event.target.value || null })
                      }
                      aria-label="Page"
                      size={5}
                      className="rounded bg-row-hover px-1 py-0.5 text-xs text-ink outline-none"
                    />
                  </label>
                  <button
                    type="button"
                    onClick={() =>
                      onChange(citations.filter((_, i) => i !== index))
                    }
                    aria-label="Remove this source"
                    className="sutra-no-print shrink-0 text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
                  >
                    ×
                  </button>
                </div>

                {source && (
                  <p className="truncate text-xs text-ink-muted">
                    {describe(source.source)}
                  </p>
                )}

                <textarea
                  value={citation.quote ?? ""}
                  onChange={(event) =>
                    update(index, { quote: event.target.value || null })
                  }
                  placeholder="What it actually says, in its own words"
                  aria-label="Quote"
                  rows={citation.quote ? 2 : 1}
                  className="sutra-quote mt-1.5 w-full resize-y rounded bg-row-hover px-2 py-1 text-sm text-ink-soft italic outline-none placeholder:text-ink-muted placeholder:not-italic"
                />
              </li>
            );
          })}
        </ul>
      )}

      {picking && (
        <SourcePicker
          onClose={() => setPicking(false)}
          onReport={onReport}
          onPick={(source) => {
            setPicking(false);
            // Citing the same source twice is legitimate — two pages, two
            // claims — so this appends rather than de-duplicating.
            onChange([
              ...citations,
              {
                id: source.id,
                page: null,
                quote: null,
                captured: new Date().toISOString(),
              },
            ]);
          }}
        />
      )}
    </section>
  );
}
