import { useEffect, useState } from "react";
import { duplicatesApi, notesApi, type NoteDoc } from "../vault/api";

/**
 * Two notes side by side, and three things a person can do about them.
 *
 * Nothing is decided for them. A duplicate suggestion is a guess, merging is
 * destructive, and being wrong about it costs work that cannot be typed again
 * — so the whole design is: show both notes in full, say plainly what merging
 * would do, and wait.
 *
 * The three buttons are the three real answers. **They are different notes**
 * is recorded in both files, so the pair is never offered again and the fact
 * survives the index being deleted. **Merge** folds one into the other and
 * sends the absorbed note to the trash, where it can be fetched back by hand.
 * **Not now** does nothing at all, because "I will decide later" has to be
 * available or the dialog becomes a thing to escape rather than to use.
 */
export default function DuplicateReview({
  left,
  right,
  reason,
  onClose,
  onMerged,
  onDismissed,
  onReport,
}: {
  left: string;
  right: string;
  reason: string;
  onClose: () => void;
  onMerged: (kept: string) => void;
  onDismissed: () => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [notes, setNotes] = useState<[NoteDoc, NoteDoc] | null>(null);
  const [busy, setBusy] = useState(false);
  /** Which of the two would be kept. Merging is directional and asymmetric. */
  const [keep, setKeep] = useState<string>(left);

  useEffect(() => {
    let cancelled = false;
    Promise.all([notesApi.read(left), notesApi.read(right)])
      .then((pair) => {
        if (!cancelled) setNotes(pair);
      })
      .catch((cause) => {
        if (!cancelled) onReport("Could not read the notes", cause);
      });
    return () => {
      cancelled = true;
    };
  }, [left, right, onReport]);

  async function act(run: () => Promise<void>, failure: string) {
    setBusy(true);
    try {
      await run();
    } catch (cause) {
      onReport(failure, cause);
    } finally {
      setBusy(false);
    }
  }

  const absorb = keep === left ? right : left;
  const keptTitle = notes?.find((n) => n.id === keep)?.title ?? "";
  const absorbedTitle = notes?.find((n) => n.id === absorb)?.title ?? "";

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Compare two notes"
      className="sutra-no-print fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
    >
      <div className="flex max-h-[86vh] w-full max-w-4xl flex-col gap-3 rounded-xl border border-border bg-surface p-5 shadow-pane">
        <div>
          <h2 className="text-lg font-semibold text-ink">
            Are these the same note?
          </h2>
          <p className="text-sm text-ink-muted">
            {/*
              The reason is written lowercase for the panel, where it sits
              under a title as a fragment. Here it opens a sentence.
            */}
            {reason.charAt(0).toUpperCase() + reason.slice(1)}. Nothing has been
            changed.
          </p>
        </div>

        {notes === null ? (
          <p className="text-sm text-ink-muted">Reading both…</p>
        ) : (
          <div className="grid min-h-0 flex-1 gap-3 overflow-y-auto sm:grid-cols-2">
            {notes.map((note) => (
              <Side
                key={note.id}
                note={note}
                kept={keep === note.id}
                onKeep={() => setKeep(note.id)}
              />
            ))}
          </div>
        )}

        {notes !== null && (
          <p className="rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
            Merging appends <strong>{absorbedTitle}</strong> to{" "}
            <strong>{keptTitle}</strong> under a heading, moves its tags and
            sources across, repoints every link that pointed at it, and puts it
            in the vault&rsquo;s trash. Nothing is deleted outright.
          </p>
        )}

        <div className="flex flex-wrap justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
          >
            Not now
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              void act(async () => {
                await duplicatesApi.dismiss(left, right);
                onDismissed();
              }, "Could not record that")
            }
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
          >
            They are different notes
          </button>
          <button
            type="button"
            disabled={busy || notes === null}
            onClick={() =>
              void act(async () => {
                await duplicatesApi.merge(keep, absorb);
                onMerged(keep);
              }, "Could not merge the notes")
            }
            className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-surface transition-opacity duration-150 ease-out hover:opacity-90 disabled:opacity-50"
          >
            {busy ? "Working…" : `Merge into ${keptTitle || "this one"}`}
          </button>
        </div>
      </div>
    </div>
  );
}

/** One of the two notes, in full, and the choice of which survives a merge. */
function Side({
  note,
  kept,
  onKeep,
}: {
  note: NoteDoc;
  kept: boolean;
  onKeep: () => void;
}) {
  return (
    <div
      className={[
        "flex min-h-0 flex-col rounded-lg border p-3 transition-colors duration-150 ease-out",
        kept ? "border-accent" : "border-border",
      ].join(" ")}
    >
      <label className="flex items-center gap-2 text-sm">
        <input
          type="radio"
          name="sutra-duplicate-keep"
          checked={kept}
          onChange={onKeep}
          className="accent-accent"
        />
        <span className="min-w-0 flex-1 truncate font-medium text-ink">
          {note.title || "Untitled"}
        </span>
        <span className="shrink-0 text-xs text-ink-muted">
          {kept ? "Keep this one" : "Keep instead"}
        </span>
      </label>
      <p className="mt-0.5 truncate text-xs text-ink-muted">
        {note.folder || "Top level"} · {note.updated.slice(0, 10)}
        {note.tags.length > 0 &&
          ` · ${note.tags.map((t) => `#${t}`).join(" ")}`}
      </p>
      {/*
        The whole body, not an excerpt. Deciding whether two notes are the same
        note is exactly the decision an excerpt cannot support, and this is the
        one screen where that decision is being made.
      */}
      <pre className="mt-2 min-h-0 flex-1 overflow-y-auto text-xs whitespace-pre-wrap text-ink-soft">
        {note.body.trim() || "(empty)"}
      </pre>
    </div>
  );
}
