import type { NoteSummary } from "../vault/api";
import { pathTo } from "./tree";

/**
 * The chain from the root down to the open note.
 *
 * Hidden entirely for a root-level note: a breadcrumb showing only where you
 * already are is noise.
 */
export default function Breadcrumbs({
  notes,
  id,
  onSelect,
}: {
  notes: NoteSummary[];
  id: string | null;
  onSelect: (id: string) => void;
}) {
  const path = pathTo(notes, id);
  if (path.length <= 1) return null;

  return (
    <nav
      aria-label="Breadcrumb"
      className="mb-3 flex flex-wrap items-center gap-1 text-sm"
    >
      {path.slice(0, -1).map((note) => (
        <span key={note.id} className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => onSelect(note.id)}
            className="max-w-48 truncate text-ink-muted transition-colors duration-150 ease-out hover:text-ink"
          >
            {note.title || "Untitled"}
          </button>
          <span aria-hidden className="text-ink-muted">
            /
          </span>
        </span>
      ))}
      <span className="max-w-48 truncate text-ink-soft">
        {path[path.length - 1]?.title || "Untitled"}
      </span>
    </nav>
  );
}
