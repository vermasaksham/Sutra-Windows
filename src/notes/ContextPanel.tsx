import type { ReactNode } from "react";
import SourcesPanel from "./SourcesPanel";
import BacklinksPanel from "./BacklinksPanel";
import type {
  Backlink,
  Citation,
  NoteSummary,
  RelatedNote,
} from "../vault/api";

/**
 * The fourth column: what is near this note, and why.
 *
 * The brief's question for this space is "why am I looking at this, and what
 * else is near it". Four answers, in order of how directly each is a fact
 * about the note: what it draws on, what points at it, what resembles it, and
 * what it sits beside.
 *
 * Every row in RELATED carries a line saying why it is there. That is the
 * whole design and not a nicety — a ranked list of neighbours with no reasons
 * is one the reader cannot check, so the first time it is wrong they have no
 * way to tell, and after that they stop looking. The reason is the same data
 * the ranking used, computed in Rust so there is one account of it.
 *
 * Hidden when printing, and closable: it is a way of getting somewhere else,
 * and while writing there is nowhere else to get to.
 */
export default function ContextPanel({
  citations,
  sources,
  inlineRefs,
  showSources,
  backlinks,
  related,
  siblings,
  folder,
  onChangeCitations,
  onOpen,
  onClose,
  onReport,
}: {
  citations: Citation[];
  sources: NoteSummary[];
  inlineRefs: string[];
  /** False for a source note, which shows its own paper above the editor. */
  showSources: boolean;
  backlinks: Backlink[];
  related: RelatedNote[];
  siblings: NoteSummary[];
  folder: string;
  onChangeCitations: (citations: Citation[]) => void;
  onOpen: (id: string) => void;
  onClose: () => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  return (
    <aside
      aria-label="Context"
      className="sutra-no-print flex h-full w-context shrink-0 flex-col overflow-y-auto border-l border-border bg-canvas"
    >
      <div className="flex items-center justify-between px-3.5 pt-3 pb-1">
        <p className="text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
          Context
        </p>
        <button
          type="button"
          onClick={onClose}
          aria-label="Hide the context panel"
          className="text-[0.6875rem] text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
        >
          Hide
        </button>
      </div>

      <div className="flex flex-col gap-5 px-3.5 pt-2 pb-6">
        {showSources && (
          <SourcesPanel
            citations={citations}
            sources={sources}
            inlineRefs={inlineRefs}
            onChange={onChangeCitations}
            onOpen={onOpen}
            onReport={onReport}
          />
        )}

        <BacklinksPanel backlinks={backlinks} onSelect={onOpen} />

        <Section
          title="Related"
          count={related.length}
          empty="Nothing resembles this yet. Tags, a shared source or prose about the same thing will bring notes here."
        >
          {related.map((note) => (
            <li key={note.id}>
              <button
                type="button"
                onClick={() => onOpen(note.id)}
                className="w-full rounded-lg border border-border bg-surface px-3 py-2 text-left transition-colors duration-150 ease-out hover:border-accent"
              >
                <span className="block truncate text-sm text-ink">
                  {note.title}
                </span>
                {/*
                  The reason, not a score. A number would say "trust me"; this
                  says something the reader can check in a second, and dismiss
                  in a second if it is wrong.
                */}
                <span className="block truncate text-xs text-ink-muted">
                  {note.reason}
                </span>
              </button>
            </li>
          ))}
        </Section>

        <Section
          // Not the folder's name: uppercased, a path like
          // `RESEARCH/SB2SE3/TRANSPORT` shouts and wraps, and the breadcrumb
          // above the note has already said where this is.
          title={folder === "" ? "At the top level" : "In this folder"}
          count={siblings.length}
          empty="Nothing else in this folder."
        >
          {siblings.map((note) => (
            <li key={note.id}>
              <button
                type="button"
                onClick={() => onOpen(note.id)}
                className="w-full truncate rounded-lg px-2 py-1 text-left text-sm text-ink-soft transition-colors duration-150 ease-out hover:bg-row-hover hover:text-ink"
              >
                {note.title}
              </button>
            </li>
          ))}
        </Section>
      </div>
    </aside>
  );
}

/**
 * One list, always rendered.
 *
 * An absent section reads as "this is broken"; an empty one with a sentence
 * reads as "nothing here yet", which is the actual fact and the thing that
 * tells someone what would put something there.
 */
function Section({
  title,
  count,
  empty,
  children,
}: {
  title: string;
  count: number;
  empty: string;
  children: ReactNode;
}) {
  return (
    <section>
      <h2 className="mb-2 text-xs font-semibold tracking-wide text-ink-muted uppercase">
        {title} {count > 0 && `(${count})`}
      </h2>
      {count === 0 ? (
        <p className="text-sm text-ink-muted">{empty}</p>
      ) : (
        <ul className="flex flex-col gap-1">{children}</ul>
      )}
    </section>
  );
}
