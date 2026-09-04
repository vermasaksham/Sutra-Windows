import { useEffect, useRef, useState } from "react";

/**
 * A small set of emoji, chosen for a research vault rather than a chat app.
 *
 * Curated rather than a full picker: a complete emoji index is a dependency
 * and a search box for a decision that takes one glance, and these are the
 * marks a lab notebook actually uses.
 */
const ICONS = [
  "🧪",
  "⚗️",
  "🔬",
  "🧫",
  "🧬",
  "⚛️",
  "🔭",
  "💎",
  "📊",
  "📈",
  "📉",
  "🗂️",
  "📋",
  "📐",
  "🧭",
  "🔎",
  "📝",
  "📌",
  "🔖",
  "⭐",
  "🔥",
  "❄️",
  "⚡",
  "💡",
  "✅",
  "⚠️",
  "❓",
  "🚧",
  "🗒️",
  "📦",
  "🧰",
  "🕯️",
] as const;

export default function IconPicker({
  icon,
  onPick,
  onClose,
}: {
  icon: string | null;
  onPick: (icon: string | null) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [mounted, setMounted] = useState(false);

  useEffect(() => setMounted(true), []);

  // Close on a click anywhere else, and on Escape. Both are what a popover is
  // expected to do, and neither is worth a dependency.
  useEffect(() => {
    function onPointerDown(event: MouseEvent) {
      if (!ref.current?.contains(event.target as Node)) onClose();
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    // Deferred a tick, or the click that opened this closes it again.
    const timer = setTimeout(() =>
      document.addEventListener("mousedown", onPointerDown),
    );
    document.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label="Choose an icon"
      style={{ opacity: mounted ? 1 : 0 }}
      // `top-full left-0` is load-bearing. Without offsets an absolutely
      // positioned element keeps its static position, and as a flex item under
      // `items-center` that means being centred on the icon row — which pulls a
      // 136px-tall popover upward until its first rows sit behind the header,
      // where they cannot be clicked. Anchoring it below the row fixes that.
      className="absolute top-full left-0 z-30 mt-1 w-64 rounded-xl border border-border bg-surface p-2 shadow-pane transition-opacity"
    >
      <div className="grid grid-cols-8 gap-0.5">
        {ICONS.map((candidate) => (
          <button
            key={candidate}
            type="button"
            onClick={() => onPick(candidate)}
            aria-label={`Icon ${candidate}`}
            aria-pressed={icon === candidate}
            className={[
              "grid aspect-square place-items-center rounded-md text-lg transition-colors",
              icon === candidate ? "bg-accent-bg" : "hover:bg-row-hover",
            ].join(" ")}
          >
            {candidate}
          </button>
        ))}
      </div>
      {icon && (
        <button
          type="button"
          onClick={() => onPick(null)}
          className="mt-1 w-full rounded-md px-2 py-1 text-left text-xs text-ink-muted transition-colors hover:text-highlight"
        >
          Remove icon
        </button>
      )}
    </div>
  );
}
