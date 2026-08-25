import { useEffect, useRef, useState } from "react";
import { linksAsTitles } from "../editor/wikilink/titleStore";
import { indexApi, type SearchHit } from "../vault/api";

/**
 * Full-text search across the vault, as an overlay.
 *
 * Results come from SQLite's FTS5 with the match already marked up, so the
 * excerpt shows why a note matched rather than just its first line.
 */
export default function SearchPanel({
  onClose,
  onSelect,
}: {
  onClose: () => void;
  onSelect: (id: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => inputRef.current?.focus(), []);

  useEffect(() => {
    // Debounced, so a fast typist does not queue a query per keystroke.
    const timer = setTimeout(() => {
      indexApi
        .search(query)
        .then((results) => {
          setHits(results);
          setSelected(0);
        })
        .catch(() => setHits([]));
    }, 120);
    return () => clearTimeout(timer);
  }, [query]);

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === "Escape") return onClose();
    if (hits.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelected((i) => (i + 1) % hits.length);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((i) => (i - 1 + hits.length) % hits.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const hit = hits[selected];
      if (hit) onSelect(hit.id);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Search notes"
      className="fixed inset-0 z-50 flex justify-center bg-canvas/70 px-6 pt-24 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="flex max-h-[60vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-border bg-surface shadow-lg shadow-black/10"
        // The overlay closes on click; the panel itself must not.
        onClick={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Search the vault"
          aria-label="Search query"
          className="border-b border-border bg-transparent px-4 py-3 text-ink outline-none placeholder:text-ink-muted"
        />

        {query.trim() === "" ? (
          <p className="px-4 py-3 text-sm text-ink-muted">
            Search titles and note text.
          </p>
        ) : hits.length === 0 ? (
          <p className="px-4 py-3 text-sm text-ink-muted">No matches.</p>
        ) : (
          <ul className="overflow-y-auto p-1">
            {hits.map((hit, index) => (
              <li key={hit.id}>
                <button
                  type="button"
                  onMouseEnter={() => setSelected(index)}
                  onClick={() => onSelect(hit.id)}
                  data-selected={index === selected}
                  className={[
                    "w-full rounded-lg px-3 py-2 text-left transition-colors duration-150 ease-out",
                    index === selected ? "bg-accent-bg" : "",
                  ].join(" ")}
                >
                  <span className="block truncate text-sm text-ink">
                    {hit.title || "Untitled"}
                  </span>
                  {/*
                    The excerpt is FTS5's snippet() output, which wraps the
                    matched terms in <mark>. It is generated from the note's own
                    indexed text by SQLite — not user-supplied HTML routed back
                    into the page — and the surrounding text is escaped below.
                  */}
                  <span
                    className="sutra-excerpt block truncate text-xs text-ink-muted"
                    dangerouslySetInnerHTML={{
                      __html: escapeExceptMark(linksAsTitles(hit.excerpt)),
                    }}
                  />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

/**
 * Escape the excerpt, then restore only the `<mark>` tags FTS5 added.
 *
 * Note bodies are arbitrary text and may contain angle brackets — a note about
 * `<script>` tags must not be able to inject one by being searched for.
 */
function escapeExceptMark(excerpt: string): string {
  return excerpt
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/&lt;mark&gt;/g, "<mark>")
    .replace(/&lt;\/mark&gt;/g, "</mark>");
}
