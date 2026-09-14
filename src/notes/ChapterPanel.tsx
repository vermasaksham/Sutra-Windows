import { useCallback, useEffect, useState } from "react";
import NotePicker from "./NotePicker";
import { chaptersApi, type ChapterEntry } from "../vault/api";
import { displayTitle } from "./titleText";

/**
 * What a chapter assembles, in order.
 *
 * A chapter is a note whose `sequence:` names other notes; see
 * docs/decisions/0002-chapter-assembly.md for why the order lives there and not
 * in a folder or a query. This panel is the whole of the editing surface for
 * that list: add, remove, reorder, open.
 *
 * It does not edit the notes. Assembly is read-only by decision, so a member's
 * title here is a link into it rather than a field — the note is the one place
 * its text can change, and a second place would need a second answer to what a
 * save conflicts with and whose timestamp moves.
 *
 * A position whose note is gone is shown as a position, not skipped. The chapter
 * still claims that note belongs there, and the choice between dropping the
 * entry and restoring the note from the trash is the author's. A list that
 * quietly renumbered itself would be wrong at exactly the moment somebody is
 * checking whether a chapter is complete.
 */
export default function ChapterPanel({
  id,
  /** Bumped by the caller to force a re-read after something outside changed. */
  revision = 0,
  onOpen,
  onReport,
}: {
  id: string;
  revision?: number;
  onOpen: (id: string) => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [entries, setEntries] = useState<ChapterEntry[]>([]);
  const [adding, setAdding] = useState(false);

  const load = useCallback(() => {
    chaptersApi
      .read(id)
      .then(setEntries)
      .catch((cause) => onReport("Could not read the chapter", cause));
  }, [id, onReport]);

  useEffect(load, [load, revision]);

  /**
   * Write a new order, then show what came back from disk.
   *
   * Not what was sent: the ids are re-resolved on read, so a note deleted since
   * the panel was drawn shows as missing here instead of appearing to be fine
   * until the next reload.
   */
  const commit = async (ids: string[]) => {
    try {
      await chaptersApi.save(id, ids);
      load();
    } catch (cause) {
      onReport("Could not save the chapter's order", cause);
    }
  };

  const ids = entries.map((entry) => entry.id);

  const move = (from: number, to: number) => {
    if (to < 0 || to >= ids.length) return;
    const next = [...ids];
    const held = next[from];
    if (held === undefined) return;
    next.splice(from, 1);
    next.splice(to, 0, held);
    void commit(next);
  };

  const remove = (at: number) =>
    void commit(ids.filter((_, index) => index !== at));

  return (
    <section aria-label="Chapter contents">
      <div className="flex items-baseline justify-between">
        <h2 className="text-xs font-semibold tracking-wide text-ink-muted uppercase">
          In this chapter {entries.length > 0 && `(${entries.length})`}
        </h2>
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="sutra-no-print text-xs text-ink-muted transition-colors hover:text-accent"
        >
          + note
        </button>
      </div>

      {entries.length === 0 ? (
        <p className="sutra-no-print mt-2 text-sm text-ink-muted">
          Empty. Add the notes this chapter is made of, in the order you want
          them read — they stay ordinary notes, editable where they live, and
          exporting the chapter writes them out in this order.
        </p>
      ) : (
        <ol className="mt-2 flex flex-col gap-1">
          {entries.map((entry, index) => (
            <li
              key={`${entry.id}:${index}`}
              className="flex items-baseline gap-2 rounded-lg border border-border bg-surface px-3 py-2"
            >
              <span className="shrink-0 font-mono text-xs text-ink-muted tabular-nums">
                {index + 1}.
              </span>
              {entry.note ? (
                <button
                  type="button"
                  onClick={() => onOpen(entry.id)}
                  className="min-w-0 flex-1 truncate text-left text-sm text-accent"
                >
                  {displayTitle(entry.note.title)}
                </button>
              ) : (
                // Deleted, or not synced to this machine yet. Named rather than
                // dropped: the chapter says this note belongs here, and only the
                // author can decide between removing it and restoring the note.
                <span
                  className="min-w-0 flex-1 truncate text-sm text-highlight"
                  title={entry.id}
                >
                  Note not in this vault
                </span>
              )}
              <span className="sutra-no-print flex shrink-0 items-baseline gap-1">
                <button
                  type="button"
                  onClick={() => move(index, index - 1)}
                  disabled={index === 0}
                  aria-label={`Move ${entry.note ? displayTitle(entry.note.title) : entry.id} up`}
                  className="px-1 text-xs text-ink-muted transition-colors not-disabled:hover:text-accent disabled:opacity-30"
                >
                  ↑
                </button>
                <button
                  type="button"
                  onClick={() => move(index, index + 1)}
                  disabled={index === entries.length - 1}
                  aria-label={`Move ${entry.note ? displayTitle(entry.note.title) : entry.id} down`}
                  className="px-1 text-xs text-ink-muted transition-colors not-disabled:hover:text-accent disabled:opacity-30"
                >
                  ↓
                </button>
                <button
                  type="button"
                  onClick={() => remove(index)}
                  aria-label={`Remove ${entry.note ? displayTitle(entry.note.title) : entry.id} from the chapter`}
                  className="px-1 text-xs text-ink-muted transition-colors hover:text-highlight"
                >
                  ×
                </button>
              </span>
            </li>
          ))}
        </ol>
      )}

      {adding && (
        <NotePicker
          // The chapter itself is not offered: a chapter that contained itself
          // would export forever.
          exclude={[id]}
          onClose={() => setAdding(false)}
          onPick={(note) => {
            setAdding(false);
            void commit([...ids, note.id]);
          }}
          onReport={onReport}
        />
      )}
    </section>
  );
}
