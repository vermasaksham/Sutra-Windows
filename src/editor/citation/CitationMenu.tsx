import { useEffect, useImperativeHandle, useRef, useState } from "react";
import type { Ref } from "react";
import type { Reference } from "../../vault/api";

export type CitationMenuHandle = {
  onKeyDown: (event: KeyboardEvent) => boolean;
};

/**
 * Something you can cite.
 *
 * Two kinds, and the difference matters to the reader: a source already in the
 * vault is instant and permanent, while a Zotero item has to be copied in
 * first. Both end up as the same thing — a citation of a source note — so the
 * distinction disappears the moment one is chosen.
 */
export type Candidate =
  | { kind: "source"; id: string; title: string; detail: string }
  | { kind: "zotero"; reference: Reference; title: string; detail: string };

type Props = {
  items: Candidate[];
  state: "idle" | "loading" | "error";
  error: string | null;
  onSelect: (candidate: Candidate) => void;
  ref?: Ref<CitationMenuHandle>;
};

export default function CitationMenu({
  items,
  state,
  error,
  onSelect,
  ref,
}: Props) {
  const [selected, setSelected] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => setSelected(0), [items]);
  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>('[data-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  useImperativeHandle(ref, () => ({
    onKeyDown(event) {
      if (items.length === 0) return false;
      if (event.key === "ArrowDown") {
        setSelected((i) => (i + 1) % items.length);
        return true;
      }
      if (event.key === "ArrowUp") {
        setSelected((i) => (i - 1 + items.length) % items.length);
        return true;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        const item = items[selected];
        if (item) onSelect(item);
        return true;
      }
      return false;
    },
  }));

  const shell = "w-96 rounded-xl border border-border bg-surface shadow-pane";

  // Zotero being switched off is the single most likely reason this is empty,
  // and it is fixable — so say so here rather than showing "no results".
  if (state === "error") {
    return (
      <div className={`${shell} px-3 py-2.5 text-sm text-highlight`}>
        {error ?? "Could not reach Zotero."}
      </div>
    );
  }
  if (state === "loading" && items.length === 0) {
    return (
      <div className={`${shell} px-3 py-2.5 text-sm text-ink-muted`}>
        Searching…
      </div>
    );
  }
  if (items.length === 0) {
    return (
      <div className={`${shell} px-3 py-2.5 text-sm text-ink-muted`}>
        No matching source
      </div>
    );
  }

  return (
    <div
      ref={listRef}
      role="listbox"
      aria-label="Cite a reference"
      className={`${shell} max-h-80 overflow-y-auto p-1`}
    >
      {items.map((item, index) => {
        const isSelected = index === selected;
        // The heading appears once, above the first item of each kind, so the
        // list reads as two places rather than one undifferentiated pile.
        const heading =
          index === 0 || items[index - 1]!.kind !== item.kind
            ? item.kind === "source"
              ? "In this vault"
              : "Zotero"
            : null;
        return (
          <div key={item.kind === "source" ? item.id : item.reference.key}>
            {heading && (
              <p className="px-2 pt-1.5 pb-1 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
                {heading}
              </p>
            )}
            <button
              type="button"
              role="option"
              aria-selected={isSelected}
              data-selected={isSelected}
              onMouseEnter={() => setSelected(index)}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => onSelect(item)}
              className={[
                "block w-full rounded-lg px-2 py-1.5 text-left transition-colors",
                isSelected ? "bg-accent-bg" : "",
              ].join(" ")}
            >
              <span
                className={`block truncate text-sm ${isSelected ? "text-accent" : "text-ink"}`}
              >
                {item.title}
              </span>
              <span className="block truncate text-xs text-ink-muted">
                {item.detail}
              </span>
            </button>
          </div>
        );
      })}
    </div>
  );
}
