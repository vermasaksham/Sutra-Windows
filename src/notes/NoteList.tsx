import type { NoteSummary } from "../vault/api";

/**
 * A flat list of notes.
 *
 * Deliberately flat. The nested, collapsible page tree is Phase 4, and it needs
 * the SQLite index behind it — `parent` and `position` are already in the
 * frontmatter and already come back from Rust, so this list is the minimum
 * needed to exercise Phase 3 without building ahead.
 */
type Props = {
  notes: NoteSummary[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
};

export default function NoteList({
  notes,
  selectedId,
  onSelect,
  onCreate,
  onDelete,
}: Props) {
  return (
    <nav className="flex h-full w-64 shrink-0 flex-col border-r border-border">
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-xs font-semibold tracking-wide text-ink-muted uppercase">
          Notes
        </span>
        <button
          type="button"
          onClick={onCreate}
          aria-label="New note"
          title="New note"
          className="grid size-6 place-items-center rounded-md text-ink-soft transition-colors duration-150 ease-out hover:bg-accent-bg hover:text-accent"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.75}
            strokeLinecap="round"
            className="size-4"
            aria-hidden
          >
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
      </div>

      {notes.length === 0 ? (
        <p className="px-3 py-2 text-sm text-ink-muted">
          No notes yet. Create one to begin.
        </p>
      ) : (
        <ul className="flex-1 overflow-y-auto px-1.5 pb-2">
          {notes.map((note) => {
            const active = note.id === selectedId;
            return (
              // `group` so the delete control only appears on approach.
              <li key={note.id} className="group relative">
                <button
                  type="button"
                  onClick={() => onSelect(note.id)}
                  className={[
                    "w-full truncate rounded-md py-1.5 pr-7 pl-2 text-left text-sm transition-colors duration-150 ease-out",
                    active
                      ? "bg-accent-bg text-accent"
                      : "text-ink-soft hover:bg-surface hover:text-ink",
                  ].join(" ")}
                >
                  {note.icon ? `${note.icon} ` : ""}
                  {note.title || "Untitled"}
                </button>
                <button
                  type="button"
                  onClick={() => onDelete(note.id)}
                  aria-label={`Move ${note.title || "Untitled"} to trash`}
                  className="absolute top-1/2 right-1 grid size-5 -translate-y-1/2 place-items-center rounded text-ink-muted opacity-0 transition-opacity duration-150 ease-out group-hover:opacity-100 hover:text-highlight focus-visible:opacity-100"
                >
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={1.75}
                    strokeLinecap="round"
                    className="size-3.5"
                    aria-hidden
                  >
                    <path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13" />
                  </svg>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </nav>
  );
}
