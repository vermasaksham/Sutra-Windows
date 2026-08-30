import ThemeToggle from "../components/ThemeToggle";
import type { NoteSummary } from "../vault/api";

/**
 * The left rail: what to look at, rather than which note.
 *
 * Bear's shape — a narrow column of collections above a tag list, sitting on
 * the darkest of the three grounds so the list and the page read as stacked in
 * front of it. Sutra's collections are simply "everything" and "notes carrying
 * this tag", because tags are the only cross-cutting axis the frontmatter has.
 */
export default function Sidebar({
  vaultName,
  notes,
  activeTag,
  onSelectTag,
  onNewNote,
}: {
  vaultName: string;
  notes: NoteSummary[];
  activeTag: string | null;
  onSelectTag: (tag: string | null) => void;
  onNewNote: () => void;
}) {
  const counts = tagCounts(notes);

  return (
    <nav
      aria-label="Collections"
      className="flex h-full w-rail shrink-0 flex-col bg-rail"
    >
      <div className="flex items-center gap-2 px-3 pt-3 pb-2">
        <Star />
        <span className="min-w-0 flex-1 truncate text-sm font-semibold text-ink">
          {vaultName}
        </span>
        <button
          type="button"
          onClick={onNewNote}
          aria-label="New note"
          title="New note (Ctrl N)"
          className="grid size-6 shrink-0 place-items-center rounded-md text-ink-muted transition-colors duration-150 ease-out hover:bg-row-hover hover:text-accent"
        >
          <Plus />
        </button>
      </div>

      <div className="px-1.5">
        <Row
          label="All notes"
          count={notes.length}
          active={activeTag === null}
          onClick={() => onSelectTag(null)}
        />
      </div>

      {counts.length > 0 && (
        <>
          <p className="px-3 pt-4 pb-1 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
            Tags
          </p>
          <ul className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
            {counts.map(([tag, count]) => (
              <li key={tag}>
                <Row
                  label={tag}
                  hash
                  count={count}
                  active={activeTag === tag}
                  onClick={() => onSelectTag(activeTag === tag ? null : tag)}
                />
              </li>
            ))}
          </ul>
        </>
      )}

      <div className="mt-auto p-2">
        <ThemeToggle />
      </div>
    </nav>
  );
}

function Row({
  label,
  count,
  active,
  hash,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  hash?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "true" : undefined}
      className={[
        "flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-sm transition-colors duration-150 ease-out",
        active
          ? "bg-row-active font-medium text-accent"
          : "text-ink-soft hover:bg-row-hover hover:text-ink",
      ].join(" ")}
    >
      {hash && (
        <span className="text-highlight" aria-hidden>
          #
        </span>
      )}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <span className="shrink-0 text-xs tabular-nums text-ink-muted">
        {count}
      </span>
    </button>
  );
}

/** Every tag in the vault with how many notes carry it, commonest first. */
function tagCounts(notes: NoteSummary[]): Array<[string, number]> {
  const counts = new Map<string, number>();
  for (const note of notes) {
    for (const tag of note.tags) {
      counts.set(tag, (counts.get(tag) ?? 0) + 1);
    }
  }
  return [...counts].sort(
    ([aTag, aCount], [bTag, bCount]) =>
      bCount - aCount || aTag.localeCompare(bTag),
  );
}

/**
 * The application's mark: the icon's shooting star, reduced to what survives at
 * 16px — a sparkle and three lines of trail behind it. Any more detail turns to
 * mud at this size.
 */
function Star() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="size-4 shrink-0 text-accent"
      aria-hidden
    >
      <path
        fill="currentColor"
        d="M8.6 2.2l1.7 4.1 4.1 1.7-4.1 1.7-1.7 4.1-1.7-4.1L2.8 8l4.1-1.7z"
      />
      <g
        fill="none"
        stroke="currentColor"
        strokeWidth={1.7}
        strokeLinecap="round"
      >
        <path d="M12.4 11.6a10 10 0 0 1 7.4 7.4" />
        <path d="M9.6 14.4a7 7 0 0 1 5.2 5.2" />
        <path d="M6.8 17.2a4 4 0 0 1 3 3" />
      </g>
    </svg>
  );
}

function Plus() {
  return (
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
  );
}
