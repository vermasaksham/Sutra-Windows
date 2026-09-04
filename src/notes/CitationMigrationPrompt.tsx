import { useState } from "react";
import { legacyCitationsApi, type CitationMigration } from "../vault/api";

/**
 * Offered while any citation still points at a Zotero item rather than a note.
 *
 * Those citations work — until Zotero is uninstalled, or the vault is opened
 * somewhere it is not, and then they resolve to nothing at all. This turns each
 * of them into a source note in the vault, after which the vault never needs
 * Zotero again.
 *
 * Needs Zotero this once: the keys came from there and only it knows what they
 * stand for. A key it cannot answer for is left exactly as it is rather than
 * dropped, and the migration can be run again later.
 */
export default function CitationMigrationPrompt({
  counts,
  onClose,
  onDone,
  onReport,
}: {
  counts: Record<string, number>;
  onClose: () => void;
  onDone: () => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<CitationMigration | null>(null);

  const keys = Object.entries(counts).sort(
    ([aKey, aCount], [bKey, bCount]) =>
      bCount - aCount || aKey.localeCompare(bKey),
  );
  const notes = Object.values(counts).reduce((a, b) => a + b, 0);

  async function run() {
    setBusy(true);
    try {
      const done = await legacyCitationsApi.migrate();
      setResult(done);
      onDone();
    } catch (cause) {
      onReport("Could not migrate the citations", cause);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Migrate Zotero citations"
      className="sutra-no-print fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
    >
      <div className="flex max-h-[80vh] w-full max-w-lg flex-col gap-3 rounded-xl border border-border bg-surface p-5 shadow-pane">
        <h2 className="text-lg font-semibold text-ink">
          Turn Zotero citations into sources?
        </h2>

        {result ? (
          <>
            {result.migrated.length === 0 ? (
              <p className="text-sm text-ink-soft">
                Nothing could be migrated, so every citation was left exactly as
                it was.
              </p>
            ) : (
              <p className="text-sm text-ink-soft">
                {result.migrated.length === 1
                  ? "One reference became a source note"
                  : `${result.migrated.length} references became source notes`}
                , and{" "}
                {result.notesChanged === 1
                  ? "one note now cites it"
                  : `${result.notesChanged} notes now cite them`}
                . Those citations no longer need Zotero.
              </p>
            )}
            {result.unresolved.length > 0 && (
              <div className="rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
                <p>
                  {result.unresolved.length === 1
                    ? "Zotero had nothing for one key, so that citation was left exactly as it was"
                    : `Zotero had nothing for ${result.unresolved.length} keys, so those citations were left exactly as they were`}
                  {" — "}deleted from the library, perhaps, or from a different
                  one. Run this again if that changes.
                </p>
                <ul className="mt-1 font-mono text-xs">
                  {result.unresolved.map((key) => (
                    <li key={key}>{key}</li>
                  ))}
                </ul>
              </div>
            )}
          </>
        ) : (
          <>
            <p className="text-sm text-ink-soft">
              {keys.length} reference{keys.length === 1 ? "" : "s"} across{" "}
              {notes} citation{notes === 1 ? "" : "s"} still point at Zotero
              items. They resolve only while Zotero is running — open this vault
              on another machine and they resolve to nothing.
            </p>
            <p className="text-sm text-ink-soft">
              Each becomes a source note in{" "}
              <code className="font-mono text-xs">Library/</code>, holding the
              paper&rsquo;s details, and the citations point at those instead.
              Nothing is deleted, and note timestamps are left alone.
            </p>
            <p className="text-sm text-ink-soft">
              Zotero needs to be running for this, once.
            </p>

            <ul className="max-h-40 overflow-y-auto rounded-lg border border-border">
              {keys.map(([key, count]) => (
                <li
                  key={key}
                  className="flex justify-between border-b border-border px-3 py-1.5 text-xs last:border-0"
                >
                  <span className="font-mono text-ink">{key}</span>
                  <span className="text-ink-muted">
                    {count} citation{count === 1 ? "" : "s"}
                  </span>
                </li>
              ))}
            </ul>
          </>
        )}

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors hover:border-accent hover:text-accent disabled:opacity-50"
          >
            {result ? "Done" : "Not now"}
          </button>
          {!result && (
            <button
              type="button"
              onClick={() => void run()}
              disabled={busy}
              className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-surface transition-opacity hover:opacity-90 disabled:opacity-50"
            >
              {busy ? "Asking Zotero…" : "Migrate them"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
