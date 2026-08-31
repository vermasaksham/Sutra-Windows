import { useCallback, useEffect, useState } from "react";
import Editor from "./editor/Editor";
import Toast from "./components/Toast";
import { setNavigate, setTitles } from "./editor/wikilink/titleStore";
import { setSources } from "./editor/citation/citationStore";
import BacklinksPanel from "./notes/BacklinksPanel";
import SourcesPanel from "./notes/SourcesPanel";
import { citedRefs } from "./notes/citedRefs";
import SourceDetails from "./notes/SourceDetails";
import ExportMenu from "./notes/ExportMenu";
import FolderBar from "./notes/FolderBar";
import ConflictPrompt from "./notes/ConflictPrompt";
import MigrationPrompt from "./notes/MigrationPrompt";
import CitationMigrationPrompt from "./notes/CitationMigrationPrompt";
import CommandPalette from "./notes/CommandPalette";
import TagManager from "./notes/TagManager";
import ViewEditor from "./notes/ViewEditor";
import NoteHeader from "./notes/NoteHeader";
import NoteList from "./notes/NoteList";
import Sidebar from "./notes/Sidebar";
import VaultPicker from "./notes/VaultPicker";
import { buildDocument } from "./export/buildDocument";
import { useShortcuts } from "./notes/shortcuts";
import { notesUnder } from "./notes/tree";
import { taggedWith } from "./notes/tags";
import { LITERATURE_TEMPLATE } from "./editor/voices/voiceRules";
import { setCurrentFolder } from "./notes/folderStore";
import { MOD, shortcut } from "./platform";
import { useNote } from "./notes/useNote";
import type { Editor as TiptapEditor } from "@tiptap/core";
import {
  exportApi,
  indexApi,
  foldersApi,
  legacyCitationsApi,
  migrationApi,
  notesApi,
  sourcesApi,
  vaultApi,
  viewsApi,
  type Backlink,
  type Citation,
  type MigrationPlan,
  type NoteSummary,
  type SourceMeta,
  type NoteType,
  type SearchHit,
  type VaultInfo,
  type ViewQuery,
  type ViewResult,
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
  /** Source notes, kept separately so a citation's id can be shown as a title. */
  const [sources, setSourceNotes] = useState<NoteSummary[]>([]);
  /** Set once, when a vault still records its hierarchy in frontmatter. */
  const [migration, setMigration] = useState<MigrationPlan | null>(null);
  const [migrating, setMigrating] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  /** Zotero keys still in note bodies. Null until looked for. */
  const [legacyCitations, setLegacyCitations] = useState<Record<
    string,
    number
  > | null>(null);
  const [citationPromptOpen, setCitationPromptOpen] = useState(false);
  const [tagsOpen, setTagsOpen] = useState(false);
  /** Every view note, for the rail. */
  const [views, setViews] = useState<NoteSummary[]>([]);
  /** The view the list is showing, if any. Exclusive with a folder or a tag. */
  const [activeView, setActiveView] = useState<string | null>(null);
  /** What that view currently matches. Null while it is being run. */
  const [viewResult, setViewResult] = useState<ViewResult | null>(null);
  /**
   * The view being edited: its id (null when it is a new one) and its query.
   * Null when the editor is closed.
   */
  const [editingView, setEditingView] = useState<{
    id: string | null;
    title: string;
    query: ViewQuery;
  } | null>(null);
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

  const refreshViews = useCallback(() => {
    viewsApi
      .list()
      .then(setViews)
      .catch(() => setViews([]));
  }, []);

  useEffect(() => {
    if (vault) refreshViews();
  }, [vault, refreshViews, notes]);

  /**
   * Run the open view.
   *
   * Re-run whenever the vault changes, because a view is a question and its
   * answer moves: writing a note that matches must make it appear here, and a
   * result list that goes stale is worse than no list at all.
   */
  useEffect(() => {
    if (!activeView) return setViewResult(null);
    let cancelled = false;
    viewsApi
      .read(activeView)
      .then((query) => viewsApi.run(query ?? {}))
      .then((found) => {
        if (!cancelled) setViewResult(found);
      })
      .catch((cause) => {
        if (!cancelled) {
          setViewResult(null);
          report("Could not run the view", cause);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [activeView, notes, report]);

  /**
   * Choose what the middle column lists: a folder, a tag, or a view.
   *
   * The three are alternatives, not filters that stack — a folder is where a
   * note lives, a tag is what it is about, and a view is a question about the
   * whole vault. Intersecting them would answer a question nobody asked. One
   * function rather than three call sites each clearing the other two, because
   * that is the shape where the fourth call site forgets one.
   */
  const show = useCallback(
    (what: {
      folder?: string | null;
      tag?: string | null;
      view?: string | null;
    }) => {
      setActiveFolder(what.folder ?? null);
      setActiveTag(what.tag ?? null);
      setActiveView(what.view ?? null);
      setViewResult(null);
    },
    [],
  );

  const selectView = useCallback(
    (id: string | null) => show({ view: id }),
    [show],
  );

  /** Open the query editor for a view, or for a new one. */
  const editView = useCallback(
    async (id: string | null) => {
      if (id === null) {
        setEditingView({ id: null, title: "", query: {} });
        return;
      }
      try {
        const query = await viewsApi.read(id);
        setEditingView({
          id,
          title: views.find((v) => v.id === id)?.title ?? "",
          query: query ?? {},
        });
      } catch (cause) {
        report("Could not read the view", cause);
      }
    },
    [views, report],
  );

  /** Write the edited query back, creating the view if it is a new one. */
  const saveView = useCallback(
    async (id: string | null, title: string, query: ViewQuery) => {
      const saved = id
        ? await viewsApi.save(id, query)
        : await viewsApi.create(title, query);
      setEditingView(null);
      await refresh();
      refreshViews();
      selectView(saved.id);
    },
    [refresh, refreshViews, selectView],
  );

  useEffect(() => {
    if (!vault) return;
    sourcesApi
      .list()
      .then((found) => {
        setSourceNotes(found);
        // The citation node views are mounted by ProseMirror, so they read the
        // sources from a module store rather than from props.
        setSources(found);
      })
      .catch(() => {
        setSourceNotes([]);
        setSources([]);
      });
  }, [vault, notes]);

  // Asked once per vault, on open. A vault laid out the old way still works —
  // its notes open and its links resolve — so this is an offer, not a gate.
  useEffect(() => {
    if (!vault) return;
    let cancelled = false;
    migrationApi
      .needed()
      .then((needed) => (needed ? migrationApi.plan() : null))
      .then((plan) => {
        if (!cancelled) setMigration(plan);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [vault]);

  const findLegacyCitations = useCallback(() => {
    legacyCitationsApi
      .find()
      .then((found) =>
        setLegacyCitations(Object.keys(found).length > 0 ? found : null),
      )
      .catch(() => setLegacyCitations(null));
  }, []);

  useEffect(() => {
    if (vault) findLegacyCitations();
  }, [vault, findLegacyCitations]);

  const runMigration = useCallback(async () => {
    setMigrating(true);
    try {
      await note.flush();
      await migrationApi.run();
      await refresh();
      refreshFolders();
      setMigration(null);
    } catch (cause) {
      report("Could not organise the vault", cause);
    } finally {
      setMigrating(false);
    }
  }, [note, refresh, refreshFolders, report]);

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

  /** Capture: a new empty note in the Inbox, with nothing to decide. */
  const capture = useCallback(async () => {
    try {
      await note.flush();
      const created = await notesApi.capture();
      await refresh();
      setSelectedId(created.id);
      // The note has no title, so NoteHeader puts the cursor there. The first
      // thing typed is what makes this findable in three months.
    } catch (cause) {
      report("Could not capture the note", cause);
    }
  }, [note, refresh, report]);

  const setType = useCallback(
    async (type: NoteType) => {
      if (!selectedId) return;
      try {
        await note.flush();
        const summary = await notesApi.setType(selectedId, type);
        note.applyMeta(summary);
        // A literature note starts from the three voices. Only into an empty
        // body, so this can never overwrite anything someone has written.
        if (type === "literature" && note.doc?.body.trim() === "") {
          note.setBody(LITERATURE_TEMPLATE);
          await note.flush();
        }
        await refresh();
      } catch (cause) {
        report("Could not change the note type", cause);
      }
    },
    [selectedId, note, refresh, report],
  );

  const setCitations = useCallback(
    async (citations: Citation[]) => {
      if (!selectedId) return;
      try {
        await note.flush();
        note.applyMeta(await sourcesApi.setCitations(selectedId, citations));
        await refresh();
      } catch (cause) {
        report("Could not update the sources", cause);
      }
    },
    [selectedId, note, refresh, report],
  );

  const setSourceMeta = useCallback(
    async (meta: SourceMeta) => {
      if (!selectedId) return;
      try {
        note.applyMeta(await sourcesApi.setMeta(selectedId, meta));
        await refresh();
      } catch (cause) {
        report("Could not update the source", cause);
      }
    },
    [selectedId, note, refresh, report],
  );

  useShortcuts({
    palette: () => setPaletteOpen(true),
    search: () => setFocusSearch((n) => n + 1),
    capture: () => void capture(),
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
        // Flush first: Rust reads the file to build the new summary, so an
        // unsaved edit would otherwise be read back as the note's real state.
        await note.flush();
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
        citedRefs(note.doc.body),
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
  // A tag selects everything beneath it too, or the tree in the rail would be
  // decoration: picking `research` must find notes filed under
  // `research/materials/sb2se3`.
  // Every tag in the vault, most-used first, for the tag editor's suggestions.
  // Derived from the notes already loaded rather than fetched: it has to be
  // current the moment a tag is added, and a round trip would lag a keystroke
  // behind. The order matters — suggestTags offers ties in this order, and the
  // spelling people already use is the one to steer towards.
  const tagUse = new Map<string, number>();
  for (const n of notes) {
    for (const t of n.tags) tagUse.set(t, (tagUse.get(t) ?? 0) + 1);
  }
  const allTags = [...tagUse.entries()]
    .sort(
      ([aTag, aUse], [bTag, bUse]) => bUse - aUse || aTag.localeCompare(bTag),
    )
    .map(([tag]) => tag);

  const openView = activeView
    ? (views.find((v) => v.id === activeView) ?? null)
    : null;

  // A view replaces the list rather than filtering it. Its results are notes
  // like any other — selecting one opens it where it actually lives.
  const listed = openView
    ? (viewResult?.notes ?? [])
    : activeTag
      ? notes.filter((n) => taggedWith(n, activeTag))
      : notesUnder(notes, activeFolder);

  return (
    <div className="sutra-shell flex h-screen">
      <div className="sutra-no-print contents">
        <Sidebar
          vaultName={vault.name}
          notes={notes}
          folders={folders}
          views={views}
          activeFolder={activeFolder}
          activeTag={activeTag}
          activeView={activeView}
          onSelectFolder={(folder) => show({ folder })}
          onSelectTag={(tag) => show({ tag })}
          onSelectView={selectView}
          onNewView={() => void editView(null)}
          onCapture={() => void capture()}
          onManageTags={() => setTagsOpen(true)}
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
            openView
              ? openView.title
              : activeTag
                ? `#${activeTag}`
                : activeFolder === null
                  ? "All notes"
                  : activeFolder
          }
          // A view's results come from wherever they come from, so every row
          // says which folder it is really in — the point being that opening
          // one opens the note in its own place, not inside the view.
          showFolders={
            activeView !== null || activeFolder === null || activeTag !== null
          }
          view={
            openView && viewResult
              ? {
                  description: viewResult.description,
                  truncated: viewResult.truncated,
                  ignored: viewResult.ignored,
                  onEdit: () => void editView(openView.id),
                }
              : null
          }
          selectedId={selectedId}
          onSelect={(id) => void select(id)}
          onDelete={(id) => void deleteNote(id)}
          onCreate={() => void createNote(activeFolder)}
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
              onSelectFolder={(folder) => show({ folder })}
              onMove={(folder) => void moveNote(folder)}
              onCreateFolder={(folder) => void createFolder(folder)}
            />
            <NoteHeader
              doc={note.doc}
              onTitle={note.setTitle}
              onIcon={(icon) => void setMeta({ icon })}
              onCover={() => void pickCover()}
              onTags={(tags) => void setMeta({ tags })}
              onSelectTag={(tag) => show({ tag })}
              onType={(type) => void setType(type)}
              allTags={allTags}
            />
            {note.doc.adopted && (
              <p className="mb-4 rounded-lg bg-highlight-bg px-3 py-2 text-sm text-highlight">
                This file had no frontmatter. Sutra will add one when you save.
              </p>
            )}
            {/*
              On a source, the details come first: they are what the note is,
              and the body below them is for your own notes about the paper.
              On every other kind of note the sources are a footer, because
              there the prose is the point and the citations support it.
            */}
            {note.doc.type === "source" && (
              <SourceDetails
                id={note.doc.id}
                meta={note.doc.source ?? {}}
                onChange={(meta) => void setSourceMeta(meta)}
                onOpen={(id) => void select(id)}
              />
            )}
            {note.doc.type === "view" && (
              // A view note opened as a note. Its body is the place to write
              // down why the view exists, so it is worth being able to open —
              // but it would be a dead end without a way back to the results.
              <div className="sutra-no-print mb-3 flex flex-wrap items-center gap-3 rounded-lg border border-border px-3 py-2 text-sm">
                <span className="min-w-0 flex-1 text-ink-soft">
                  This note is a saved view.
                </span>
                <button
                  type="button"
                  onClick={() => selectView(selectedId)}
                  className="text-accent transition-opacity duration-150 ease-out hover:opacity-80"
                >
                  Run it
                </button>
                <button
                  type="button"
                  onClick={() => void editView(selectedId)}
                  className="text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
                >
                  Edit the query
                </button>
              </div>
            )}
            <Editor
              key={`${note.doc.id}:${note.revision}`}
              body={note.doc.body}
              onChange={note.setBody}
              onReady={setEditor}
            />
            {note.doc.type !== "source" && (
              <SourcesPanel
                citations={note.doc.sources ?? []}
                sources={sources}
                inlineRefs={citedRefs(note.doc.body)}
                onChange={(citations) => void setCitations(citations)}
                onOpen={(id) => void select(id)}
                onReport={report}
              />
            )}
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
                  : `Pick one from the list, or press ${shortcut(MOD, "K")}.`}
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

      {paletteOpen && (
        <CommandPalette
          notes={notes}
          openNoteId={selectedId}
          onClose={() => setPaletteOpen(false)}
          onOpenNote={(id) => void select(id)}
          onNewNote={() => void createNote(activeFolder)}
          onCapture={() => void capture()}
          onSearch={(text) => {
            setQuery(text);
            setFocusSearch((n) => n + 1);
          }}
          onSetType={(type) => void setType(type)}
          onExportDocx={() => void exportDocx()}
          onExportPdf={exportPdf}
          onManageTags={() => setTagsOpen(true)}
          onNewView={() => void editView(null)}
          currentSearch={query}
          onSaveSearchAsView={() =>
            setEditingView({
              id: null,
              title: query.trim(),
              query: { all: [{ text: query.trim() }] },
            })
          }
          legacyCitations={
            legacyCitations ? Object.keys(legacyCitations).length : 0
          }
          onMigrateCitations={() => setCitationPromptOpen(true)}
          onReindex={() => void indexApi.reindex().then(() => refresh())}
        />
      )}

      {tagsOpen && (
        <TagManager
          onClose={() => setTagsOpen(false)}
          onChanged={() => void refresh()}
          onReport={report}
        />
      )}

      {editingView && (
        <ViewEditor
          title={editingView.title}
          query={editingView.query}
          folders={folders}
          tags={allTags}
          sources={sources}
          onCancel={() => setEditingView(null)}
          onSave={(title, query) => saveView(editingView.id, title, query)}
          onReport={report}
        />
      )}

      {citationPromptOpen && legacyCitations && (
        <CitationMigrationPrompt
          counts={legacyCitations}
          onClose={() => {
            setCitationPromptOpen(false);
            findLegacyCitations();
          }}
          onDone={() => {
            // The migration rewrote note bodies on disk, so the open buffer is
            // now behind the file. Re-read it before anything can write the
            // stale version back.
            void note.reload();
            void refresh();
          }}
          onReport={report}
        />
      )}

      {migration && (
        <MigrationPrompt
          plan={migration}
          busy={migrating}
          onRun={() => void runMigration()}
          onDismiss={() => setMigration(null)}
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
