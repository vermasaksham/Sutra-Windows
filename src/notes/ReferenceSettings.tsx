import { useEffect, useState } from "react";
import {
  CITATION_STYLES,
  zoteroApi,
  type ReferenceConfig,
  type ReferenceStatus,
} from "../vault/api";

/**
 * Where the literature comes from, and how citations should read.
 *
 * Two settings that look unrelated and are not. A citation style is rendered
 * by the library, so a style without a reachable library is a preference that
 * cannot take effect — and putting them in one panel is what makes that
 * visible instead of puzzling.
 */
export default function ReferenceSettingsPanel({
  onReport,
}: {
  onReport: (message: string, cause: unknown) => void;
}) {
  const [config, setConfig] = useState<ReferenceConfig | null>(null);
  const [status, setStatus] = useState<ReferenceStatus | null>(null);
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [restyled, setRestyled] = useState<number | null>(null);

  useEffect(() => {
    let live = true;
    zoteroApi
      .config()
      .then((c) => live && setConfig(c))
      .catch((cause) =>
        onReport("Could not read the reference settings", cause),
      );
    return () => {
      live = false;
    };
  }, [onReport]);

  if (!config) {
    return <p className="text-sm text-ink-muted">Reading settings…</p>;
  }

  const custom = !CITATION_STYLES.some((s) => s.id === config.style);

  async function connect() {
    setBusy(true);
    setStatus(null);
    try {
      const next = await zoteroApi.connect(key.trim());
      setConfig(next);
      setKey("");
      // Connect means connected: the button used to say "Save and test" and
      // only save, which left the one question the user had unanswered.
      setStatus(await zoteroApi.status());
    } catch (cause) {
      onReport("Could not connect to Zotero", cause);
    } finally {
      setBusy(false);
    }
  }

  async function save(next: ReferenceConfig, sendKey: boolean) {
    setBusy(true);
    setStatus(null);
    setRestyled(null);
    try {
      setConfig(
        await zoteroApi.configure(
          next.account,
          next.userId?.trim() || null,
          sendKey ? key.trim() : null,
          next.style.trim(),
          next.locale.trim() || "en-US",
        ),
      );
      if (sendKey) setKey("");
    } catch (cause) {
      onReport("Could not save the reference settings", cause);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <div
        role="radiogroup"
        aria-label="Where the library is"
        className="flex flex-col gap-1.5"
      >
        <Choice
          checked={!config.account}
          onChoose={() => void save({ ...config, account: false }, false)}
          title="Zotero on this computer"
          detail="Zotero must be running, with “Allow other applications on this computer to communicate with Zotero” on in Settings → Advanced. Nothing leaves this machine."
        />
        <Choice
          checked={config.account}
          onChoose={() => void save({ ...config, account: true }, false)}
          title="My Zotero account"
          detail="Works with Zotero closed, and on a machine where it is not installed. Your search terms and the library it returns travel to zotero.org."
        />
      </div>

      {config.account && (
        <div className="flex flex-col gap-2 rounded-lg border border-border bg-canvas p-3">
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-ink-soft">
              API key
              {config.keyInEnvironment && (
                <>
                  {" "}
                  — <code className="font-mono text-xs">ZOTERO_API_KEY</code> is
                  set, and is used in preference to anything typed here
                </>
              )}
            </span>
            <input
              type="password"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder={
                config.hasKey
                  ? "A key is stored — paste a new one to replace it"
                  : "Paste your key from zotero.org/settings/keys"
              }
              aria-label="Zotero API key"
              className="rounded border border-border bg-surface px-2 py-1 font-mono text-sm text-ink outline-none focus:border-accent"
            />
            <span className="text-xs text-ink-muted">
              Create one at{" "}
              <code className="font-mono text-xs">
                zotero.org/settings/keys/new
              </code>{" "}
              with <strong>Allow library access</strong> ticked. Read-only is
              enough — Sutra never writes to your library. A key typed here is
              stored in plain text in the app&rsquo;s config file; setting{" "}
              <code className="font-mono text-xs">ZOTERO_API_KEY</code> instead
              stores nothing at all.
            </span>
          </label>

          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              disabled={busy || key.trim() === ""}
              onClick={() => void connect()}
              className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-surface transition-opacity duration-150 ease-out hover:opacity-90 disabled:opacity-50"
            >
              {busy ? "Connecting…" : "Connect"}
            </button>
            {config.userId && (
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  setBusy(true);
                  setStatus(null);
                  zoteroApi
                    .status()
                    .then(setStatus)
                    .catch((cause) =>
                      onReport("Could not test the connection", cause),
                    )
                    .finally(() => setBusy(false));
                }}
                className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
              >
                Test connection
              </button>
            )}
          </div>

          {/*
            Shown, not asked for. The user ID is the thing that was going wrong:
            the API wants a number, Zotero's own docs warn it is "different from
            usernames", and the obvious mistake answers 404 in a way that looks
            like an empty library. So the app reads it off the key and just
            reports what it found.
          */}
          {config.userId ? (
            <p className="text-xs text-highlight">
              Connected as user{" "}
              <code className="font-mono text-xs">{config.userId}</code>.
            </p>
          ) : (
            <p className="text-xs text-ink-muted">
              Paste the key and press Connect. Sutra reads your user ID from the
              key — you do not need to find it yourself.
            </p>
          )}
        </div>
      )}

      {status && (
        <p
          className={[
            "text-xs",
            status.ready ? "text-highlight" : "text-accent",
          ].join(" ")}
        >
          {status.ready
            ? `Connected to ${status.provider}.`
            : `${status.provider}: ${status.reason ?? "unreachable"}`}
        </p>
      )}

      <label className="flex flex-col gap-1 text-sm">
        <span className="text-ink-soft">Citation style</span>
        <select
          aria-label="Citation style"
          value={custom ? "__custom" : config.style}
          onChange={(e) => {
            const chosen = e.target.value;
            if (chosen === "__custom") return;
            void save({ ...config, style: chosen }, false);
          }}
          className="rounded border border-border bg-surface px-2 py-1 text-sm text-ink"
        >
          {CITATION_STYLES.map((style) => (
            <option key={style.id} value={style.id}>
              {style.label}
            </option>
          ))}
          <option value="__custom">Another style…</option>
        </select>
        {custom && (
          <input
            aria-label="Custom citation style id"
            value={config.style}
            onChange={(e) => setConfig({ ...config, style: e.target.value })}
            onBlur={() => void save(config, false)}
            placeholder="e.g. elsevier-harvard"
            className="rounded border border-border bg-surface px-2 py-1 font-mono text-xs text-ink outline-none focus:border-accent"
          />
        )}
        <span className="text-xs text-ink-muted">
          {/*
            Said plainly because it is the honest description of the feature:
            the app does not format citations, Zotero does. That is why every
            style in its repository works, and why the result matches what the
            same library produces in Word.
          */}
          Rendered by Zotero&rsquo;s own citation engine, so any style id from{" "}
          <code className="font-mono text-xs">zotero.org/styles</code> works and
          the result matches what this library produces in Word.
        </span>
      </label>

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            setRestyled(null);
            zoteroApi
              .restyle()
              .then(setRestyled)
              .catch((cause) =>
                onReport("Could not restyle the sources", cause),
              )
              .finally(() => setBusy(false));
          }}
          className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
        >
          {busy ? "Working…" : "Restyle existing sources"}
        </button>
        {restyled !== null && (
          <span className="text-xs text-ink-muted">
            {restyled === 0
              ? "Nothing to restyle — no sources are linked to a library."
              : `${restyled} source${restyled === 1 ? "" : "s"} re-rendered.`}
          </span>
        )}
      </div>
      <p className="text-xs text-ink-muted">
        Changing the style only affects sources imported afterwards, because the
        rendered citations are cached so they keep working offline. Restyle
        brings the existing ones over.
      </p>
    </div>
  );
}

function Choice({
  checked,
  onChoose,
  title,
  detail,
}: {
  checked: boolean;
  onChoose: () => void;
  title: string;
  detail: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      onClick={onChoose}
      className={[
        "rounded-lg border px-3 py-2 text-left transition-colors duration-150 ease-out",
        checked
          ? "border-accent bg-accent-bg"
          : "border-border hover:border-accent",
      ].join(" ")}
    >
      <span
        className={[
          "block text-sm",
          checked ? "font-semibold text-accent" : "font-medium text-ink",
        ].join(" ")}
      >
        {title}
      </span>
      <span className="block text-xs text-ink-muted">{detail}</span>
    </button>
  );
}
