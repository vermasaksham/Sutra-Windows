import { useEffect, useState } from "react";
import { tagsApi, type TagChange, type TagSuggestion } from "../vault/api";

/**
 * Renaming, merging and tidying tags across the whole vault.
 *
 * Section 11 asks for control over a tag list without letting it explode, and
 * the rule running through this whole dialog is that nothing here happens
 * without being asked for. Suggestions are questions. A rename that would merge
 * says so before it runs. And every operation can be put back, because a merge
 * loses information and "just rename it back" would not restore it.
 */
export default function TagManager({
  onClose,
  onChanged,
  onReport,
}: {
  onClose: () => void;
  onChanged: () => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [suggestions, setSuggestions] = useState<TagSuggestion[]>([]);
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  /** The last operation, kept only so it can be undone. */
  const [undo, setUndo] = useState<{
    label: string;
    changed: TagChange[];
  } | null>(null);

  const load = () => {
    tagsApi
      .list()
      .then(setCounts)
      .catch(() => setCounts({}));
    tagsApi
      .similar()
      .then(setSuggestions)
      .catch(() => setSuggestions([]));
  };
  useEffect(load, []);

  async function apply(from: string, to: string, label: string) {
    setBusy(true);
    try {
      const result = await tagsApi.retag(from, to);
      setUndo({ label, changed: result.changed });
      setEditing(null);
      load();
      onChanged();
    } catch (cause) {
      onReport("Could not rename the tag", cause);
    } finally {
      setBusy(false);
    }
  }

  async function revert() {
    if (!undo) return;
    setBusy(true);
    try {
      await tagsApi.undo(undo.changed);
      setUndo(null);
      load();
      onChanged();
    } catch (cause) {
      onReport("Could not undo", cause);
    } finally {
      setBusy(false);
    }
  }

  const tags = Object.entries(counts).sort(
    ([aTag, aCount], [bTag, bCount]) =>
      bCount - aCount || aTag.localeCompare(bTag),
  );

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Manage tags"
      className="sutra-no-print fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="flex max-h-[80vh] w-full max-w-2xl flex-col gap-3 rounded-xl border border-border bg-surface p-5 shadow-pane"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-baseline justify-between gap-3">
          <h2 className="text-lg font-semibold text-ink">Tags</h2>
          <span className="text-sm text-ink-muted">
            {tags.length} in this vault
          </span>
        </div>

        {undo && (
          <div className="flex items-center justify-between gap-3 rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
            <span className="min-w-0 truncate">
              {undo.label} — {undo.changed.length} note
              {undo.changed.length === 1 ? "" : "s"} changed.
            </span>
            <button
              type="button"
              onClick={() => void revert()}
              disabled={busy}
              className="shrink-0 rounded-md border border-current px-2 py-0.5 text-xs disabled:opacity-50"
            >
              Undo
            </button>
          </div>
        )}

        {suggestions.length > 0 && (
          <div className="rounded-lg border border-border">
            <p className="border-b border-border px-3 py-1.5 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
              Possibly the same tag
            </p>
            <ul>
              {suggestions.map((s) => (
                <li
                  key={`${s.from}->${s.into}`}
                  className="flex items-center justify-between gap-3 border-b border-border px-3 py-2 last:border-0"
                >
                  {/*
                    The arrow, not "and": a merge has a direction, the rarer
                    tag disappears, and the row has to say which one that is
                    before the button is pressed.
                  */}
                  <span className="min-w-0 text-sm">
                    <span className="text-ink">#{s.from}</span>
                    <span className="text-ink-muted"> ({s.fromCount})</span>
                    <span className="text-ink-muted" aria-label="becomes">
                      {" → "}
                    </span>
                    <span className="text-ink">#{s.into}</span>
                    <span className="text-ink-muted"> ({s.intoCount})</span>
                    <span className="block text-xs text-ink-muted">
                      {s.reason}
                    </span>
                  </span>
                  <button
                    type="button"
                    disabled={busy}
                    title={`Merge #${s.from} into #${s.into}`}
                    aria-label={`Merge #${s.from} into #${s.into}`}
                    onClick={() =>
                      void apply(
                        s.from,
                        s.into,
                        `Merged #${s.from} into #${s.into}`,
                      )
                    }
                    className="shrink-0 rounded-md border border-border px-2 py-1 text-xs text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
                  >
                    Merge
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto rounded-lg border border-border">
          {tags.length === 0 ? (
            <p className="px-3 py-2 text-sm text-ink-muted">
              No tags yet. Add one to a note and it appears here.
            </p>
          ) : (
            <ul>
              {tags.map(([tag, count]) => (
                <li
                  key={tag}
                  className="flex items-center gap-3 border-b border-border px-3 py-1.5 last:border-0"
                >
                  {editing === tag ? (
                    <form
                      className="flex flex-1 items-center gap-2"
                      onSubmit={(event) => {
                        event.preventDefault();
                        const to = draft.trim();
                        if (!to || to === tag) return setEditing(null);
                        const merging = counts[to] !== undefined;
                        void apply(
                          tag,
                          to,
                          merging
                            ? `Merged #${tag} into #${to}`
                            : `Renamed #${tag} to #${to}`,
                        );
                      }}
                    >
                      <input
                        autoFocus
                        value={draft}
                        onChange={(event) => setDraft(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Escape") setEditing(null);
                        }}
                        aria-label={`New name for ${tag}`}
                        className="min-w-0 flex-1 rounded-md bg-row-hover px-2 py-1 text-sm text-ink outline-none"
                      />
                      {counts[draft.trim()] !== undefined &&
                        draft.trim() !== tag && (
                          <span className="shrink-0 text-xs text-highlight">
                            merges into an existing tag
                          </span>
                        )}
                      <button
                        type="submit"
                        disabled={busy}
                        className="shrink-0 rounded-md bg-accent px-2 py-1 text-xs font-medium text-surface disabled:opacity-50"
                      >
                        Apply
                      </button>
                    </form>
                  ) : (
                    <>
                      <span className="min-w-0 flex-1 truncate text-sm text-ink">
                        <span className="text-highlight">#</span>
                        {tag}
                      </span>
                      <span className="shrink-0 text-xs tabular-nums text-ink-muted">
                        {count}
                      </span>
                      <button
                        type="button"
                        onClick={() => {
                          setEditing(tag);
                          setDraft(tag);
                        }}
                        className="shrink-0 text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
                      >
                        Rename
                      </button>
                    </>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>

        <p className="text-xs text-ink-muted">
          Renaming a tag brings everything beneath it: renaming
          <span className="font-mono"> research/materials </span>
          also moves
          <span className="font-mono"> research/materials/sb2se3</span>.
          Renaming onto a tag that already exists merges the two.
        </p>

        <div className="flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
