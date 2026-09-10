import { useEffect, useState } from "react";
import { notesApi, TYPE_LABELS, type NoteSummary } from "../vault/api";

/**
 * Choosing a note from the vault.
 *
 * Deliberately plain: every note, filtered by title as you type. There is no
 * Zotero half and no import step, because unlike a source, a note being chosen
 * here already exists — the whole point of a chapter is that it assembles work
 * that was written first.
 *
 * The folder is shown under each title rather than the note's excerpt. Two notes
 * in a thesis vault are very often called "Methods", and which folder they sit in
 * is the thing that tells them apart.
 */
export default function NotePicker({
  exclude = [],
  onClose,
  onPick,
  onReport,
}: {
  /** Ids to leave out — a chapter should not be able to contain itself. */
  exclude?: string[];
  onClose: () => void;
  onPick: (note: NoteSummary) => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [query, setQuery] = useState("");
  const [notes, setNotes] = useState<NoteSummary[]>([]);

  useEffect(() => {
    notesApi
      .list()
      .then(setNotes)
      .catch((cause) => onReport("Could not list the notes", cause));
  }, [onReport]);

  const skip = new Set(exclude);
  const needle = query.trim().toLowerCase();
  const matching = notes
    .filter((note) => !skip.has(note.id))
    .filter((note) => !needle || note.title.toLowerCase().includes(needle));

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Add a note"
      className="sutra-no-print fixed inset-0 z-50 flex justify-center bg-canvas/70 px-6 pt-24 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="flex max-h-[60vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-border bg-surface shadow-pane"
        onClick={(event) => event.stopPropagation()}
      >
        <input
          autoFocus
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") onClose();
          }}
          placeholder="Note title"
          aria-label="Find a note"
          className="border-b border-border bg-transparent px-4 py-3 text-ink outline-none placeholder:text-ink-muted"
        />

        <div className="overflow-y-auto p-1">
          {matching.length === 0 ? (
            <p className="px-3 py-2 text-sm text-ink-muted">
              {notes.length === 0 ? "No notes yet." : "Nothing here matches."}
            </p>
          ) : (
            <ul>
              {matching.slice(0, 20).map((note) => (
                <li key={note.id}>
                  <button
                    type="button"
                    onClick={() => onPick(note)}
                    className="w-full rounded-lg px-3 py-1.5 text-left transition-colors hover:bg-row-hover"
                  >
                    <span className="block truncate text-sm text-ink">
                      {note.title}
                    </span>
                    <span className="block truncate text-xs text-ink-muted">
                      {note.folder || "Vault root"}
                      {note.type !== "standard" &&
                        ` · ${TYPE_LABELS[note.type]}`}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
