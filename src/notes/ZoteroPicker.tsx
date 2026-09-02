import { useEffect, useMemo, useRef, useState } from "react";
import {
  sourcesApi,
  zoteroApi,
  type Reference,
  type ReferenceStatus,
} from "../vault/api";

/**
 * The reference picker.
 *
 * This exists because the citation menu — `@` inside the editor — was the only
 * way to reach a Zotero library, and a feature reachable by one undiscoverable
 * keystroke is a feature most people will tell you is missing. It is: same
 * library, same search, but a door with a name on it.
 *
 * Three things can be done with a result, and they are genuinely different, so
 * they are three buttons rather than one with a mode:
 *
 *   - **Literature note** makes the note you write *about* the paper.
 *   - **Add as source** brings the paper in so it can be cited, and stops.
 *   - **Open in Zotero** raises the item in Zotero itself.
 */
export default function ZoteroPicker({
  folder,
  onOpenNote,
  onClose,
  onReport,
}: {
  /** Where a new literature note should go: wherever the user is working. */
  folder: string | null;
  onOpenNote: (id: string) => void;
  onClose: () => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Reference[]>([]);
  const [status, setStatus] = useState<ReferenceStatus | null>(null);
  const [searching, setSearching] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => inputRef.current?.focus(), []);

  // Asked once, on open. If Zotero is closed the picker says so straight away
  // rather than looking broken while every search returns nothing.
  useEffect(() => {
    let live = true;
    zoteroApi
      .status()
      .then((s) => live && setStatus(s))
      .catch(() => {
        if (live) {
          setStatus({
            ready: false,
            providerId: "zotero",
            provider: "Zotero",
            reason: "could not ask about Zotero",
          });
        }
      });
    return () => {
      live = false;
    };
  }, []);

  // Debounced, and guarded against a slow response overwriting a newer one.
  useEffect(() => {
    const text = query.trim();
    if (text === "") {
      setResults([]);
      setSearching(false);
      return;
    }
    setSearching(true);
    let live = true;
    const timer = setTimeout(() => {
      zoteroApi
        .search(text)
        .then((found) => {
          if (!live) return;
          setResults(found);
          setSelected(0);
        })
        .catch(() => live && setResults([]))
        .finally(() => live && setSearching(false));
    }, 180);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [query]);

  const chosen = results[selected];

  async function run<T>(
    label: string,
    key: string,
    work: () => Promise<T>,
    then: (value: T) => void,
  ) {
    setBusy(key);
    try {
      then(await work());
    } catch (cause) {
      onReport(label, cause);
    } finally {
      setBusy(null);
    }
  }

  function literatureNote(reference: Reference) {
    void run(
      "Could not create the literature note",
      reference.key,
      () => zoteroApi.literatureNote(reference.key, folder),
      (summary) => {
        onOpenNote(summary.id);
        onClose();
      },
    );
  }

  function addSource(reference: Reference) {
    void run(
      "Could not add the source",
      reference.key,
      () => sourcesApi.importZotero(reference.key),
      (summary) => {
        onOpenNote(summary.id);
        onClose();
      },
    );
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Zotero references"
      className="sutra-no-print fixed inset-0 z-50 grid place-items-start justify-center bg-canvas/70 px-6 pt-[12vh] backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[70vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border bg-surface shadow-pane">
        <div className="flex items-center gap-2 border-b border-border px-4 py-3">
          <span className="text-xs font-semibold tracking-wide text-ink-muted uppercase">
            Zotero
          </span>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") onClose();
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelected((n) => Math.min(n + 1, results.length - 1));
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelected((n) => Math.max(n - 1, 0));
              }
              if (e.key === "Enter" && chosen) {
                e.preventDefault();
                literatureNote(chosen);
              }
            }}
            placeholder="Title, author, year, DOI, journal…"
            aria-label="Search Zotero"
            className="min-w-0 flex-1 bg-transparent text-ink outline-none placeholder:text-ink-muted"
          />
        </div>

        {status && !status.ready && (
          <StatusLine reason={status.reason} provider={status.provider} />
        )}

        <div className="min-h-0 flex-1 overflow-y-auto">
          {query.trim() === "" ? (
            <p className="px-4 py-6 text-sm text-ink-muted">
              Search your Zotero library. Enter makes a literature note from the
              highlighted paper.
            </p>
          ) : searching && results.length === 0 ? (
            <p className="px-4 py-6 text-sm text-ink-muted">Searching…</p>
          ) : results.length === 0 ? (
            <p className="px-4 py-6 text-sm text-ink-muted">
              {status && !status.ready
                ? "Nothing to search while Zotero is unreachable."
                : "No matching references."}
            </p>
          ) : (
            <ul>
              {results.map((reference, i) => (
                <Row
                  key={reference.key}
                  reference={reference}
                  active={i === selected}
                  busy={busy === reference.key}
                  onHover={() => setSelected(i)}
                  onLiteratureNote={() => literatureNote(reference)}
                  onAddSource={() => addSource(reference)}
                  onOpen={() =>
                    void zoteroApi
                      .open(reference.key)
                      .catch((cause) =>
                        onReport("Could not open Zotero", cause),
                      )
                  }
                />
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Said plainly, and without throwing anything away.
 *
 * Zotero being closed changes nothing about the notes, sources, citations or
 * evidence already in the vault — those were copied in when they were
 * imported. Only the *live* library is missing, and this line is careful to
 * say that rather than implying anything is lost.
 */
function StatusLine({
  reason,
  provider,
}: {
  reason: string | null;
  provider: string;
}) {
  return (
    <p className="border-b border-border bg-highlight-bg px-4 py-2 text-xs text-highlight">
      <strong className="font-semibold">{provider} unavailable.</strong>{" "}
      {reason ?? "Not running."} Sources already in your vault keep working.
    </p>
  );
}

function Row({
  reference,
  active,
  busy,
  onHover,
  onLiteratureNote,
  onAddSource,
  onOpen,
}: {
  reference: Reference;
  active: boolean;
  busy: boolean;
  onHover: () => void;
  onLiteratureNote: () => void;
  onAddSource: () => void;
  onOpen: () => void;
}) {
  // Only what Zotero actually returned. A missing year or journal is shown as
  // missing rather than guessed at or quietly omitted in a way that makes the
  // record look more complete than it is.
  const detail = useMemo(
    () =>
      [reference.creators, reference.year, reference.container]
        .map((part) => part?.trim())
        .filter((part): part is string => !!part)
        .join(" · "),
    [reference],
  );

  return (
    <li
      onMouseEnter={onHover}
      className={[
        "border-b border-border px-4 py-3 last:border-0",
        active ? "bg-row-active" : "",
      ].join(" ")}
    >
      <p className="text-sm font-medium text-ink">{reference.title}</p>
      <p className="mt-0.5 text-xs text-ink-muted">
        {detail || "No author or year in Zotero"}
      </p>
      <p className="mt-0.5 font-mono text-xs text-ink-muted">
        {reference.citationKey
          ? `@${reference.citationKey}`
          : "no citation key"}
        {reference.doi ? ` · ${reference.doi}` : ""}
      </p>
      <div className="mt-2 flex flex-wrap gap-2">
        <button
          type="button"
          onClick={onLiteratureNote}
          disabled={busy}
          className="rounded-lg bg-accent px-2.5 py-1 text-xs font-medium text-surface transition-opacity duration-150 ease-out hover:opacity-90 disabled:opacity-50"
        >
          {busy ? "Working…" : "Literature note"}
        </button>
        <button
          type="button"
          onClick={onAddSource}
          disabled={busy}
          className="rounded-lg border border-border px-2.5 py-1 text-xs text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
        >
          Add as source
        </button>
        <button
          type="button"
          onClick={onOpen}
          className="rounded-lg border border-border px-2.5 py-1 text-xs text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent"
        >
          Open in Zotero
        </button>
      </div>
    </li>
  );
}
