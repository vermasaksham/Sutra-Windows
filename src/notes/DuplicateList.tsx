import { useEffect, useState } from "react";
import { duplicatesApi, type DuplicatePair } from "../vault/api";

/**
 * Every pair in the vault that may be the same note twice.
 *
 * The tidying pass, run when asked rather than in the background. A vault that
 * volunteered this would be one that interrupts to say the filing is wrong,
 * which is the opposite of what a notes app is for; a vault that offers it
 * behind a command is one that can be asked.
 *
 * Nothing is done from here. Each row opens the side-by-side comparison, which
 * is where the three real answers live.
 */
export default function DuplicateList({
  onCompare,
  onClose,
  onReport,
}: {
  onCompare: (left: string, right: string, reason: string) => void;
  onClose: () => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [pairs, setPairs] = useState<DuplicatePair[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    duplicatesApi
      .all()
      .then((found) => {
        if (!cancelled) setPairs(found);
      })
      .catch((cause) => {
        if (!cancelled) {
          setPairs([]);
          onReport("Could not scan for duplicates", cause);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [onReport]);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Possible duplicates"
      className="sutra-no-print fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
    >
      <div className="flex max-h-[80vh] w-full max-w-2xl flex-col gap-3 rounded-xl border border-border bg-surface p-5 shadow-pane">
        <div>
          <h2 className="text-lg font-semibold text-ink">
            Notes that may be duplicates
          </h2>
          <p className="text-sm text-ink-muted">
            Candidates, not conclusions. Open a pair to compare them in full.
          </p>
        </div>

        {pairs === null ? (
          <p className="text-sm text-ink-muted">Comparing the vault…</p>
        ) : pairs.length === 0 ? (
          <p className="text-sm text-ink-muted">
            Nothing looks written twice. Notes are only offered here when both
            the titles and the text substantially agree.
          </p>
        ) : (
          <ul className="min-h-0 flex-1 overflow-y-auto rounded-lg border border-border">
            {pairs.map((pair) => (
              <li key={`${pair.left}:${pair.right}`}>
                <button
                  type="button"
                  onClick={() => onCompare(pair.left, pair.right, pair.reason)}
                  className="w-full border-b border-border px-3 py-2 text-left transition-colors duration-150 ease-out last:border-0 hover:bg-row-hover"
                >
                  <span className="block truncate text-sm text-ink">
                    {pair.leftTitle}
                    <span className="text-ink-muted"> and </span>
                    {pair.rightTitle}
                  </span>
                  <span className="block truncate text-xs text-ink-muted">
                    {pair.reason} · {pair.leftFolder || "top level"} and{" "}
                    {pair.rightFolder || "top level"}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}

        <div className="flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
