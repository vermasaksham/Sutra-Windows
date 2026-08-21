import { useCallback, useEffect, useState } from "react";
import Editor from "./editor/Editor";
import ThemeToggle from "./components/ThemeToggle";
import ConflictPrompt from "./notes/ConflictPrompt";
import NoteList from "./notes/NoteList";
import VaultPicker from "./notes/VaultPicker";
import { useNote } from "./notes/useNote";
import { notesApi, vaultApi, type NoteSummary, type VaultInfo } from "./vault/api";

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

  const refresh = useCallback(async () => {
    try {
      setNotes(await notesApi.list());
    } catch {
      setNotes([]);
    }
  }, []);

  const note = useNote(selectedId, refresh);

  // The vault from last session is reopened in Rust before the window appears,
  // so this only has to ask whether one is already open.
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

  // Select the first note once a vault's contents arrive.
  useEffect(() => {
    if (!selectedId && notes.length > 0) setSelectedId(notes[0]!.id);
  }, [notes, selectedId]);

  async function createNote() {
    await note.flush();
    const created = await notesApi.create("Untitled");
    await refresh();
    setSelectedId(created.id);
  }

  async function deleteNote(id: string) {
    await notesApi.remove(id);
    const remaining = await notesApi.list();
    setNotes(remaining);
    if (id === selectedId) setSelectedId(remaining[0]?.id ?? null);
  }

  async function select(id: string) {
    // Flush first, so switching away never drops the last few keystrokes.
    await note.flush();
    setSelectedId(id);
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
          <span
            className="text-xs text-ink-muted tabular-nums"
            aria-live="polite"
          >
            {SAVE_LABEL[note.saveState]}
          </span>
          <ThemeToggle />
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <NoteList
          notes={notes}
          selectedId={selectedId}
          onSelect={(id) => void select(id)}
          onCreate={() => void createNote()}
          onDelete={(id) => void deleteNote(id)}
        />

        <main className="min-w-0 flex-1 overflow-y-auto">
          {note.doc ? (
            <div className="mx-auto max-w-content px-6 py-12">
              <input
                value={note.doc.title}
                onChange={(e) => note.setTitle(e.target.value)}
                placeholder="Untitled"
                aria-label="Note title"
                className="mb-4 w-full bg-transparent text-4xl font-semibold tracking-tight text-ink outline-none placeholder:text-ink-muted"
              />
              {note.doc.adopted && (
                <p className="mb-4 rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
                  This file had no frontmatter. Sutra will add one when you
                  save.
                </p>
              )}
              {/*
                Keyed on the note and its revision so the editor remounts on a
                switch or an external reload. Remounting resets undo history,
                which is correct: undo must not reach across notes, or back
                past content that arrived from disk.
              */}
              <Editor
                key={`${note.doc.id}:${note.revision}`}
                body={note.doc.body}
                onChange={note.setBody}
              />
            </div>
          ) : (
            <div className="grid h-full place-items-center px-6">
              <p className="text-ink-muted">
                {notes.length === 0
                  ? "Create a note to begin."
                  : "Select a note."}
              </p>
            </div>
          )}
        </main>
      </div>

      {note.conflict && (
        <ConflictPrompt
          note={note.conflict}
          onResolve={(choice) => void note.resolveConflict(choice)}
        />
      )}
    </div>
  );
}
