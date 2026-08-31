import { useEffect, useState } from "react";
import { sourcesApi, type CitingNote, type SourceMeta } from "../vault/api";

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

      {meta.zotero && (
        <p className="mt-2 text-xs text-ink-muted">
          Imported from Zotero ({meta.zotero}). These details live here now — a
          citation of this source keeps working whether or not Zotero does.
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
