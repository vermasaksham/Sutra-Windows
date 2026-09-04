import { useEffect, useState } from "react";
import { updatesApi, type UpdateStatus } from "../vault/api";

/**
 * The version you are running, and a button that asks whether there is a newer
 * one.
 *
 * A button, not a timer. Everything else in Sutra is computed from your own
 * vault, and the app says in those words that nothing leaves this machine
 * unless you turn something on — a background check phoning GitHub would make
 * that quietly untrue. The cost is that you have to press it; the benefit is
 * that the sentence stays true.
 */
export default function UpdateCheck({
  onReport,
}: {
  onReport: (message: string, cause?: unknown) => void;
}) {
  const [version, setVersion] = useState<string | null>(null);
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    let live = true;
    void updatesApi
      .version()
      .then((v) => live && setVersion(v))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  async function check() {
    setChecking(true);
    try {
      setStatus(await updatesApi.check());
    } catch (e) {
      // Said out loud rather than shown as "up to date": being told nothing is
      // wrong when the check never happened is worse than being told it failed.
      onReport(e instanceof Error ? e.message : String(e));
    } finally {
      setChecking(false);
    }
  }

  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <p className="text-sm text-ink-soft">
        {status?.newer
          ? `Sutra ${status.latest} is out. You have ${status.current}.`
          : status
            ? `Sutra ${status.current}. This is the newest release.`
            : `Sutra ${version ?? "…"}. Updates are not automatic yet.`}
      </p>
      {status?.newer ? (
        <button
          type="button"
          onClick={() =>
            void updatesApi.open(status.url).catch(() => undefined)
          }
          className="rounded-lg border border-accent px-3 py-1.5 text-sm text-accent transition-colors hover:bg-accent-bg"
        >
          Get {status.latest}
        </button>
      ) : (
        <button
          type="button"
          onClick={() => void check()}
          disabled={checking}
          className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors hover:border-accent hover:text-accent disabled:opacity-50"
        >
          {checking ? "Checking…" : "Check for updates"}
        </button>
      )}
    </div>
  );
}
