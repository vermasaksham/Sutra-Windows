import { useEffect, useImperativeHandle, useRef, useState } from "react";
import type { Ref } from "react";
import type { SlashItem } from "./items";

export type SlashMenuHandle = {
  /** Returns true when the menu consumed the key, so the editor ignores it. */
  onKeyDown: (event: KeyboardEvent) => boolean;
};

type Props = {
  items: SlashItem[];
  onSelect: (item: SlashItem) => void;
  onDismiss: () => void;
  ref?: Ref<SlashMenuHandle>;
};

export default function SlashMenu({ items, onSelect, onDismiss, ref }: Props) {
  const [selected, setSelected] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  // The item list is re-filtered on every keystroke; keep the highlight in range.
  useEffect(() => setSelected(0), [items]);

  // Keep the highlighted row visible without scrolling the page behind it.
  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>('[data-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  function choose(item: SlashItem) {
    onSelect(item);
  }

  useImperativeHandle(ref, () => ({
    onKeyDown(event) {
      if (event.key === "ArrowDown") {
        setSelected((i) => (i + 1) % Math.max(items.length, 1));
        return true;
      }
      if (event.key === "ArrowUp") {
        setSelected((i) => (i - 1 + items.length) % Math.max(items.length, 1));
        return true;
      }
      if (event.key === "Enter") {
        const item = items[selected];
        if (item) choose(item);
        return true;
      }
      if (event.key === "Escape") {
        onDismiss();
        return true;
      }
      return false;
    },
  }));

  if (items.length === 0) {
    return (
      <div className="w-72 rounded-xl border border-border bg-surface px-3 py-2.5 text-sm text-ink-muted shadow-pane">
        No blocks match
      </div>
    );
  }

  let lastGroup: string | null = null;

  return (
    <div
      ref={listRef}
      role="listbox"
      aria-label="Insert block"
      className="max-h-80 w-72 overflow-y-auto rounded-xl border border-border bg-surface p-1 shadow-pane"
    >
      {items.map((item, index) => {
        const isSelected = index === selected;
        const showGroup = item.group !== lastGroup;
        lastGroup = item.group;
        const Icon = item.icon;

        return (
          <div key={item.id}>
            {showGroup && (
              <div className="px-2 pt-2 pb-1 text-[11px] font-semibold tracking-wide text-ink-muted uppercase">
                {item.group}
              </div>
            )}
            <button
              type="button"
              role="option"
              aria-selected={isSelected}
              data-selected={isSelected}
              // Mouse hover moves the highlight so keyboard and pointer agree.
              onMouseEnter={() => setSelected(index)}
              // The editor still holds focus; taking it here would close the menu.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(item)}
              className={[
                "flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left transition-colors duration-150 ease-out",
                isSelected ? "bg-accent-bg" : "bg-transparent",
              ].join(" ")}
            >
              <span
                className={[
                  "grid size-8 shrink-0 place-items-center rounded-md border border-border",
                  isSelected ? "text-accent" : "text-ink-soft",
                ].join(" ")}
              >
                <Icon className="size-4" />
              </span>
              <span className="min-w-0">
                <span
                  className={[
                    "block truncate text-sm leading-tight",
                    isSelected ? "text-accent" : "text-ink",
                  ].join(" ")}
                >
                  {item.title}
                </span>
                <span className="block truncate text-xs leading-tight text-ink-muted">
                  {item.hint}
                </span>
              </span>
            </button>
          </div>
        );
      })}
    </div>
  );
}
