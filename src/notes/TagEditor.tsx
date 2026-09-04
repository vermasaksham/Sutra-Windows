import { useState } from "react";
import { suggestTags } from "./tags";

/**
 * The note's tags, as chips with an inline input.
 *
 * Tags are normalised in Rust — trimmed, lowercased, hierarchy split on
 * slashes, de-duplicated — so the same tag typed two ways is one tag. This
 * component does not second-guess that; it sends what was typed and renders
 * what comes back.
 *
 * What it does do is offer the tags that already exist while you type, ignoring
 * punctuation, so that reaching for `thermalcond` puts `thermal-conductivity`
 * in front of you. Preventing the second spelling is worth more than detecting
 * it a month later.
 */
export default function TagEditor({
  tags,
  all,
  onChange,
  onSelect,
}: {
  tags: string[];
  /** Every tag already in the vault, for the suggestions. */
  all: string[];
  onChange: (tags: string[]) => void;
  onSelect: (tag: string) => void;
}) {
  const [draft, setDraft] = useState("");
  const [adding, setAdding] = useState(false);
  const [highlighted, setHighlighted] = useState(0);

  const suggestions = adding ? suggestTags(draft, all, tags) : [];

  function add(value = draft) {
    const trimmed = value.trim();
    setDraft("");
    setAdding(false);
    setHighlighted(0);
    if (trimmed) onChange([...tags, trimmed]);
  }

  return (
    <div className="mb-4 flex flex-wrap items-center gap-1.5">
      {tags.map((tag) => (
        <span
          key={tag}
          className="group/tag inline-flex items-center rounded-full bg-highlight-bg text-highlight"
        >
          <button
            type="button"
            onClick={() => onSelect(tag)}
            title={`Find notes tagged ${tag}`}
            className="py-0.5 pr-1 pl-2.5 text-xs"
          >
            {tag}
          </button>
          <button
            type="button"
            onClick={() => onChange(tags.filter((t) => t !== tag))}
            aria-label={`Remove tag ${tag}`}
            className="pr-2 pl-0.5 text-xs opacity-0 transition-opacity group-hover/tag:opacity-100 focus-visible:opacity-100"
          >
            ×
          </button>
        </span>
      ))}

      {adding ? (
        <span className="relative">
          <input
            autoFocus
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
              setHighlighted(0);
            }}
            // A short delay, or clicking a suggestion would blur the input and
            // commit the half-typed draft before the click ever lands.
            onBlur={() => window.setTimeout(() => add(), 120)}
            onKeyDown={(e) => {
              // Comma as well as Enter: people type tag lists with commas out
              // of habit, and rejecting that would be a small daily annoyance.
              if (e.key === "Enter" || e.key === ",") {
                e.preventDefault();
                add(suggestions[highlighted] ?? draft);
              } else if (e.key === "Tab" && suggestions.length > 0) {
                // Tab completes to the highlighted suggestion without
                // committing, so it can still be refined.
                e.preventDefault();
                setDraft(suggestions[highlighted] ?? draft);
              } else if (e.key === "ArrowDown" && suggestions.length > 0) {
                e.preventDefault();
                setHighlighted((i) => (i + 1) % suggestions.length);
              } else if (e.key === "ArrowUp" && suggestions.length > 0) {
                e.preventDefault();
                setHighlighted(
                  (i) => (i - 1 + suggestions.length) % suggestions.length,
                );
              } else if (e.key === "Escape") {
                e.preventDefault();
                setDraft("");
                setAdding(false);
              }
            }}
            placeholder="tag"
            aria-label="New tag"
            aria-autocomplete="list"
            size={Math.max(draft.length || 3, 3)}
            className="rounded-full border border-border bg-transparent px-2 py-0.5 text-xs text-ink outline-none placeholder:text-ink-muted"
          />
          {suggestions.length > 0 && (
            <ul className="absolute top-full left-0 z-30 mt-1 w-56 overflow-hidden rounded-lg border border-border bg-surface p-1 shadow-pane">
              {suggestions.map((suggestion, index) => (
                <li key={suggestion}>
                  <button
                    type="button"
                    onMouseDown={(event) => {
                      // mousedown, not click: the input's blur would otherwise
                      // fire first and commit the draft instead.
                      event.preventDefault();
                      add(suggestion);
                    }}
                    onMouseEnter={() => setHighlighted(index)}
                    className={[
                      "w-full truncate rounded-md px-2 py-1 text-left text-xs transition-colors",
                      index === highlighted
                        ? "bg-row-active text-accent"
                        : "text-ink-soft",
                    ].join(" ")}
                  >
                    {suggestion}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </span>
      ) : (
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="sutra-no-print rounded-full border border-border px-2 py-0.5 text-xs text-ink-muted transition-colors hover:border-accent hover:text-accent"
        >
          + tag
        </button>
      )}
    </div>
  );
}
