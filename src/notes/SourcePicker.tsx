import { useEffect, useState } from "react";
import {
  sourcesApi,
  zoteroApi,
  type NoteSummary,
  type Reference,
} from "../vault/api";

/**
 * Choosing what a note is drawing on.
 *
 * Two places to look, in this order: sources already in the vault, and Zotero.
 * The vault first because it is instant, always available, and because citing
 * a source you already have is the common case — a paper gets read once and
 * cited a dozen times.
 *
 * Picking a Zotero item copies it into the vault first. That is the whole
 * argument of this step: the citation ends up pointing at a note, so it keeps
 * meaning something when Zotero is not running.
 */
export default function SourcePicker({
  onClose,
  onPick,
  onReport,
}: {
  onClose: () => void;
  onPick: (source: NoteSummary) => void;
  onReport: (message: string, cause: unknown) => void;
}) {
  const [query, setQuery] = useState("");
  const [vault, setVault] = useState<NoteSummary[]>([]);
  const [zotero, setZotero] = useState<Reference[]>([]);
  /** Zotero is optional, so its absence is a note, not an error. */
  const [zoteroProblem, setZoteroProblem] = useState<string | null>(null);
  const [importing, setImporting] = useState<string | null>(null);

  useEffect(() => {
    sourcesApi
      .list()
      .then(setVault)
      .catch(() => setVault([]));
  }, []);

  useEffect(() => {
    const text = query.trim();
    if (text === "") {
      setZotero([]);
      setZoteroProblem(null);
      return;
    }
    const timer = setTimeout(() => {
      zoteroApi
        .search(text)
        .then((found) => {
          setZotero(found);
          setZoteroProblem(null);
        })
        .catch((cause) => {
          setZotero([]);
          setZoteroProblem(String(cause));
        });
    }, 200);
    return () => clearTimeout(timer);
  }, [query]);

  const needle = query.trim().toLowerCase();
  const matching = needle
    ? vault.filter((s) => s.title.toLowerCase().includes(needle))
    : vault;
  // A Zotero item already in the vault is offered once, from the vault.
  const inVault = new Set(
    vault.map((s) => s.source?.zotero).filter(Boolean) as string[],
  );
  const newToVault = zotero.filter((r) => !inVault.has(r.key));

  async function importAndPick(reference: Reference) {
    setImporting(reference.key);
    try {
      onPick(await sourcesApi.importZotero(reference.key));
    } catch (cause) {
      onReport("Could not import that source", cause);
    } finally {
      setImporting(null);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Add a source"
      className="sutra-no-print fixed inset-0 z-50 flex justify-center bg-canvas/70 px-6 pt-24 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="flex max-h-[60vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-border bg-surface shadow-pane"
        onClick={(event) => event.stopPropagation()}
      >
        <input
          autoFocus
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") onClose();
          }}
          placeholder="Title, author or DOI"
          aria-label="Find a source"
          className="border-b border-border bg-transparent px-4 py-3 text-ink outline-none placeholder:text-ink-muted"
        />

        <div className="overflow-y-auto p-1">
          <p className="px-3 pt-2 pb-1 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
            In this vault
          </p>
          {matching.length === 0 ? (
            <p className="px-3 pb-2 text-sm text-ink-muted">
              {vault.length === 0
                ? "No sources yet. Find one in Zotero, or write one by hand."
                : "Nothing here matches."}
            </p>
          ) : (
            <ul>
              {matching.slice(0, 12).map((source) => (
                <li key={source.id}>
                  <button
                    type="button"
                    onClick={() => onPick(source)}
                    className="w-full rounded-lg px-3 py-1.5 text-left transition-colors duration-150 ease-out hover:bg-row-hover"
                  >
                    <span className="block truncate text-sm text-ink">
                      {source.title}
                    </span>
                    <span className="block truncate text-xs text-ink-muted">
                      {describe(source.source)}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}

          <p className="px-3 pt-3 pb-1 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
            Zotero
          </p>
          {zoteroProblem ? (
            <p className="px-3 pb-2 text-xs text-highlight">{zoteroProblem}</p>
          ) : needle === "" ? (
            <p className="px-3 pb-2 text-sm text-ink-muted">
              Type to search your library.
            </p>
          ) : newToVault.length === 0 ? (
            <p className="px-3 pb-2 text-sm text-ink-muted">
              Nothing new in Zotero for that.
            </p>
          ) : (
            <ul>
              {newToVault.slice(0, 8).map((reference) => (
                <li key={reference.key}>
                  <button
                    type="button"
                    disabled={importing !== null}
                    onClick={() => void importAndPick(reference)}
                    className="w-full rounded-lg px-3 py-1.5 text-left transition-colors duration-150 ease-out hover:bg-row-hover disabled:opacity-50"
                  >
                    <span className="block truncate text-sm text-ink">
                      {reference.title}
                    </span>
                    <span className="block truncate text-xs text-ink-muted">
                      {importing === reference.key
                        ? "Copying into the vault…"
                        : [
                            reference.creators,
                            reference.year,
                            reference.container,
                          ]
                            .filter(Boolean)
                            .join(" · ")}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}

/** A source's details on one line, for a list. */
export function describe(meta: NoteSummary["source"]): string {
  if (!meta) return "";
  return [
    meta.authors,
    meta.year,
    meta.container,
    meta.doi && `doi:${meta.doi}`,
  ]
    .filter(Boolean)
    .join(" · ");
}
