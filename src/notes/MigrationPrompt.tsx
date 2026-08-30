import type { MigrationPlan } from "../vault/api";

/**
 * Offered once, when a vault still records its hierarchy in frontmatter.
 *
 * It shows every move before any of them happen. Reorganising someone's
 * research vault behind their back is the exact failure this design exists to
 * avoid, so the plan is the point of the dialog — the buttons are almost
 * incidental.
 */
export default function MigrationPrompt({
  plan,
  busy,
  onRun,
  onDismiss,
}: {
  plan: MigrationPlan;
  busy: boolean;
  onRun: () => void;
  onDismiss: () => void;
}) {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Organise this vault into folders"
      className="fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
    >
      <div className="flex max-h-[80vh] w-full max-w-2xl flex-col gap-3 rounded-xl border border-border bg-surface p-5 shadow-pane">
        <h2 className="text-lg font-semibold text-ink">
          Organise this vault into folders?
        </h2>

        <p className="text-sm text-ink-soft">
          Every note here sits in one flat directory, with its place in the tree
          written inside the file. Folders can hold that instead, which is what
          makes the vault readable outside Sutra. {plan.moves.length} file
          {plan.moves.length === 1 ? "" : "s"} would move.
        </p>

        <p className="text-sm text-ink-soft">
          Nothing is deleted and no link breaks — a link names a note&rsquo;s
          id, and ids do not live in paths. A copy of every note is kept under{" "}
          <code className="font-mono text-xs">.sutra/backups/</code> first.
        </p>

        {plan.flattened.length > 0 && (
          <p className="rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
            {plan.flattened.length} note
            {plan.flattened.length === 1 ? " was" : "s were"} nested deeper than
            folders go, so {plan.flattened.length === 1 ? "it" : "they"} will
            sit four folders down rather than deeper.
          </p>
        )}

        {plan.skipped.length > 0 && (
          <div className="rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
            <p>
              {plan.skipped.length} file
              {plan.skipped.length === 1 ? "" : "s"} could not be read and will
              be left exactly where{" "}
              {plan.skipped.length === 1 ? "it is" : "they are"}. Usually an
              unquoted colon in the frontmatter.
            </p>
            <ul className="mt-1 font-mono text-xs">
              {plan.skipped.map((path) => (
                <li key={path} className="truncate">
                  {path}
                </li>
              ))}
            </ul>
          </div>
        )}

        {plan.moves.length > 0 && (
          <div className="min-h-0 flex-1 overflow-y-auto rounded-lg border border-border">
            <table className="w-full text-xs">
              <tbody>
                {plan.moves.map(([from, to]) => (
                  <tr
                    key={from}
                    className="border-b border-border last:border-0"
                  >
                    {/*
                      Wrapping, not truncating. The destination is the whole
                      point of this dialog — a clipped one tells the reader
                      nothing about where their note is going.
                    */}
                    <td className="px-2.5 py-1.5 font-mono break-all text-ink-muted">
                      {from}
                    </td>
                    <td
                      className="w-4 px-0 align-top text-ink-muted"
                      aria-hidden
                    >
                      →
                    </td>
                    <td className="px-2.5 py-1.5 font-mono break-all text-ink">
                      {to}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <div className="mt-1 flex justify-end gap-2">
          <button
            type="button"
            onClick={onDismiss}
            disabled={busy}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
          >
            Not now
          </button>
          <button
            type="button"
            onClick={onRun}
            disabled={busy}
            className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-surface transition-opacity duration-150 ease-out hover:opacity-90 disabled:opacity-50"
          >
            {busy ? "Organising…" : "Organise into folders"}
          </button>
        </div>
      </div>
    </div>
  );
}
