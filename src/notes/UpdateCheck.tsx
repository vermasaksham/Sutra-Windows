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
  /** Set when the app could not say what version it is. */
  const [unknown, setUnknown] = useState(false);
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    let live = true;
    void updatesApi
      .version()
      .then((v) => live && setVersion(v))
      // A failure here used to be swallowed, and the line went on reading
      // "Sutra …" for ever — an ellipsis that looks like it is still loading
      // and never resolves. Saying the version is unavailable is the honest
      // answer, and it is the answer somebody needs when they are about to
      // report a bug.
      .catch(() => live && setUnknown(true));
    return () => {
      live = false;
    };
  }, []);

  /**
   * The version to show. After a check the backend has said so again as part
   * of the answer, and the two always agree — but preferring `status.current`
   * means there is one place the number comes from once a check has happened.
   */
  const running = status?.current ?? version;

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
      {/* The version on its own line, and the update news underneath it.
          They used to be one sentence, which read well and buried the one
          fact this section exists to state — the version was only ever a
          clause inside a message about something else. */}
      <div className="flex flex-col gap-0.5">
        <p className="text-sm text-ink">
          {running
            ? `Sutra ${running}`
            : unknown
              ? "Sutra — version unavailable"
              : "Sutra …"}
        </p>
        <p className="text-xs text-ink-muted">
          {status?.newer
            ? `${status.latest} is out.`
            : status
              ? "This is the newest release."
              : "Updates are not automatic yet."}
        </p>
      </div>
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
