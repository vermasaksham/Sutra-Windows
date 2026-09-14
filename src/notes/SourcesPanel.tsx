import { useState } from "react";
import SourcePicker, { describe } from "./SourcePicker";
import { displayTitle } from "./titleText";
import {
  useCitation,
  type CitationState,
} from "../editor/citation/citationStore";
import { divergence, isConsistent, summarise } from "./provenance";
import {
  EVIDENCE_KINDS,
  NOTE_TYPES,
  type Citation,
  type NoteSummary,
  type NoteType,
} from "../vault/api";

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
  notes,
  inlineRefs,
  onChange,
  onOpen,
  onReport,
}: {
  citations: Citation[];
  /** Every note in the vault, for resolving a citation's id to its note. */
  notes: NoteSummary[];
  /** Every `[@ref]` in the body, so the prose and this list can be compared. */
  inlineRefs: string[];
  onChange: (citations: Citation[]) => void;
  onOpen: (id: string) => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [picking, setPicking] = useState(false);
  const byId = new Map(notes.map((n) => [n.id, n]));
  // Both directions of disagreement, from one tested function rather than a
  // filter here — the reverse direction (recorded, never cited) had no filter
  // at all before v0.2.1 and so was invisible.
  const found = divergence(citations, inlineRefs);
  const onlyInProse = found.unrecorded;

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
          className="sutra-no-print text-xs text-ink-muted transition-colors hover:text-accent"
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
                      {displayTitle(source.title)}
                    </button>
                  ) : (
                    // No note in the vault carries this id. Named as a state
                    // to get out of rather than as an internal identifier: the
                    // id is on the line below, where it is useful for finding
                    // the file, instead of standing in for the paper's name.
                    <span className="min-w-0 flex-1 truncate text-sm text-highlight">
                      Source note missing
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
                    className="sutra-no-print shrink-0 text-xs text-ink-muted transition-colors hover:text-accent"
                  >
                    ×
                  </button>
                </div>

                {source ? (
                  source.type === "source" ? (
                    <p className="truncate text-xs text-ink-muted">
                      {describe(source.source)}
                    </p>
                  ) : (
                    // Found, and not a source note. Said plainly rather than
                    // reported as missing: the note is right there, and what is
                    // wrong is one field on it. Until v0.4 this read "Source
                    // not in this vault", because resolution consulted the
                    // source list and a filtered list cannot tell a note of
                    // another type apart from no note at all.
                    <p className="text-xs text-highlight">
                      A {typeLabel(source.type)} note, not a source. Set its
                      type to Source so it can carry the paper&rsquo;s details.
                    </p>
                  )
                ) : (
                  // The citation still says what it said — the page and the
                  // quote are in this file, not the missing one, which is the
                  // whole reason they are written here. So the record stays,
                  // and this says what would restore it.
                  <p className="text-xs text-ink-muted">
                    Nothing in this vault has the id{" "}
                    <span className="selectable font-mono">{citation.id}</span>.
                    It may be in <span className="font-mono">.sutra/trash</span>
                    , or on another machine that has not synced yet. What was
                    recorded below is kept either way.
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

      {!isConsistent(found) && (
        <div className="mt-3 rounded-lg border border-accent px-3 py-2">
          <h3 className="text-xs font-semibold tracking-wide text-ink uppercase">
            Citation consistency
          </h3>
          <p className="mt-1 text-xs text-ink-muted">{summarise(found)}</p>
          <p className="mt-1 text-xs text-ink-muted">
            Nothing has been changed. Which of these is wrong is yours to say —
            a paragraph you have not finished writing looks exactly like this.
          </p>
        </div>
      )}

      {found.uncited.length > 0 && (
        <div className="mt-2 rounded-lg border border-border px-3 py-2">
          <p className="text-xs text-ink-muted">
            Recorded here but not cited anywhere in the text, so nothing in the
            note rests on them:
          </p>
          <ul className="mt-1 flex flex-col gap-1">
            {found.uncited.map((ref) => (
              <Uncited key={ref} reference={ref} onOpen={() => onOpen(ref)} />
            ))}
          </ul>
        </div>
      )}

      {onlyInProse.length > 0 && (
        <div className="mt-2 rounded-lg border border-border px-3 py-2">
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
 * A source recorded in frontmatter that no sentence cites.
 *
 * Offers to open it, and nothing else. There is deliberately no "remove"
 * button here: a provenance record carries a page number and a transcribed
 * quote, and a one-click way to delete that because a paragraph is unfinished
 * is precisely the silent loss this release exists to remove. Removing it is
 * already possible above, on the entry itself, where the quote is visible.
 */
function Uncited({
  reference,
  onOpen,
}: {
  reference: string;
  onOpen: () => void;
}) {
  const state = useCitation(reference);
  return (
    <li className="flex items-center justify-between gap-2">
      <CitedName state={state} reference={reference} />
      <button
        type="button"
        onClick={onOpen}
        className="sutra-no-print shrink-0 text-xs text-ink-muted transition-colors hover:text-accent"
      >
        open
      </button>
    </li>
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

  return (
    <li className="flex items-center justify-between gap-2">
      <CitedName state={state} reference={reference} />
      {legacy ? (
        <span className="shrink-0 text-xs text-highlight">
          a Zotero reference — migrate it first
        </span>
      ) : (
        <button
          type="button"
          onClick={onRecord}
          className="sutra-no-print shrink-0 text-xs text-ink-muted transition-colors hover:text-accent"
        >
          record it
        </button>
      )}
    </li>
  );
}

/**
 * What to call a reference in a one-line list.
 *
 * Its title once it resolves. Before that it is still being looked up, and if
 * it never resolves it is missing — which is a state, and says so. It is not
 * called `Reference 01M2…`: a ULID is how the file records the link, not a
 * name for a paper, and putting one where a title goes tells the reader only
 * that something has gone wrong without saying what or what to do.
 *
 * The id is still reachable, as the row's tooltip, because it is what finds
 * the file in the trash or on the other machine.
 */
function CitedName({
  state,
  reference,
}: {
  state: CitationState;
  reference: string;
}) {
  if (state.status === "found") {
    return (
      <span className="min-w-0 flex-1 truncate text-sm text-ink-soft">
        {state.cited.title}
      </span>
    );
  }
  return (
    <span
      className="min-w-0 flex-1 truncate text-sm text-highlight"
      title={reference}
    >
      {state.status === "loading"
        ? "Looking this up…"
        : state.legacy
          ? "Not in the Zotero library"
          : "Source note missing"}
    </span>
  );
}

/** A note type as the type picker writes it, lower-cased to sit in a sentence. */
function typeLabel(type: NoteType): string {
  const known = NOTE_TYPES.find((t) => t.value === type)?.label;
  return (known ?? type).toLowerCase();
}
