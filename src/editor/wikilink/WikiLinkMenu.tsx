import { useEffect, useImperativeHandle, useRef, useState } from "react";
import type { Ref } from "react";

export type WikiLinkItem = { id: string; title: string };

export type WikiLinkMenuHandle = {
  onKeyDown: (event: KeyboardEvent) => boolean;
};

type Props = {
  items: WikiLinkItem[];
  onSelect: (item: WikiLinkItem) => void;
  ref?: Ref<WikiLinkMenuHandle>;
};

export default function WikiLinkMenu({ items, onSelect, ref }: Props) {
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
      // Enter and Tab both accept — Tab is what people reach for in an
      // autocomplete, and Enter is what they reach for in a list.
      if (event.key === "Enter" || event.key === "Tab") {
        const item = items[selected];
        if (item) onSelect(item);
        return true;
      }
      return false;
    },
  }));

  if (items.length === 0) {
    return (
      <div className="w-72 rounded-xl border border-border bg-surface px-3 py-2.5 text-sm text-ink-muted shadow-pane">
        No matching note
      </div>
    );
  }

  return (
    <div
      ref={listRef}
      role="listbox"
      aria-label="Link to note"
      className="max-h-72 w-72 overflow-y-auto rounded-xl border border-border bg-surface p-1 shadow-pane"
    >
      {items.map((item, index) => {
        const isSelected = index === selected;
        return (
          <button
            key={item.id}
            type="button"
            role="option"
            aria-selected={isSelected}
            data-selected={isSelected}
            onMouseEnter={() => setSelected(index)}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => onSelect(item)}
            className={[
              "block w-full truncate rounded-lg px-2 py-1.5 text-left text-sm transition-colors",
              isSelected ? "bg-accent-bg text-accent" : "text-ink",
            ].join(" ")}
          >
            {item.title || "Untitled"}
          </button>
        );
      })}
    </div>
  );
}
