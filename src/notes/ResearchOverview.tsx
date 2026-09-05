import { useEffect, useMemo, useState } from "react";
import { overviewApi, type Overview } from "../vault/api";
import { voiceOf } from "../editor/voices/voiceRules";

/**
 * What the vault knows about where the research has got to.
 *
 * Deliberately not an analysis. Everything here is counted, not judged: which
 * questions you wrote and have not answered, which papers you brought in and
 * never cited, how much of your evidence carries a page and the source's own
 * words. Ranking the questions would be inventing a judgement the app cannot
 * make, and the whole point of the three voices is that the judgement is
 * yours.
 *
 * The voice of each heading is decided by `voiceOf`, the same function the
 * editor uses. Rust gathers the headings and does not classify them, so there
 * is exactly one definition of what counts as a question.
 */
export default function ResearchOverview({
  onClose,
  onOpen,
}: {
  onClose: () => void;
  onOpen: (id: string) => void;
}) {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void overviewApi
      .read()
      .then((o) => live && setOverview(o))
      .catch(
        (e) => live && setFailed(e instanceof Error ? e.message : String(e)),
      );
    return () => {
      live = false;
    };
  }, []);

  const summary = useMemo(() => {
    if (!overview) return null;
    const questions = overview.headings.filter(
      (h) => voiceOf(h.text) === "question",
    );
    return {
      // Unanswered first — a question with prose under it has been worked on.
      open: questions.filter((q) => q.words === 0),
      started: questions.filter((q) => q.words > 0),
      interpretations: overview.headings.filter(
        (h) => voiceOf(h.text) === "interpretation" && h.words > 0,
      ),
      uncited: overview.sources.filter((s) => !overview.citations[s.id]),
      cited: overview.sources.filter((s) => overview.citations[s.id]),
    };
  }, [overview]);

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/20 p-6"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label="Research overview"
        onClick={(e) => e.stopPropagation()}
        className="flex max-h-full w-full max-w-2xl flex-col gap-4 overflow-y-auto rounded-xl border border-border bg-surface p-5 shadow-pane"
      >
        <div className="flex items-baseline justify-between gap-3">
          <h2 className="text-lg font-semibold text-ink">Where the work is</h2>
          <button
            type="button"
            onClick={onClose}
            className="text-sm text-ink-muted transition-colors hover:text-accent"
          >
            Close
          </button>
        </div>

        {failed && <p className="text-sm text-highlight">{failed}</p>}
        {!overview && !failed && (
          <p className="text-sm text-ink-muted">Reading the vault…</p>
        )}

        {summary && overview && (
          <>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <Stat n={summary.open.length} label="questions unanswered" />
              <Stat n={summary.uncited.length} label="sources never cited" />
              <Stat n={overview.withPage} label="citations with a page" />
              <Stat n={overview.withQuote} label="with their words" />
            </div>

            <Group
              title="Questions you have not answered yet"
              empty="No open questions. Either everything is answered, or nothing has been asked."
              items={summary.open.map((q) => ({
                id: q.note,
                primary: q.text,
                secondary: q.noteTitle,
              }))}
              onOpen={onOpen}
            />

            <Group
              title="Questions you have started on"
              empty="Nothing in progress."
              items={summary.started.map((q) => ({
                id: q.note,
                primary: q.text,
                secondary: `${q.noteTitle} · ${q.words} words`,
              }))}
              onOpen={onOpen}
            />

            <Group
              title="Sources nothing cites"
              empty="Every source you have imported is cited somewhere."
              items={summary.uncited.map((s) => ({
                id: s.id,
                primary: s.title,
                secondary: s.source?.authors ?? "",
              }))}
              onOpen={onOpen}
            />

            <p className="text-xs text-ink-muted">
              Counted from your vault, nothing else. A question is a heading the
              editor reads as one; a source is cited when a note names it.
            </p>
          </>
        )}
      </div>
    </div>
  );
}

function Stat({ n, label }: { n: number; label: string }) {
  return (
    <div className="rounded-lg border border-border px-3 py-2">
      <p className="text-xl font-semibold tabular-nums text-ink">{n}</p>
      <p className="text-xs text-ink-muted">{label}</p>
    </div>
  );
}

function Group({
  title,
  empty,
  items,
  onOpen,
}: {
  title: string;
  empty: string;
  items: Array<{ id: string; primary: string; secondary: string }>;
  onOpen: (id: string) => void;
}) {
  return (
    <section className="flex flex-col gap-1 border-t border-border pt-3">
      <h3 className="text-xs font-semibold tracking-wide text-ink-soft uppercase">
        {title}
      </h3>
      {items.length === 0 ? (
        <p className="text-sm text-ink-muted">{empty}</p>
      ) : (
        <ul className="flex flex-col">
          {items.slice(0, 50).map((item, i) => (
            <li key={`${item.id}:${i}`}>
              <button
                type="button"
                onClick={() => onOpen(item.id)}
                className="w-full rounded px-1 py-1 text-left transition-colors hover:bg-row-hover"
              >
                <span className="block text-sm text-ink">{item.primary}</span>
                {item.secondary && (
                  <span className="block truncate text-xs text-ink-muted">
                    {item.secondary}
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}
      {items.length > 50 && (
        <p className="text-xs text-ink-muted">…and {items.length - 50} more.</p>
      )}
    </section>
  );
}
