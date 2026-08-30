import { useState } from "react";

/**
 * The note's tags, as chips with an inline input.
 *
 * Tags are normalised in Rust — trimmed, lowercased, de-duplicated — so the
 * same tag typed two ways is one tag. This component does not second-guess
 * that; it sends what was typed and renders what comes back.
 */
export default function TagEditor({
  tags,
  onChange,
  onSelect,
}: {
  tags: string[];
  onChange: (tags: string[]) => void;
  onSelect: (tag: string) => void;
}) {
  const [draft, setDraft] = useState("");
  const [adding, setAdding] = useState(false);

  function add() {
    const value = draft.trim();
    setDraft("");
    setAdding(false);
    if (value) onChange([...tags, value]);
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
            className="pr-2 pl-0.5 text-xs opacity-0 transition-opacity duration-150 ease-out group-hover/tag:opacity-100 focus-visible:opacity-100"
          >
            ×
          </button>
        </span>
      ))}

      {adding ? (
        <input
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={add}
          onKeyDown={(e) => {
            // Comma as well as Enter: people type tag lists with commas out of
            // habit, and rejecting that would be a small daily annoyance.
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              add();
            }
            if (e.key === "Escape") {
              e.preventDefault();
              setDraft("");
              setAdding(false);
            }
          }}
          placeholder="tag"
          aria-label="New tag"
          size={Math.max(draft.length || 3, 3)}
          className="rounded-full border border-border bg-transparent px-2 py-0.5 text-xs text-ink outline-none placeholder:text-ink-muted"
        />
      ) : (
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="sutra-no-print rounded-full border border-border px-2 py-0.5 text-xs text-ink-muted transition-colors duration-150 ease-out hover:border-accent hover:text-accent"
        >
          + tag
        </button>
      )}
    </div>
  );
}
