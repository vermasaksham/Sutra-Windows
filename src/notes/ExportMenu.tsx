import { useEffect, useRef, useState } from "react";

/**
 * Export choices for the open note.
 *
 * The two formats reach paper by very different routes, and the labels say so
 * rather than pretending they are equivalent.
 */
export default function ExportMenu({
  onDocx,
  onPdf,
  busy,
}: {
  onDocx: () => void;
  onPdf: () => void;
  busy: boolean;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: MouseEvent) {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    const timer = setTimeout(() => document.addEventListener("mousedown", onPointerDown));
    document.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        disabled={busy}
        aria-expanded={open}
        className="rounded-md border border-border px-2 py-0.5 text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-ink disabled:opacity-60"
      >
        {busy ? "Exporting…" : "Export"}
      </button>

      {open && (
        <div
          role="menu"
          className="absolute top-full right-0 z-30 mt-1 w-56 rounded-xl border border-border bg-surface p-1 shadow-lg shadow-black/10"
        >
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onDocx();
            }}
            className="block w-full rounded-lg px-2 py-1.5 text-left transition-colors duration-150 ease-out hover:bg-accent-bg"
          >
            <span className="block text-sm text-ink">Word (.docx)</span>
            <span className="block text-xs text-ink-muted">
              Equations become images
            </span>
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onPdf();
            }}
            className="block w-full rounded-lg px-2 py-1.5 text-left transition-colors duration-150 ease-out hover:bg-accent-bg"
          >
            <span className="block text-sm text-ink">PDF</span>
            {/* Said plainly: this hands off to the system print dialog rather
                than writing a file directly. */}
            <span className="block text-xs text-ink-muted">
              Via the print dialog — choose “Save as PDF”
            </span>
          </button>
        </div>
      )}
    </div>
  );
}
