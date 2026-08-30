import { useCallback, useEffect, useState } from "react";
import Editor from "./editor/Editor";
import ThemeToggle from "./components/ThemeToggle";
import Toast from "./components/Toast";
import { setNavigate, setTitles } from "./editor/wikilink/titleStore";
import BacklinksPanel from "./notes/BacklinksPanel";
import Bibliography from "./notes/Bibliography";
import Breadcrumbs from "./notes/Breadcrumbs";
import ConflictPrompt from "./notes/ConflictPrompt";
import NoteHeader from "./notes/NoteHeader";
import NoteTree from "./notes/NoteTree";
import SearchPanel from "./notes/SearchPanel";
import VaultPicker from "./notes/VaultPicker";
import { useShortcuts } from "./notes/shortcuts";
import { useNote } from "./notes/useNote";
import {
  indexApi,
  notesApi,
  vaultApi,
  type Backlink,
  type NoteSummary,
  type VaultInfo,
} from "./vault/api";

const SAVE_LABEL = {
  saved: "Saved",
  dirty: "Unsaved",
  saving: "Saving…",
  error: "Save failed",
} as const;

export default function App() {
  const [vault, setVault] = useState<VaultInfo | null>(null);
  const [checked, setChecked] = useState(false);
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [backlinks, setBacklinks] = useState<Backlink[]>([]);
  const [search, setSearch] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  /** Report a failed command instead of swallowing it. */
  const report = useCallback((what: string, cause: unknown) => {
    const detail = cause instanceof Error ? cause.message : String(cause);
    setError(`${what}: ${detail}`);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const list = await notesApi.list();
      setNotes(list);
      // Feed the wikilink renderer, so [[id]] shows the current title. This is
      // why a rename cannot break a link: nothing stores the title but here.
      setTitles(list.map((n) => [n.id, n.title] as const));
    } catch (cause) {
      report("Could not list notes", cause);
    }
  }, [report]);

  const note = useNote(selectedId, refresh);

  useEffect(() => {
    vaultApi
      .current()
      .then(setVault)
      .catch(() => setVault(null))
      .finally(() => setChecked(true));
  }, []);

  useEffect(() => {
    if (vault) void refresh();
  }, [vault, refresh]);

  useEffect(() => {
    if (!selectedId && notes.length > 0) setSelectedId(notes[0]!.id);
  }, [notes, selectedId]);

  const select = useCallback(
    async (id: string) => {
      // Flush first, so switching away never drops the last few keystrokes.
      await note.flush();
      setSelectedId(id);
      setSearch(null);
    },
    [note],
  );

  useEffect(() => setNavigate((id) => void select(id)), [select]);

  useEffect(() => {
    if (!selectedId) return setBacklinks([]);
    indexApi
      .backlinks(selectedId)
      .then(setBacklinks)
      .catch(() => setBacklinks([]));
  }, [selectedId, notes]);

  const createNote = useCallback(
    async (parent: string | null) => {
      try {
        await note.flush();
        const created = await notesApi.create("Untitled", parent);
        await refresh();
        setSelectedId(created.id);
      } catch (cause) {
        report("Could not create the note", cause);
      }
    },
    [note, refresh, report],
  );

  useShortcuts({
    search: () => setSearch((open) => (open === null ? "" : null)),
    newNote: () => void createNote(null),
    save: () => void note.flush(),
  });

  async function deleteNote(id: string) {
    try {
      // Cancel any queued autosave for this note first. Deleting it while a
      // save is still pending would write to a file that has just moved to the
      // trash.
      if (id === selectedId) note.discard();
      await notesApi.remove(id);
      const remaining = await notesApi.list();
      setNotes(remaining);
      setTitles(remaining.map((n) => [n.id, n.title] as const));
      if (id === selectedId) setSelectedId(remaining[0]?.id ?? null);
    } catch (cause) {
      report("Could not move the note to the trash", cause);
    }
  }

  /** Icon, cover and tags all go the same way: full desired state to Rust. */
  const setMeta = useCallback(
    async (patch: { icon?: string | null; cover?: string | null; tags?: string[] }) => {
      const current = note.doc;
      if (!current) return;
      try {
        const updated = await notesApi.setMeta(
          current.id,
          patch.icon !== undefined ? patch.icon : current.icon,
          patch.cover !== undefined ? patch.cover : current.cover,
          patch.tags ?? current.tags,
        );
        // Merge the new metadata rather than re-reading the note: a re-read
        // would replace the body with what is on disk and lose anything still
        // in the buffer.
        note.applyMeta(updated);
        await refresh();
      } catch (cause) {
        report("Could not update the page", cause);
      }
    },
    [note, refresh, report],
  );

  async function pickCover() {
    try {
      const reference = await notesApi.attach();
      if (reference) await setMeta({ cover: reference });
    } catch (cause) {
      report("Could not add the cover", cause);
    }
  }

  if (!checked) return null;
  if (!vault) {
    return <VaultPicker onOpened={() => void vaultApi.current().then(setVault)} />;
  }

  return (
    <div className="flex h-screen flex-col">
      <header className="flex h-11 shrink-0 items-center justify-between border-b border-border px-3">
        <span className="text-sm font-semibold tracking-tight text-ink-soft">
          {vault.name}
        </span>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => setSearch("")}
            className="rounded-md border border-border px-2 py-0.5 text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-ink"
          >
            Search <span className="font-mono">Ctrl K</span>
          </button>
          <span className="text-xs text-ink-muted tabular-nums" aria-live="polite">
            {SAVE_LABEL[note.saveState]}
          </span>
          <ThemeToggle />
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <NoteTree
          notes={notes}
          selectedId={selectedId}
          onSelect={(id) => void select(id)}
          onCreate={(parent) => void createNote(parent)}
          onDelete={(id) => void deleteNote(id)}
        />

        <main className="min-w-0 flex-1 overflow-y-auto">
          {note.doc ? (
            // `group/page` so the icon and cover affordances appear on
            // approach rather than sitting there permanently.
            <div className="group/page mx-auto max-w-content px-6 pt-6 pb-12">
              <Breadcrumbs
                notes={notes}
                id={selectedId}
                onSelect={(id) => void select(id)}
              />
              <NoteHeader
                doc={note.doc}
                onTitle={note.setTitle}
                onIcon={(icon) => void setMeta({ icon })}
                onCover={() => void pickCover()}
                onTags={(tags) => void setMeta({ tags })}
                onSelectTag={(tag) => setSearch(tag)}
              />
              {note.doc.adopted && (
                <p className="mb-4 rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
                  This file had no frontmatter. Sutra will add one when you save.
                </p>
              )}
              <Editor
                key={`${note.doc.id}:${note.revision}`}
                body={note.doc.body}
                onChange={note.setBody}
              />
              <Bibliography body={note.doc.body} />
              <BacklinksPanel
                backlinks={backlinks}
                onSelect={(id) => void select(id)}
              />
            </div>
          ) : (
            <div className="grid h-full place-items-center px-6">
              <div className="flex max-w-sm flex-col items-center gap-2 text-center">
                <p className="text-ink-soft">
                  {notes.length === 0
                    ? "This vault is empty."
                    : "No note selected."}
                </p>
                <p className="text-sm text-ink-muted">
                  {notes.length === 0
                    ? "Every note is one markdown file in the folder you chose."
                    : "Pick one from the sidebar, or search with Ctrl K."}
                </p>
                {notes.length === 0 && (
                  <button
                    type="button"
                    onClick={() => void createNote(null)}
                    className="mt-2 rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-surface transition-opacity duration-150 ease-out hover:opacity-90"
                  >
                    New note
                  </button>
                )}
              </div>
            </div>
          )}
        </main>
      </div>

      {search !== null && (
        <SearchPanel
          initialQuery={search}
          onClose={() => setSearch(null)}
          onSelect={(id) => void select(id)}
        />
      )}

      {note.conflict && (
        <ConflictPrompt
          note={note.conflict}
          onResolve={(choice) => void note.resolveConflict(choice)}
        />
      )}

      {error && <Toast message={error} onDismiss={() => setError(null)} />}
    </div>
  );
}
