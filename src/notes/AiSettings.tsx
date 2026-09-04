import { useState } from "react";
import { aiApi, type AiStatus } from "../vault/api";

/**
 * Switching assistance on, and where the key lives.
 *
 * Written to say the awkward things rather than bury them: what leaves the
 * machine, that a stored key is plain text, and that the environment variable
 * stores nothing at all. Somebody deciding whether to send years of
 * unpublished research to a third party is entitled to read that before they
 * decide, not after.
 */
export default function AiSettingsDialog({
  status,
  onSaved,
  onClose,
  onReport,
}: {
  status: AiStatus;
  onSaved: (status: AiStatus) => void;
  onClose: () => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [enabled, setEnabled] = useState(status.enabled);
  const [key, setKey] = useState("");
  const [model, setModel] = useState(status.model);
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    try {
      // An untouched key box leaves the stored key alone; typing into it
      // replaces it. "Forget the key" is its own button, so an empty box can
      // never silently wipe a working setup.
      onSaved(await aiApi.configure(enabled, key.trim() || null, model.trim()));
    } catch (cause) {
      onReport("Could not save the settings", cause);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Assistance"
      className="sutra-no-print fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
    >
      <div className="flex max-h-[86vh] w-full max-w-lg flex-col gap-3 overflow-y-auto rounded-xl border border-border bg-surface p-5 shadow-pane">
        <h2 className="text-lg font-semibold text-ink">Assistance</h2>

        <label className="flex items-start gap-2 text-sm text-ink">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
            className="mt-1 accent-accent"
          />
          <span>
            Let a model suggest summaries, tags and open questions.
            <span className="block text-ink-muted">
              Off by default. Everything else Sutra suggests is computed from
              your vault and can show its working; this cannot.
            </span>
          </span>
        </label>

        <div className="rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
          <p className="font-medium">What leaves this machine</p>
          <p>
            The one note you ask about — its title and body — and, when asking
            for tags, the list of tags your vault already uses. Nothing else: no
            other notes, no file paths, no attachments. Suggestions are never
            saved unless you accept them, and a citation naming a source your
            vault does not have is removed before you see it.
          </p>
        </div>

        <label className="flex flex-col gap-1 text-sm">
          <span className="text-ink-soft">
            API key
            {status.keyInEnvironment && (
              <span className="text-ink-muted">
                {" "}
                — not needed,{" "}
                <code className="font-mono text-xs">ANTHROPIC_API_KEY</code> is
                already set
              </span>
            )}
          </span>
          <input
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={
              status.hasKey
                ? "A key is stored — type to replace it"
                : "sk-ant-…"
            }
            className="rounded-lg border border-border bg-surface px-2 py-1 font-mono text-xs text-ink placeholder:text-ink-muted"
          />
          <span className="text-xs text-ink-muted">
            Stored as plain text in Sutra&rsquo;s config folder, not in your
            vault — a vault gets synced and shared, and a key does not belong
            somewhere that happens to. Anything running as you can read that
            file. Setting <code className="font-mono">ANTHROPIC_API_KEY</code>{" "}
            in your environment instead stores nothing at all, and is used in
            preference to this.
          </span>
        </label>

        <label className="flex flex-col gap-1 text-sm">
          <span className="text-ink-soft">Model</span>
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            className="rounded-lg border border-border bg-surface px-2 py-1 font-mono text-xs text-ink"
          />
        </label>

        <div className="flex flex-wrap justify-end gap-2">
          {status.hasKey && (
            <button
              type="button"
              disabled={busy}
              onClick={() =>
                void aiApi
                  .configure(enabled, "", model.trim())
                  .then(onSaved)
                  .catch((cause) => onReport("Could not forget the key", cause))
              }
              className="mr-auto rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors hover:border-accent hover:text-accent disabled:opacity-50"
            >
              Forget the stored key
            </button>
          )}
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors hover:border-accent hover:text-accent disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void save()}
            disabled={busy}
            className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-surface transition-opacity hover:opacity-90 disabled:opacity-50"
          >
            {busy ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
