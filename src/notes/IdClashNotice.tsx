import type { IdClash } from "../vault/api";

/**
 * Two files on disk claiming to be the same note.
 *
 * A sync client's conflicted copy, a note duplicated in Explorer, a bad merge.
 * Both files are listed and both are safe; only the first is reachable by id,
 * so the second sits in the list looking like a note and opens as its twin.
 *
 * Shown rather than fixed. Which of two versions of somebody's work to discard
 * is not a decision this program gets to make, and the two are often not
 * duplicates at all — they are the morning's writing and the afternoon's, and
 * the person who wrote them is the only one who can say which is which.
 */
export default function IdClashNotice({ clashes }: { clashes: IdClash[] }) {
  if (clashes.length === 0) return null;

  return (
    <div
      role="alert"
      className="sutra-no-print mx-3 mb-2 rounded-lg border border-accent bg-accent-bg px-3 py-2"
    >
      <p className="text-xs font-semibold text-ink">
        {clashes.length === 1
          ? "Two files claim to be the same note"
          : `${clashes.length} notes have more than one file claiming to be them`}
      </p>
      <p className="mt-1 text-xs text-ink-soft">
        Both are on disk and nothing has been changed. Sutra opens the first;
        the other is listed but cannot be opened by itself. Compare them in
        Explorer and delete whichever you do not want.
      </p>
      <ul className="mt-2 flex flex-col gap-1">
        {clashes.map((clash) => (
          <li key={clash.id} className="font-mono text-xs text-ink-muted">
            <span className="text-ink-soft">{clash.opened}</span>
            {" ← opened, shadowing → "}
            <span className="text-ink-soft">{clash.shadowed}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
