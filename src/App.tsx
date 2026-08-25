import { useCallback, useEffect, useState } from "react";
import Editor from "./editor/Editor";
import ThemeToggle from "./components/ThemeToggle";
import { setNavigate, setTitles } from "./editor/wikilink/titleStore";
import BacklinksPanel from "./notes/BacklinksPanel";
import Breadcrumbs from "./notes/Breadcrumbs";
import ConflictPrompt from "./notes/ConflictPrompt";
import NoteTree from "./notes/NoteTree";
import SearchPanel from "./notes/SearchPanel";
import VaultPicker from "./notes/VaultPicker";
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
  const [searching, setSearching] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const list = await notesApi.list();
      setNotes(list);
      // Feed the wikilink renderer, so [[id]] shows the current title. This is
      // why a rename cannot break a link: nothing stores the title but here.
      setTitles(list.map((n) => [n.id, n.title] as const));
    } catch {
      setNotes([]);
    }
  }, []);

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
      setSearching(false);
    },
    [note],
  );

  // Following a [[link]] is a navigation like any other.
  useEffect(() => setNavigate((id) => void select(id)), [select]);

  // Backlinks track the open note, and refresh when it is saved — editing a
  // note can add or remove the links pointing out of it.
  useEffect(() => {
    if (!selectedId) return setBacklinks([]);
    indexApi
      .backlinks(selectedId)
      .then(setBacklinks)
      .catch(() => setBacklinks([]));
  }, [selectedId, notes]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSearching((open) => !open);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function createNote(parent: string | null) {
    await note.flush();
    const created = await notesApi.create("Untitled", parent);
    await refresh();
    setSelectedId(created.id);
  }

  async function deleteNote(id: string) {
    await notesApi.remove(id);
    const remaining = await notesApi.list();
    setNotes(remaining);
    setTitles(remaining.map((n) => [n.id, n.title] as const));
    if (id === selectedId) setSelectedId(remaining[0]?.id ?? null);
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
            onClick={() => setSearching(true)}
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
            <div className="mx-auto max-w-content px-6 py-12">
              <Breadcrumbs
                notes={notes}
                id={selectedId}
                onSelect={(id) => void select(id)}
              />
              <input
                value={note.doc.title}
                onChange={(e) => note.setTitle(e.target.value)}
                placeholder="Untitled"
                aria-label="Note title"
                className="mb-4 w-full bg-transparent text-4xl font-semibold tracking-tight text-ink outline-none placeholder:text-ink-muted"
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
              <BacklinksPanel
                backlinks={backlinks}
                onSelect={(id) => void select(id)}
              />
            </div>
          ) : (
            <div className="grid h-full place-items-center px-6">
              <p className="text-ink-muted">
                {notes.length === 0 ? "Create a note to begin." : "Select a note."}
              </p>
            </div>
          )}
        </main>
      </div>

      {searching && (
        <SearchPanel
          onClose={() => setSearching(false)}
          onSelect={(id) => void select(id)}
        />
      )}

      {note.conflict && (
        <ConflictPrompt
          note={note.conflict}
          onResolve={(choice) => void note.resolveConflict(choice)}
        />
      )}
    </div>
  );
}
