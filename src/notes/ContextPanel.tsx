import type { ReactNode } from "react";
import SourcesPanel from "./SourcesPanel";
import BacklinksPanel from "./BacklinksPanel";
import AiPanel from "./AiPanel";
import type {
  Backlink,
  Citation,
  Disagreement,
  Duplicate,
  NoteSummary,
  RelatedNote,
} from "../vault/api";

/**
 * The fourth column: what is near this note, and why.
 *
 * The brief's question for this space is "why am I looking at this, and what
 * else is near it". The answers run in order of how directly each is a fact
 * about the note: what it draws on, what points at it, what may be it written
 * twice, what disagrees with it arithmetically, what resembles it, and what it
 * sits beside.
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
  duplicates,
  disagreements,
  related,
  siblings,
  folder,
  title,
  body,
  aiReady,
  aiWanted,
  onChangeCitations,
  onOpen,
  onCompare,
  onAcceptText,
  onAcceptTags,
  onOpenAiSettings,
  onClose,
  onReport,
}: {
  citations: Citation[];
  sources: NoteSummary[];
  inlineRefs: string[];
  /** False for a source note, which shows its own paper above the editor. */
  showSources: boolean;
  backlinks: Backlink[];
  duplicates: Duplicate[];
  disagreements: Disagreement[];
  related: RelatedNote[];
  siblings: NoteSummary[];
  folder: string;
  /** The open note, as it is on screen, for the assistant to be asked about. */
  title: string;
  body: string;
  aiReady: boolean;
  /** The setting, so the panel can tell "off" from "on but not set up". */
  aiWanted: boolean;
  onChangeCitations: (citations: Citation[]) => void;
  onOpen: (id: string) => void;
  /** Open the side-by-side comparison for a candidate duplicate. */
  onCompare: (id: string, reason: string) => void;
  onAcceptText: (text: string) => void;
  onAcceptTags: (tags: string[]) => void;
  onOpenAiSettings: () => void;
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

        {duplicates.length > 0 && (
          <section>
            <h2 className="mb-2 text-xs font-semibold tracking-wide text-highlight uppercase">
              Possibly the same note
            </h2>
            <ul className="flex flex-col gap-1">
              {duplicates.map((note) => (
                <li key={note.id}>
                  <button
                    type="button"
                    onClick={() => onCompare(note.id, note.reason)}
                    className="w-full rounded-lg border border-highlight/40 bg-highlight-bg px-3 py-2 text-left transition-colors duration-150 ease-out hover:border-highlight"
                  >
                    <span className="block truncate text-sm text-ink">
                      {note.title}
                    </span>
                    <span className="block truncate text-xs text-ink-muted">
                      {note.reason} — compare them
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}

        {disagreements.length > 0 && (
          <section>
            {/*
              "Differ", not "contradict". Detecting that two passages of prose
              disagree is a research problem; detecting that two numbers
              written as the same quantity in the same unit differ is
              arithmetic, and only the arithmetic is shipped. Which is right —
              or whether they are even about the same measurement — is not
              knowable from the text and is not claimed.
            */}
            <h2 className="mb-2 text-xs font-semibold tracking-wide text-highlight uppercase">
              Numbers that differ
            </h2>
            <ul className="flex flex-col gap-1">
              {disagreements.map((note, i) => (
                // A note can differ from another on more than one quantity, so
                // the id alone is not a key.
                <li key={`${note.id}:${note.label}:${i}`}>
                  <button
                    type="button"
                    onClick={() => onOpen(note.id)}
                    className="w-full rounded-lg border border-border bg-surface px-3 py-2 text-left transition-colors duration-150 ease-out hover:border-accent"
                  >
                    <span className="block truncate font-mono text-xs text-ink">
                      {note.here}
                    </span>
                    <span className="block truncate font-mono text-xs text-ink">
                      {note.there}
                    </span>
                    <span className="block truncate text-xs text-ink-muted">
                      {round(note.factor)}× apart, in {note.title}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}

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

        {/*
          Last, and below everything computed. The order is the argument: what
          the vault can prove comes first, and the part that cannot show its
          working comes after it.
        */}
        <AiPanel
          ready={aiReady}
          wanted={aiWanted}
          title={title}
          body={body}
          onAcceptText={onAcceptText}
          onAcceptTags={onAcceptTags}
          onOpenSettings={onOpenAiSettings}
          onReport={onReport}
        />
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

/**
 * A ratio, said the way a person would say it.
 *
 * "10×" rather than "10.0×", and "1.5×" rather than "2×" — rounding a factor
 * to a whole number would make a claim about how far apart two values are that
 * the arithmetic does not support.
 */
function round(factor: number): string {
  return factor >= 10
    ? String(Math.round(factor))
    : factor.toFixed(1).replace(/\.0$/, "");
}
