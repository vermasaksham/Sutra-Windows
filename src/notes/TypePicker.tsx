import { useEffect, useRef, useState } from "react";
import { NOTE_TYPES, type NoteType } from "../vault/api";

/**
 * What kind of note this is, changeable at any time.
 *
 * A plain note shows nothing but a quiet "Note" chip, because the default
 * should cost no attention. Only a note someone has actually classified wears
 * its type in the accent colour.
 */
export default function TypePicker({
  type,
  onChange,
}: {
  type: NoteType;
  onChange: (type: NoteType) => void;
}) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", dismiss, true);
    return () => document.removeEventListener("mousedown", dismiss, true);
  }, [open]);

  // A hand-edited file can carry a type this build has never heard of, so the
  // label falls back rather than the component crashing.
  const label = NOTE_TYPES.find((t) => t.value === type)?.label ?? "Note";
  const isDefault = type === "standard";

  return (
    <div ref={box} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-label={`Note type: ${label}. Change it.`}
        className={[
          "rounded-full px-2.5 py-0.5 text-xs transition-colors duration-150 ease-out",
          isDefault
            ? "border border-border text-ink-muted hover:border-accent hover:text-accent"
            : "bg-accent-bg font-medium text-accent",
        ].join(" ")}
      >
        {label}
      </button>

      {open && (
        <div className="absolute top-full left-0 z-30 mt-1 w-52 rounded-xl border border-border bg-surface p-1 shadow-pane">
          <ul>
            {NOTE_TYPES.map((option) => (
              <li key={option.value}>
                <button
                  type="button"
                  onClick={() => {
                    onChange(option.value);
                    setOpen(false);
                  }}
                  aria-current={option.value === type ? "true" : undefined}
                  className={[
                    "w-full rounded-lg px-2.5 py-1.5 text-left text-sm transition-colors duration-150 ease-out",
                    option.value === type
                      ? "bg-row-active font-medium text-accent"
                      : "text-ink-soft hover:bg-row-hover hover:text-ink",
                  ].join(" ")}
                >
                  {option.label}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
