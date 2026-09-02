import { useState } from "react";
import SourcePicker, { describe } from "./SourcePicker";
import { useCitation } from "../editor/citation/citationStore";
import { EVIDENCE_KINDS, type Citation, type NoteSummary } from "../vault/api";

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
 *
 * One list, not two. Anything cited in the prose but not recorded here appears
 * below with an invitation to record it — a reference list and a provenance
 * record that disagree are worse than either alone, and the disagreement is
 * exactly the thing worth showing.
 */
export default function SourcesPanel({
  citations,
  sources,
  inlineRefs,
  onChange,
  onOpen,
  onReport,
}: {
  citations: Citation[];
  /** Every source note in the vault, for resolving ids to titles. */
  sources: NoteSummary[];
  /** Every `[@ref]` in the body, so the prose and this list can be compared. */
  inlineRefs: string[];
  onChange: (citations: Citation[]) => void;
  onOpen: (id: string) => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [picking, setPicking] = useState(false);
  const byId = new Map(sources.map((s) => [s.id, s]));
  const recorded = new Set(citations.map((c) => c.id));
  const onlyInProse = inlineRefs.filter((ref) => !recorded.has(ref));

  const update = (index: number, patch: Partial<Citation>) =>
    onChange(citations.map((c, i) => (i === index ? { ...c, ...patch } : c)));

  return (
    <section>
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

                {/*
                  Marked, not merely indented. Section 11 asks that the
                  source's words, the reader's interpretation and the reader's
                  questions never be visually mixed; the other two live in the
                  note body under their own headings, and this is the one place
                  in the interface holding text that is not the author's. It
                  says so in words rather than relying on the italics, which a
                  reader skimming at midnight will not register as meaning
                  "somebody else wrote this".
                */}
                <p className="mt-2 text-[0.65rem] font-semibold tracking-wide text-highlight uppercase">
                  Source evidence — their words
                </p>
                <textarea
                  value={citation.quote ?? ""}
                  onChange={(event) =>
                    update(index, { quote: event.target.value || null })
                  }
                  placeholder="What it actually says, in its own words"
                  aria-label="Quote"
                  rows={citation.quote ? 2 : 1}
                  className="sutra-quote mt-0.5 w-full resize-y rounded border-l-2 border-highlight bg-highlight-bg/40 px-2 py-1 text-sm text-ink-soft italic outline-none placeholder:text-ink-muted placeholder:not-italic"
                />

                <label className="sutra-no-print mt-1.5 flex items-center gap-1.5 text-xs text-ink-muted">
                  Evidence
                  <select
                    value={citation.kind ?? ""}
                    onChange={(event) =>
                      update(index, { kind: event.target.value || null })
                    }
                    aria-label="Kind of evidence"
                    className="rounded border border-border bg-surface px-1 py-0.5 text-xs text-ink"
                  >
                    <option value="">unspecified</option>
                    {/* A kind written by a newer build is kept and shown,
                        rather than being silently reset to "unspecified" by an
                        older one — the same rule as an unknown view term. */}
                    {citation.kind &&
                      !EVIDENCE_KINDS.includes(
                        citation.kind as (typeof EVIDENCE_KINDS)[number],
                      ) && (
                        <option value={citation.kind}>{citation.kind}</option>
                      )}
                    {EVIDENCE_KINDS.map((kind) => (
                      <option key={kind} value={kind}>
                        {kind}
                      </option>
                    ))}
                  </select>
                </label>
              </li>
            );
          })}
        </ul>
      )}

      {onlyInProse.length > 0 && (
        <div className="mt-3 rounded-lg border border-border px-3 py-2">
          <p className="text-xs text-ink-muted">
            Cited in the text but not recorded here, so there is no page or
            quote to trace the claim back to:
          </p>
          <ul className="mt-1 flex flex-col gap-1">
            {onlyInProse.map((ref) => (
              <InlineOnly
                key={ref}
                reference={ref}
                onRecord={() =>
                  onChange([
                    ...citations,
                    {
                      id: ref,
                      page: null,
                      quote: null,
                      captured: new Date().toISOString(),
                    },
                  ])
                }
              />
            ))}
          </ul>
        </div>
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

/**
 * A work cited in the prose but absent from the provenance record.
 *
 * Recording it is offered, never done: adding a row the user did not ask for
 * would be the app deciding what their evidence is.
 *
 * A legacy Zotero reference cannot be recorded at all — a citation must name a
 * source note, and this one names an item in another program. It says so, and
 * points at the migration.
 */
function InlineOnly({
  reference,
  onRecord,
}: {
  reference: string;
  onRecord: () => void;
}) {
  const state = useCitation(reference);
  const legacy =
    state.status === "found"
      ? state.cited.legacy
      : state.status === "missing" && state.legacy;
  const name =
    state.status === "found" ? state.cited.title : `Reference ${reference}`;

  return (
    <li className="flex items-center justify-between gap-2">
      <span className="min-w-0 flex-1 truncate text-sm text-ink-soft">
        {name}
      </span>
      {legacy ? (
        <span className="shrink-0 text-xs text-highlight">
          a Zotero reference — migrate it first
        </span>
      ) : (
        <button
          type="button"
          onClick={onRecord}
          className="sutra-no-print shrink-0 text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
        >
          record it
        </button>
      )}
    </li>
  );
}
