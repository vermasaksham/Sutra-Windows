import { useCallback, useEffect, useState } from "react";
import Editor from "./editor/Editor";
import Toast from "./components/Toast";
import { setNavigate, setTitles } from "./editor/wikilink/titleStore";
import BacklinksPanel from "./notes/BacklinksPanel";
import Bibliography, { citationKeys } from "./notes/Bibliography";
import ExportMenu from "./notes/ExportMenu";
import FolderBar from "./notes/FolderBar";
import ConflictPrompt from "./notes/ConflictPrompt";
import NoteHeader from "./notes/NoteHeader";
import NoteList from "./notes/NoteList";
import Sidebar from "./notes/Sidebar";
import VaultPicker from "./notes/VaultPicker";
import { buildDocument } from "./export/buildDocument";
import { useShortcuts } from "./notes/shortcuts";
import { notesUnder } from "./notes/tree";
import { setCurrentFolder } from "./notes/folderStore";
import { useNote } from "./notes/useNote";
import type { Editor as TiptapEditor } from "@tiptap/core";
import {
  exportApi,
  indexApi,
  foldersApi,
  notesApi,
  vaultApi,
  type Backlink,
  type NoteSummary,
  type SearchHit,
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
  /** What is typed in the list's search field. "" means not searching. */
  const [query, setQuery] = useState("");
  /** Results for `query`, or null when there is no search running. */
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [activeTag, setActiveTag] = useState<string | null>(null);
  /** The folder the list is showing. null means the whole vault. */
  const [activeFolder, setActiveFolder] = useState<string | null>(null);
  const [folders, setFolders] = useState<string[]>([]);
  /** Bumped to move focus to the search field; the shortcut lives up here but
   *  the input is three components down. */
  const [focusSearch, setFocusSearch] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<TiptapEditor | null>(null);
  const [exporting, setExporting] = useState(false);

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
    },
    [note],
  );

  useEffect(() => setNavigate((id) => void select(id)), [select]);

  useEffect(() => {
    if (query.trim() === "") {
      setHits(null);
      return;
    }
    // Debounced, so a fast typist does not queue a query per keystroke.
    const timer = setTimeout(() => {
      indexApi
        .search(query)
        .then(setHits)
        .catch(() => setHits([]));
    }, 120);
    return () => clearTimeout(timer);
  }, [query, notes]);

  // The slash menu's attach item reads this, so an attachment lands beside the
  // note that will reference it.
  useEffect(() => {
    setCurrentFolder(note.doc?.folder ?? null);
  }, [note.doc?.folder]);

  useEffect(() => {
    if (!selectedId) return setBacklinks([]);
    indexApi
      .backlinks(selectedId)
      .then(setBacklinks)
      .catch(() => setBacklinks([]));
  }, [selectedId, notes]);

  const refreshFolders = useCallback(() => {
    foldersApi
      .list()
      .then(setFolders)
      .catch(() => setFolders([]));
  }, []);

  useEffect(refreshFolders, [refreshFolders, notes]);

  const createNote = useCallback(
    async (folder: string | null) => {
      try {
        await note.flush();
        const created = await notesApi.create("Untitled", folder);
        await refresh();
        setSelectedId(created.id);
      } catch (cause) {
        report("Could not create the note", cause);
      }
    },
    [note, refresh, report],
  );

  const moveNote = useCallback(
    async (folder: string) => {
      if (!selectedId) return;
      try {
        // Flush first: an autosave landing after the rename would write the
        // note back to where it used to be.
        await note.flush();
        const summary = await notesApi.move(selectedId, folder);
        // The open note's folder is part of what the page shows, and the doc in
        // the buffer still says where it used to be. Merge rather than re-read:
        // re-reading would replace the body with what is on disk.
        note.applyMeta(summary);
        await refresh();
        refreshFolders();
      } catch (cause) {
        report("Could not move the note", cause);
      }
    },
    [selectedId, note, refresh, refreshFolders, report],
  );

  const createFolder = useCallback(
    async (folder: string) => {
      try {
        await foldersApi.create(folder);
        refreshFolders();
      } catch (cause) {
        report("Could not create the folder", cause);
      }
    },
    [refreshFolders, report],
  );

  useShortcuts({
    search: () => setFocusSearch((n) => n + 1),
    newNote: () => void createNote(activeFolder),
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
    async (patch: {
      icon?: string | null;
      cover?: string | null;
      tags?: string[];
    }) => {
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

  async function exportDocx() {
    if (!note.doc || !editor) return;
    setExporting(true);
    try {
      // Built here rather than in Rust because the pieces that need a browser
      // — rasterising formulas, reading attachments — only exist on this side.
      const document = await buildDocument(
        note.doc.title,
        editor.getJSON(),
        citationKeys(note.doc.body),
      );
      const saved = await exportApi.docx(document);
      if (saved) setError(`Exported ${saved}`);
    } catch (cause) {
      report("Could not export", cause);
    } finally {
      setExporting(false);
    }
  }

  function exportPdf() {
    // The webview's print dialog offers "Save as PDF" on every platform we
    // target. The print stylesheet is what makes the output worth having.
    window.print();
  }

  async function pickCover() {
    try {
      const reference = await notesApi.attach(note.doc?.folder ?? null);
      if (reference) await setMeta({ cover: reference });
    } catch (cause) {
      report("Could not add the cover", cause);
    }
  }

  if (!checked) return null;
  if (!vault) {
    return (
      <VaultPicker onOpened={() => void vaultApi.current().then(setVault)} />
    );
  }

  // What the middle column lists. A folder or a tag narrows it; a search
  // replaces it. Selecting a folder includes everything beneath it, because a
  // parent that held only subfolders would otherwise look empty.
  const listed = activeTag
    ? notes.filter((n) => n.tags.includes(activeTag))
    : notesUnder(notes, activeFolder);

  return (
    <div className="sutra-shell flex h-screen">
      <div className="sutra-no-print contents">
        <Sidebar
          vaultName={vault.name}
          notes={notes}
          folders={folders}
          activeFolder={activeFolder}
          activeTag={activeTag}
          onSelectFolder={(folder) => {
            setActiveFolder(folder);
            setActiveTag(null);
          }}
          onSelectTag={(tag) => {
            setActiveTag(tag);
            if (tag !== null) setActiveFolder(null);
          }}
          onNewNote={() => void createNote(activeFolder)}
          onNewFolder={(parent) => {
            const name = window.prompt(
              parent
                ? `New folder inside ${parent}`
                : "New folder at the top level",
            );
            if (name?.trim())
              void createFolder(
                parent ? `${parent}/${name.trim()}` : name.trim(),
              );
          }}
        />

        <NoteList
          notes={listed}
          hits={hits}
          query={query}
          onQuery={setQuery}
          heading={
            activeTag
              ? `#${activeTag}`
              : activeFolder === null
                ? "All notes"
                : activeFolder
          }
          showFolders={activeFolder === null || activeTag !== null}
          selectedId={selectedId}
          onSelect={(id) => void select(id)}
          onDelete={(id) => void deleteNote(id)}
          focusSearch={focusSearch}
        />
      </div>

      <main className="sutra-main relative min-w-0 flex-1 overflow-y-auto bg-surface">
        {/*
          The window's only chrome, and it floats over the page rather than
          sitting in a bar above it — the point of this layout is that the note
          is the window. Sticky so it stays reachable while reading.
        */}
        <div className="sutra-no-print sticky top-0 z-10 flex items-center justify-end gap-3 bg-surface/85 px-4 py-2 backdrop-blur-sm">
          <span
            className="text-xs tabular-nums text-ink-muted"
            aria-live="polite"
          >
            {SAVE_LABEL[note.saveState]}
          </span>
          <ExportMenu
            onDocx={() => void exportDocx()}
            onPdf={exportPdf}
            busy={exporting}
          />
        </div>

        {note.doc ? (
          // `group/page` so the icon and cover affordances appear on
          // approach rather than sitting there permanently.
          <div className="sutra-page group/page mx-auto max-w-content px-8 pt-2 pb-16">
            <FolderBar
              folder={note.doc.folder}
              folders={folders}
              onSelectFolder={(folder) => {
                setActiveFolder(folder);
                setActiveTag(null);
              }}
              onMove={(folder) => void moveNote(folder)}
              onCreateFolder={(folder) => void createFolder(folder)}
            />
            <NoteHeader
              doc={note.doc}
              onTitle={note.setTitle}
              onIcon={(icon) => void setMeta({ icon })}
              onCover={() => void pickCover()}
              onTags={(tags) => void setMeta({ tags })}
              onSelectTag={setActiveTag}
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
              onReady={setEditor}
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
                  : "Pick one from the list, or search with Ctrl K."}
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
