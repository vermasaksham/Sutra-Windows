import { useEffect, useMemo, useState } from "react";
import {
  CONDITION_KINDS,
  NOTE_TYPES,
  VIEW_SORTS,
  condition,
  conditionKind,
  conditionValue,
  viewsApi,
  type Condition,
  type ConditionKind,
  type NoteSummary,
  type ViewQuery,
  type ViewResult,
  type ViewSort,
} from "../vault/api";

/**
 * Building a saved query, as a form rather than a search box.
 *
 * The query is typed, so the editor can be too: every row is a kind picked
 * from a list and a value, and there is no syntax to get wrong, no operator
 * precedence to explain, and no escaping problem the first time a folder is
 * called `Data [raw]`.
 *
 * The results run live underneath. A query you cannot see the effect of is a
 * query you are guessing at, and guessing is what a form is meant to remove.
 */
export default function ViewEditor({
  title,
  query,
  folders,
  tags,
  sources,
  onCancel,
  onSave,
  onReport,
}: {
  /** The view being edited, or "" for one being made. */
  title: string;
  query: ViewQuery;
  folders: string[];
  tags: string[];
  sources: NoteSummary[];
  onCancel: () => void;
  onSave: (title: string, query: ViewQuery) => Promise<void>;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [name, setName] = useState(title);
  const [draft, setDraft] = useState<ViewQuery>(query);
  const [result, setResult] = useState<ViewResult | null>(null);
  const [saving, setSaving] = useState(false);

  // Run on every change. A view is one indexed query, so this costs less than
  // the keystroke that triggered it and reads no file at all.
  useEffect(() => {
    let cancelled = false;
    viewsApi
      .run(draft)
      .then((found) => {
        if (!cancelled) setResult(found);
      })
      .catch(() => {
        if (!cancelled) setResult(null);
      });
    return () => {
      cancelled = true;
    };
  }, [draft]);

  const setGroup = (group: "all" | "any" | "none", terms: Condition[]) =>
    setDraft((prev) => ({ ...prev, [group]: terms }));

  async function save() {
    setSaving(true);
    try {
      await onSave(name.trim() || "Untitled view", draft);
    } catch (cause) {
      onReport("Could not save the view", cause);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={title ? `Edit view: ${title}` : "New view"}
      className="sutra-no-print fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
    >
      <div className="flex max-h-[86vh] w-full max-w-2xl flex-col gap-3 rounded-xl border border-border bg-surface p-5 shadow-pane">
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="What is this view for?"
          aria-label="View name"
          className="w-full bg-transparent text-lg font-semibold text-ink outline-none placeholder:text-ink-muted"
        />

        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
          <Group
            legend="Matching all of"
            hint="Every one of these must be true."
            terms={draft.all ?? []}
            onChange={(terms) => setGroup("all", terms)}
            {...{ folders, tags, sources }}
          />
          <Group
            legend="And any of"
            hint="At least one of these. Leave empty for no such requirement."
            terms={draft.any ?? []}
            onChange={(terms) => setGroup("any", terms)}
            {...{ folders, tags, sources }}
          />
          <Group
            legend="But none of"
            hint="Anything matching one of these is left out."
            terms={draft.none ?? []}
            onChange={(terms) => setGroup("none", terms)}
            {...{ folders, tags, sources }}
          />

          <div className="flex flex-wrap items-center gap-3 text-sm">
            <label className="flex items-center gap-2 text-ink-soft">
              Order
              <select
                value={draft.sort ?? "recent"}
                onChange={(e) =>
                  setDraft((prev) => ({
                    ...prev,
                    sort: e.target.value as ViewSort,
                  }))
                }
                className="rounded-lg border border-border bg-surface px-2 py-1 text-ink"
              >
                {VIEW_SORTS.map((s) => (
                  <option key={s.value} value={s.value}>
                    {s.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-2 text-ink-soft">
              At most
              <input
                type="number"
                min={1}
                value={draft.limit ?? 200}
                onChange={(e) =>
                  setDraft((prev) => ({
                    ...prev,
                    limit: Math.max(1, Number(e.target.value) || 1),
                  }))
                }
                className="w-20 rounded-lg border border-border bg-surface px-2 py-1 text-ink"
              />
              notes
            </label>
          </div>

          <Preview result={result} />
        </div>

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={saving}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void save()}
            disabled={saving}
            className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-surface transition-opacity duration-150 ease-out hover:opacity-90 disabled:opacity-50"
          >
            {saving ? "Saving…" : title ? "Save view" : "Create view"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** What the query matches right now, with its own sentence above it. */
function Preview({ result }: { result: ViewResult | null }) {
  if (!result) return null;
  return (
    <div className="rounded-lg border border-border">
      <div className="border-b border-border px-3 py-2">
        <p className="text-sm text-ink">{result.description}</p>
        <p className="text-xs text-ink-muted">
          {result.notes.length === 1
            ? "1 note"
            : `${result.notes.length} notes`}
          {result.truncated && " — the limit, so there may be more"}
        </p>
      </div>
      {result.notes.length === 0 ? (
        <p className="px-3 py-2 text-sm text-ink-muted">Nothing matches yet.</p>
      ) : (
        <ul className="max-h-40 overflow-y-auto">
          {result.notes.slice(0, 40).map((note) => (
            <li
              key={note.id}
              className="flex justify-between gap-3 border-b border-border px-3 py-1.5 text-xs last:border-0"
            >
              <span className="min-w-0 truncate text-ink">{note.title}</span>
              <span className="shrink-0 text-ink-muted">
                {note.folder || "Top level"}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** One of the three lists, with its rows. */
function Group({
  legend,
  hint,
  terms,
  onChange,
  folders,
  tags,
  sources,
}: {
  legend: string;
  hint: string;
  terms: Condition[];
  onChange: (terms: Condition[]) => void;
  folders: string[];
  tags: string[];
  sources: NoteSummary[];
}) {
  return (
    <fieldset className="rounded-lg border border-border px-3 py-2">
      <legend className="px-1 text-xs font-medium text-ink-soft">
        {legend}
      </legend>
      {terms.length === 0 && (
        <p className="pb-1 text-xs text-ink-muted">{hint}</p>
      )}
      <ul className="flex flex-col gap-1.5">
        {terms.map((term, i) => (
          // Rows have no identity of their own — two `tag: xrd` rows are the
          // same condition — so the index is the honest key here.
          // eslint-disable-next-line react/no-array-index-key
          <li key={i}>
            <Row
              term={term}
              onChange={(next) =>
                onChange(terms.map((t, j) => (i === j ? next : t)))
              }
              onRemove={() => onChange(terms.filter((_, j) => i !== j))}
              {...{ folders, tags, sources }}
            />
          </li>
        ))}
      </ul>
      <button
        type="button"
        onClick={() => onChange([...terms, { under: "" }])}
        className="mt-1.5 text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
      >
        + Add a condition
      </button>
    </fieldset>
  );
}

/** One condition: which kind, and what it is looking for. */
function Row({
  term,
  onChange,
  onRemove,
  folders,
  tags,
  sources,
}: {
  term: Condition;
  onChange: (term: Condition) => void;
  onRemove: () => void;
  folders: string[];
  tags: string[];
  sources: NoteSummary[];
}) {
  const kind = conditionKind(term);
  const value = conditionValue(term);
  const listId = useMemo(
    () => `sutra-view-options-${Math.random().toString(36).slice(2)}`,
    [],
  );

  // What the value means depends on the kind, so the input changes with it.
  const options =
    kind === "under" || kind === "in"
      ? folders
      : kind === "tag"
        ? tags
        : kind === "cites"
          ? sources.map((s) => s.id)
          : null;

  return (
    <div className="flex items-center gap-2">
      <select
        value={kind}
        onChange={(e) =>
          // Changing the kind clears the value: a folder path is not a date,
          // and carrying it over would produce a condition matching nothing
          // while looking filled in.
          onChange(condition(e.target.value as ConditionKind, ""))
        }
        aria-label="Condition"
        className="shrink-0 rounded-lg border border-border bg-surface px-2 py-1 text-sm text-ink"
      >
        {CONDITION_KINDS.map((c) => (
          <option key={c.kind} value={c.kind}>
            {c.label}
          </option>
        ))}
      </select>

      {kind === "type" ? (
        <select
          value={value}
          onChange={(e) => onChange(condition(kind, e.target.value))}
          aria-label="Note type"
          className="min-w-0 flex-1 rounded-lg border border-border bg-surface px-2 py-1 text-sm text-ink"
        >
          <option value="">Choose a kind…</option>
          {NOTE_TYPES.map((t) => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </select>
      ) : (
        <>
          <input
            value={value}
            onChange={(e) => onChange(condition(kind, e.target.value))}
            list={options ? listId : undefined}
            type={kind.startsWith("updated-") ? "date" : "text"}
            placeholder={
              kind === "in" || kind === "under"
                ? "Folder — empty is the top level"
                : kind === "tag"
                  ? "Tag, without the #"
                  : kind === "text"
                    ? "Words to look for"
                    : "Note id"
            }
            aria-label="Value"
            className="min-w-0 flex-1 rounded-lg border border-border bg-surface px-2 py-1 text-sm text-ink placeholder:text-ink-muted"
          />
          {options && (
            <datalist id={listId}>
              {kind === "cites"
                ? sources.map((s) => (
                    <option key={s.id} value={s.id} label={s.title} />
                  ))
                : options.map((o) => <option key={o} value={o} />)}
            </datalist>
          )}
        </>
      )}

      <button
        type="button"
        onClick={onRemove}
        aria-label="Remove this condition"
        className="shrink-0 rounded px-1 text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
      >
        ×
      </button>
    </div>
  );
}
