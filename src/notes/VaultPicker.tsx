import { useState } from "react";
import { vaultApi } from "../vault/api";

/**
 * Shown until a vault is chosen. The folder dialog opens on the Rust side, so
 * the chosen path never reaches this component — only the resulting name.
 */
export default function VaultPicker({ onOpened }: { onOpened: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function choose() {
    setBusy(true);
    setError(null);
    try {
      const vault = await vaultApi.pick();
      // Null means the user cancelled the dialog, which is not a failure.
      if (vault) onOpened();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="grid min-h-screen place-items-center px-6">
      <div className="flex max-w-md flex-col items-start gap-4">
        <p className="text-sm tracking-wide text-ink-muted">सूत्र · thread</p>
        <h1 className="text-3xl font-semibold tracking-tight text-ink">
          Choose a vault
        </h1>
        <p className="text-ink-soft">
          A vault is an ordinary folder. Every note is one markdown file inside
          it, readable and editable by anything else on your machine. Sutra
          keeps no copy of its own.
        </p>
        <button
          type="button"
          onClick={choose}
          disabled={busy}
          className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-surface transition-opacity hover:opacity-90 disabled:opacity-60"
        >
          {busy ? "Opening…" : "Choose folder"}
        </button>
        {error && <p className="text-sm text-highlight">{error}</p>}
      </div>
    </div>
  );
}
