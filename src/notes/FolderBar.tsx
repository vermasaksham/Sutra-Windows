import { useEffect, useRef, useState } from "react";
import { folderCrumbs } from "./tree";

/**
 * Where the open note lives, and the only way to change it.
 *
 * The path reads as breadcrumbs and each segment is a jump. The last one opens
 * a picker, because moving a note is exactly "choose a different folder" and
 * giving that its own button somewhere else would only hide it.
 */
export default function FolderBar({
  folder,
  folders,
  onSelectFolder,
  onMove,
  onCreateFolder,
}: {
  folder: string;
  folders: string[];
  onSelectFolder: (folder: string | null) => void;
  onMove: (folder: string) => void;
  onCreateFolder: (folder: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false);
    };
    // Capture, so a click on a button inside the editor still closes this.
    document.addEventListener("mousedown", dismiss, true);
    return () => document.removeEventListener("mousedown", dismiss, true);
  }, [open]);

  const crumbs = folderCrumbs(folder);
  const elsewhere = folders.filter((f) => f !== folder);

  return (
    <div className="sutra-no-print relative mb-3 flex items-center gap-1 text-sm">
      <button
        type="button"
        onClick={() => onSelectFolder(null)}
        className="rounded px-1 text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
      >
        All notes
      </button>
      {crumbs.map(([name, path]) => (
        <span key={path} className="flex items-center gap-1">
          <span className="text-ink-muted" aria-hidden>
            ›
          </span>
          <button
            type="button"
            onClick={() => onSelectFolder(path)}
            className="rounded px-1 text-ink-soft transition-colors duration-150 ease-out hover:text-accent"
          >
            {name}
          </button>
        </span>
      ))}

      <div ref={box}>
        <button
          type="button"
          onClick={() => {
            setDraft("");
            setOpen((v) => !v);
          }}
          aria-expanded={open}
          aria-label="Move this note to another folder"
          title="Move to another folder"
          className="ml-1 rounded-md px-1.5 py-0.5 text-xs text-ink-muted transition-colors duration-150 ease-out hover:bg-row-hover hover:text-accent"
        >
          Move…
        </button>

        {open && (
          <div className="absolute top-full left-0 z-30 mt-1 w-72 rounded-xl border border-border bg-surface p-1 shadow-pane">
            <p className="px-2 pt-1 pb-1.5 text-[0.6875rem] font-semibold tracking-wide text-ink-muted uppercase">
              Move to
            </p>
            <ul className="max-h-60 overflow-y-auto">
              {folder !== "" && (
                <li>
                  <Choice
                    label="Top level"
                    onClick={() => {
                      onMove("");
                      setOpen(false);
                    }}
                  />
                </li>
              )}
              {elsewhere.map((f) => (
                <li key={f}>
                  <Choice
                    label={f}
                    onClick={() => {
                      onMove(f);
                      setOpen(false);
                    }}
                  />
                </li>
              ))}
            </ul>
            <form
              className="mt-1 border-t border-border p-1"
              onSubmit={(event) => {
                event.preventDefault();
                const wanted = draft.trim();
                if (wanted === "") return;
                onCreateFolder(wanted);
                onMove(wanted);
                setOpen(false);
              }}
            >
              <input
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                placeholder="New folder, e.g. Research/Sb2Se3"
                aria-label="New folder path"
                className="w-full rounded-md bg-row-hover px-2 py-1 text-xs text-ink outline-none placeholder:text-ink-muted"
              />
            </form>
          </div>
        )}
      </div>
    </div>
  );
}

function Choice({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full truncate rounded-lg px-2 py-1.5 text-left text-sm text-ink-soft transition-colors duration-150 ease-out hover:bg-row-hover hover:text-ink"
    >
      {label}
    </button>
  );
}
