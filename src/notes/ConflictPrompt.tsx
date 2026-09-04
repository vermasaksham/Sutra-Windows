import type { NoteDoc } from "../vault/api";

/**
 * Shown when a note changed on disk while the buffer had unsaved edits.
 *
 * A clean buffer reloads silently — there is nothing to lose and a dialog would
 * be noise. This only appears when the two versions genuinely disagree and
 * something would be discarded either way, so it does not offer a default:
 * choosing for the user is choosing which of their edits to destroy.
 */
type Props = {
  note: NoteDoc;
  onResolve: (choice: "mine" | "theirs") => void;
};

export default function ConflictPrompt({ note, onResolve }: Props) {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="sutra-conflict-title"
      className="fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
    >
      <div className="flex max-w-md flex-col gap-3 rounded-xl border border-border bg-surface p-5 shadow-pane">
        <h2
          id="sutra-conflict-title"
          className="text-lg font-semibold text-ink"
        >
          “{note.title || "Untitled"}” changed on disk
        </h2>
        <p className="text-sm text-ink-soft">
          Something outside Sutra edited this note while you had unsaved
          changes. Keeping one version discards the other.
        </p>
        <div className="mt-1 flex gap-2">
          <button
            type="button"
            onClick={() => onResolve("mine")}
            className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-surface transition-opacity hover:opacity-90"
          >
            Keep my version
          </button>
          <button
            type="button"
            onClick={() => onResolve("theirs")}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors hover:text-ink"
          >
            Load the file
          </button>
        </div>
        <p className="text-xs text-ink-muted">
          Either way the other version is still in the vault's file history if
          your folder is under version control or a sync service.
        </p>
      </div>
    </div>
  );
}
