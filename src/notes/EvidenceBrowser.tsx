import { useEffect, useMemo, useState } from "react";
import { evidenceApi, EVIDENCE_KINDS, type EvidenceItem } from "../vault/api";
import { displayTitle } from "./titleText";

/**
 * Everything quoted, apart from the notes quoting it.
 *
 * A retrieval surface, not a second application. It answers the questions a
 * note cannot answer about itself — what have I got from this paper, what rests
 * on this sentence, where did that number come from — and then gets out of the
 * way by opening the note or the paper.
 *
 * Sits where the note list sits, for the same reason the reading pane does: the
 * window already carries four regions and a fifth would need a monitor nobody
 * has. Browsing evidence and reading the note list are not things anyone does
 * at the same moment.
 *
 * Deliberately not editable. Changing a quotation is done where it was
 * recorded, beside the page and the reader's own words, so that editing the
 * paper's words is always a deliberate act in the place that shows what else
 * depends on them.
 */
export default function EvidenceBrowser({
  onOpen,
  onClose,
  onReport,
}: {
  onOpen: (id: string) => void;
  onClose: () => void;
  onReport: (what: string, cause: unknown) => void;
}) {
  const [items, setItems] = useState<EvidenceItem[] | null>(null);
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("");
  const [kind, setKind] = useState("");

  useEffect(() => {
    evidenceApi
      .all()
      .then(setItems)
      .catch((cause) => {
        onReport("Could not read the vault's evidence", cause);
        setItems([]);
      });
    // Once: the browser is opened to look at a state, not to watch it change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Papers that actually have evidence, so the filter never offers an empty
   *  answer. Sorted by title because that is how a person looks for one. */
  const papers = useMemo(() => {
    const seen = new Map<string, string>();
    for (const item of items ?? []) seen.set(item.source, item.source_title);
    return [...seen].sort((a, b) => a[1].localeCompare(b[1]));
  }, [items]);

  /** Kinds present, plus anything a newer build wrote that this one does not
   *  know — the same rule as the picker: kept and shown, never dropped. */
  const kinds = useMemo(() => {
    const present = new Set(
      (items ?? []).map((i) => i.kind).filter((k): k is string => !!k),
    );
    return [
      ...new Set([...EVIDENCE_KINDS.filter((k) => present.has(k)), ...present]),
    ];
  }, [items]);

  const shown = useMemo(() => {
    // Exact text, not fuzzy: the point of a quotation is its words, and a
    // search that matches things you did not write makes it unquotable.
    const needle = query.trim().toLowerCase();
    return (items ?? []).filter((item) => {
      if (source && item.source !== source) return false;
      if (kind && item.kind !== kind) return false;
      if (!needle) return true;
      return (item.quote ?? "").toLowerCase().includes(needle);
    });
  }, [items, query, source, kind]);

  return (
    <div
      className="sutra-no-print flex h-full w-list shrink-0 flex-col border-r border-l border-border bg-canvas"
      aria-label="Evidence"
    >
      <div className="flex items-start justify-between gap-2 px-3 pt-3 pb-2">
        <div className="min-w-0">
          <p className="text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
            Evidence
          </p>
          <p className="truncate text-sm font-semibold text-ink">
            {items === null
              ? "Reading the vault…"
              : `${shown.length} of ${items.length}`}
          </p>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close evidence"
          className="shrink-0 rounded px-1.5 py-0.5 text-xs text-ink-muted transition-colors hover:text-accent"
        >
          ✕
        </button>
      </div>

      <div className="flex flex-col gap-1.5 px-3 pb-2">
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search the exact words"
          aria-label="Search evidence text"
          className="w-full rounded-lg border border-border bg-surface px-2 py-1 text-xs text-ink outline-none placeholder:text-ink-muted"
        />
        <div className="flex gap-1.5">
          <select
            value={source}
            onChange={(event) => setSource(event.target.value)}
            aria-label="Filter by source"
            className="min-w-0 flex-1 rounded border border-border bg-surface px-1 py-0.5 text-xs text-ink"
          >
            <option value="">Every paper</option>
            {papers.map(([id, title]) => (
              <option key={id} value={id}>
                {displayTitle(title)}
              </option>
            ))}
          </select>
          <select
            value={kind}
            onChange={(event) => setKind(event.target.value)}
            aria-label="Filter by kind of evidence"
            className="shrink-0 rounded border border-border bg-surface px-1 py-0.5 text-xs text-ink"
          >
            <option value="">Any kind</option>
            {kinds.map((k) => (
              <option key={k} value={k}>
                {k}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-4">
        {items !== null && items.length === 0 && (
          <p className="mt-2 text-sm text-ink-muted">
            Nothing recorded yet. Reading a paper and capturing a sentence puts
            it here, with the page it came from.
          </p>
        )}
        {items !== null && items.length > 0 && shown.length === 0 && (
          <p className="mt-2 text-sm text-ink-muted">
            No evidence matches. The search is over the source&rsquo;s exact
            words, not your own notes on them.
          </p>
        )}
        <ul className="flex flex-col gap-2">
          {shown.map((item) => (
            <Item key={item.eid} item={item} onOpen={onOpen} />
          ))}
        </ul>
      </div>
    </div>
  );
}

function Item({
  item,
  onOpen,
}: {
  item: EvidenceItem;
  onOpen: (id: string) => void;
}) {
  return (
    <li className="rounded-lg border border-border bg-surface px-3 py-2">
      {item.quote ? (
        <p className="sutra-quote selectable border-l-2 border-highlight pl-2 text-sm text-ink-soft italic">
          {item.quote}
        </p>
      ) : (
        // A reference whose record is gone. Shown rather than hidden: what is
        // worth knowing here is that something was recorded and is not there.
        <p className="text-sm text-highlight">
          The record for this evidence is missing
        </p>
      )}

      <div className="mt-1 flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-xs text-ink-muted">
        <button
          type="button"
          onClick={() => onOpen(item.source)}
          className="min-w-0 truncate text-left text-accent transition-opacity hover:opacity-80"
        >
          {displayTitle(item.source_title)}
        </button>
        {item.page && <span>p. {item.page}</span>}
        {!item.page && item.page_index && <span>PDF p. {item.page_index}</span>}
        {item.kind && <span className="text-highlight">{item.kind}</span>}
        {item.shared && (
          <span title="This record lives on the paper, so more than one note can rest on it">
            shared
          </span>
        )}
      </div>

      {item.used_by.length > 0 ? (
        <ul className="mt-1.5 flex flex-col gap-0.5 border-t border-border pt-1.5">
          {item.used_by.map((use) => (
            <li key={`${item.eid}:${use.note}`}>
              <button
                type="button"
                onClick={() => onOpen(use.note)}
                className="w-full truncate rounded px-1 py-0.5 text-left text-xs text-ink-soft transition-colors hover:bg-row-hover hover:text-ink"
              >
                {displayTitle(use.title)}
                {/* The reader's own words, marked as theirs. Never run
                    together with the quotation above. */}
                {use.comment && (
                  <span className="text-ink-muted"> — {use.comment}</span>
                )}
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="mt-1.5 border-t border-border pt-1.5 text-xs text-ink-muted">
          Nothing rests on this yet.
        </p>
      )}
    </li>
  );
}
