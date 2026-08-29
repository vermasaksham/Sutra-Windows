import { useEffect } from "react";

/**
 * A failure the user should know about.
 *
 * Commands used to fail into a `.catch` that set state and said nothing, so a
 * save that could not reach the disk looked identical to one that worked. This
 * is the smallest honest fix: say what failed, and stay until dismissed if it
 * matters.
 */
export default function Toast({
  message,
  onDismiss,
}: {
  message: string;
  onDismiss: () => void;
}) {
  useEffect(() => {
    // Long enough to read, short enough not to sit in the way. Errors that
    // need a decision get a dialog, not a toast.
    const timer = setTimeout(onDismiss, 6000);
    return () => clearTimeout(timer);
  }, [message, onDismiss]);

  return (
    <div
      role="status"
      aria-live="polite"
      className="sutra-toast fixed bottom-4 left-1/2 z-50 flex max-w-md -translate-x-1/2 items-start gap-3 rounded-lg border border-border bg-surface px-4 py-2.5 shadow-lg shadow-black/10"
    >
      <span className="text-sm text-ink">{message}</span>
      <button
        type="button"
        onClick={onDismiss}
        aria-label="Dismiss"
        className="text-ink-muted transition-colors duration-150 ease-out hover:text-ink"
      >
        ×
      </button>
    </div>
  );
}
