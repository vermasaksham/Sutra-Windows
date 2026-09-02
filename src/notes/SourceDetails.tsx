import { useEffect, useState } from "react";
import {
  sourcesApi,
  zoteroApi,
  type CitingNote,
  type SourceMeta,
} from "../vault/api";

/**
 * The paper a source note stands for, and what has been built on it.
 *
 * Shown only on a note of `type: source`. Everything is editable and everything
 * is optional — a source captured from a scribbled reference with only a title
 * is still a source, and refusing it would push people back to writing
 * citations by hand in prose, which is the loss of provenance this exists to
 * prevent.
 *
 * "Cited by" is the other half of the evidence trail. A paper that shows which
 * of your notes rest on it, and at which page, is the difference between a
 * reference list and something you can actually walk.
 */

const FIELDS: ReadonlyArray<{
  key: keyof SourceMeta;
  label: string;
  placeholder: string;
}> = [
  { key: "authors", label: "Authors", placeholder: "Zhou, Y.; Wang, L." },
  { key: "year", label: "Year", placeholder: "2019" },
  { key: "container", label: "Published in", placeholder: "Nature Energy" },
  { key: "doi", label: "DOI", placeholder: "10.1038/s41560-019-0398-y" },
  { key: "url", label: "URL", placeholder: "https://…" },
];

export default function SourceDetails({
  id,
  meta,
  onChange,
  onOpen,
}: {
  id: string;
  meta: SourceMeta;
  onChange: (meta: SourceMeta) => void;
  onOpen: (noteId: string) => void;
}) {
  const [citing, setCiting] = useState<CitingNote[]>([]);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    sourcesApi
      .citing(id)
      .then(setCiting)
      .catch(() => setCiting([]));
  }, [id]);

  return (
    <section className="mb-6 rounded-lg border border-border bg-surface p-3">
      <dl className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-sm">
        {FIELDS.map((field) => (
          <div key={field.key} className="contents">
            <dt className="py-0.5 text-xs text-ink-muted">{field.label}</dt>
            <dd className="min-w-0">
              <input
                value={(meta[field.key] as string | null | undefined) ?? ""}
                onChange={(event) =>
                  onChange({ ...meta, [field.key]: event.target.value || null })
                }
                placeholder={field.placeholder}
                aria-label={field.label}
                className="w-full rounded bg-transparent px-1 py-0.5 text-sm text-ink outline-none transition-colors duration-150 ease-out hover:bg-row-hover focus:bg-row-hover placeholder:text-ink-muted"
              />
            </dd>
          </div>
        ))}
      </dl>

      {/*
        Everything the library told us, and nothing it did not. A missing
        citation key says "none in Zotero" rather than showing a generated one:
        an invented key reads correctly in a draft and fails at the
        bibliography, which is the worst way for it to be wrong.
      */}
      {(meta.citationKey ||
        meta.itemType ||
        (meta.collections?.length ?? 0) > 0 ||
        meta.pdf) && (
        <dl className="mt-2 grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1 border-t border-border pt-2 text-sm">
          <dt className="text-xs text-ink-muted">Citation key</dt>
          <dd className="min-w-0 font-mono text-xs text-ink-soft">
            {meta.citationKey ? `@${meta.citationKey}` : "None in Zotero"}
          </dd>
          {meta.itemType && (
            <>
              <dt className="text-xs text-ink-muted">Type</dt>
              <dd className="min-w-0 text-xs text-ink-soft">{meta.itemType}</dd>
            </>
          )}
          {(meta.collections?.length ?? 0) > 0 && (
            <>
              <dt className="text-xs text-ink-muted">Collections</dt>
              <dd className="min-w-0 text-xs text-ink-soft">
                {meta.collections?.join(" · ")}
              </dd>
            </>
          )}
          <dt className="text-xs text-ink-muted">PDF</dt>
          <dd className="min-w-0 text-xs text-ink-soft">
            {/* Named, never copied: the file stays Zotero's, and this only
                lets the note say one exists while Zotero is closed. */}
            {meta.pdf ? `${meta.pdf} — in Zotero` : "Not available"}
          </dd>
        </dl>
      )}

      {meta.abstractText && (
        <div className="mt-2 border-t border-border pt-2">
          <p className="text-[0.65rem] font-semibold tracking-wide text-highlight uppercase">
            Abstract — as published
          </p>
          <p className="mt-0.5 border-l-2 border-highlight bg-highlight-bg/40 px-2 py-1 text-sm text-ink-soft italic">
            {meta.abstractText}
          </p>
        </div>
      )}

      {meta.zotero && (
        <div className="mt-2 flex flex-wrap items-center gap-2 border-t border-border pt-2">
          <p className="min-w-0 flex-1 text-xs text-ink-muted">
            Imported from Zotero ({meta.zotero}). These details live here now —
            a citation of this source keeps working whether or not Zotero does.
          </p>
          <button
            type="button"
            onClick={() => {
              const key = meta.zotero;
              if (key) void zoteroApi.open(key).catch(() => setFailed(true));
            }}
            className="sutra-no-print shrink-0 rounded-lg border border-border px-2 py-1 text-xs text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent"
          >
            Open in Zotero
          </button>
        </div>
      )}

      {failed && (
        <p className="mt-1 text-xs text-highlight">
          Zotero did not open. Everything above is cached here and is
          unaffected.
        </p>
      )}

      <div className="mt-3 border-t border-border pt-2">
        <h3 className="mb-1 text-xs font-semibold tracking-wide text-ink-muted uppercase">
          Cited by {citing.length > 0 && `(${citing.length})`}
        </h3>
        {citing.length === 0 ? (
          <p className="text-sm text-ink-muted">Nothing rests on this yet.</p>
        ) : (
          <ul className="flex flex-col gap-0.5">
            {citing.map((note) => (
              <li key={`${note.id}:${note.page ?? ""}`}>
                <button
                  type="button"
                  onClick={() => onOpen(note.id)}
                  className="w-full truncate rounded px-1 py-0.5 text-left text-sm text-ink-soft transition-colors duration-150 ease-out hover:bg-row-hover hover:text-accent"
                >
                  {note.title || "Untitled"}
                  {note.page && (
                    <span className="text-ink-muted"> · p. {note.page}</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}
