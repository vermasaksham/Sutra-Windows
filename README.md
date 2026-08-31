# Sutra

**सूत्र** — "thread". A local-first, block-based notes app for materials-chemistry
research. Notion's writing experience, but the source of truth is plain markdown
files on disk, and LaTeX maths and chemical equations are first-class.

## The one architectural rule

**Markdown files are the source of truth. SQLite is a disposable index.**

- Every note is one `.md` file in a flat vault directory. No folder nesting.
- Hierarchy lives in YAML frontmatter, not the filesystem.
- SQLite holds only derived data: full-text search, the folder tree, backlinks.
- Delete the database and the app rebuilds it on next launch, losing nothing.
- Nothing is ever stored in SQLite that does not also exist in a markdown file.

Links between notes are `[[id]]` on disk and render as the target's title, so
renaming a note never breaks a link.

## Stack

| Layer | Choice |
| --- | --- |
| Shell | Tauri v2 |
| Backend | Rust |
| Frontend | React + TypeScript + Vite |
| Styling | Tailwind CSS v4 |
| Editor | TipTap (ProseMirror) |
| Maths | KaTeX + mhchem |
| Index | SQLite |
| Target | Windows 10+ primarily; macOS kept working where free |

Rust owns the filesystem, frontmatter, the SQLite index, search, backlinks,
file watching, and export. React owns everything visual, editor state,
navigation, and the markdown ↔ editor conversion. They meet only at Tauri
commands — no filesystem paths and no SQL cross that boundary.

**On markdown.** The original plan put markdown parsing and serialisation in
Rust. It lives in the frontend instead, via `@tiptap/markdown`: Rust reads and
writes the file and splits off the frontmatter, but treats the body as opaque
text. The deciding factor is Phase 5 — maths and chemistry have to round-trip
losslessly, and this way a maths node declares its own `parseMarkdown` /
`renderMarkdown` next to its schema, so `$$…$$` and `\ce{…}` are defined in
exactly one place instead of twice with an agreed-upon intermediate format
between them. Measured before adopting: all twelve supported block types
round-trip stably, eleven of twelve byte-identically. Rust still reads raw
markdown for the Phase 4 index — extracting search text and `[[id]]` links —
but that is extraction, never re-serialisation.

## Getting started

Toolchain setup for Windows is in [`docs/phase-0-toolchain.md`](docs/phase-0-toolchain.md).

```bash
npm install
npm run tauri:dev     # opens the app window with hot reload
```

Other scripts:

```bash
npm run dev           # frontend only, in a browser (the Rust bridge is inert)
npm run typecheck     # tsc --noEmit
npm run build         # typecheck + production frontend bundle
npm run tauri:build   # installer for the current platform
cargo check           # from src-tauri/ — compile the Rust side
```

## Layout

```
index.html            Sets data-theme before first paint
src/
  main.tsx            React entry
  App.tsx             The three-pane shell
  notes/
    useNote.ts        Buffer, autosave, external-change handling
    NoteHeader.tsx    Cover, icon, title, tags — all frontmatter
    IconPicker.tsx    Curated emoji, no dependency
    TagEditor.tsx     Tag chips; clicking one selects that tag
    shortcuts.ts      App-level keys only; the editor owns its own
    CommandPalette.tsx  Every action behind Ctrl/Cmd K
    TypePicker.tsx    What kind of note this is, changeable at any time
    MigrationPrompt.tsx  Offered once to a vault laid out the old way
    CitationMigrationPrompt.tsx  Zotero keys -> source notes, once
    tree.ts           Folder paths -> a tree, inferring missing parents
    tags.ts           Tag paths -> a tree, plus autocomplete. Unit tested.
    TagManager.tsx    Rename, merge and tidy tags across the vault
    SourcesPanel.tsx  What a note draws on, with page and quote
    SourceDetails.tsx A source note's paper, and what rests on it
    SourcePicker.tsx  The vault first, then Zotero
    folderStore.ts    The open note's folder, for the slash menu's attach
    Sidebar.tsx       Rail: the vault, the folder tree, the tag list
    NoteList.tsx      Middle column: rows and the search field
    FolderBar.tsx     Where the note lives, and the only way to move it
    BacklinksPanel.tsx
    VaultPicker.tsx   Shown until a vault is chosen
    ConflictPrompt.tsx
  editor/voices/      The three voices of a literature note. Unit tested.
  export/
    buildDocument.ts  Editor state -> a flat model Rust can write
    mathToImage.ts    Formulas rendered for export, and why via MathJax
    rasterise.ts      SVG -> PNG, for the raster copy Word insists on
  platform.ts         Ctrl or ⌘, and how to write a shortcut
  vault/api.ts        Typed wrappers over the Tauri commands
  components/
  editor/
    Editor.tsx        TipTap instance and the drag handle
    markdown.ts       Markdown ↔ editor, and why it lives here
    wikilink/         [[id]] node, its markdown spec, and [[ autocomplete
    math/             $…$ and $$…$$ nodes, KaTeX rendering, mhchem
    extensions.ts     The block vocabulary
    initialContent.ts Seed document — Phase 2 has no persistence
    icons.tsx         Inline SVGs, all currentColor
    slash/            The / menu: matching, items, and the popover
  theme/theme.ts      Theme preference, persistence, and OS following
  styles/
    tokens.css        Every colour in the app. The source of truth.
    index.css         Fonts, Tailwind, token bridging, base styles
    editor.css        Prose and editor chrome, plain CSS against the tokens
src-tauri/
  src/
    main.rs           Tauri builder and command registration
    commands.rs       The entire surface the frontend can call
    vault.rs          Vault operations: list, read, save, delete, attach
    index.rs          SQLite: tree, FTS5 search, backlinks. Disposable.
    protocol.rs       sutra:// scheme serving vault attachments
    export.rs         .docx writing
    zotero.rs         Reading references from a running Zotero
    links.rs          Finding [[id]] references in markdown
    citations.rs      Finding and rewriting [@ref] in markdown. Unit tested.
    frontmatter.rs    The YAML block, parsing and serialising
    note.rs           Filenames, slugging, atomic writes
    watcher.rs        Debounced filesystem watching
    state.rs          Open vault, and the remembered one
    error.rs          One error type for the storage layer
  tauri.conf.json     Window, CSP, bundle config
  capabilities/       What the frontend is permitted to call
```

## Visual identity

Source Sans 3 throughout — a humanist sans whose subscripts are properly drawn,
which matters for Sb₂Se₃. Body text 16px, line-height 1.65, content column
capped at 700px.

**Every colour is a CSS custom property, defined once in `src/styles/tokens.css`.
Components never hardcode a colour.** Theming is a single `data-theme` attribute
on `<html>`; because the light and dark blocks define the same variable names,
nothing downstream needs to know which theme is active.

The neutrals are warm, not grey — deliberate, and to be preserved. Body text is
off-white in dark mode, never pure white. Persimmon is for links and interactive
accents; teal is reserved strictly for highlights and tags.

The palette is drawn from the application icon, so the two belong to each other.
It replaces the indigo-and-saffron pair the original brief named — a deliberate
change, recorded at the top of `tokens.css` along with what to do to get the old
one back.

The window is three panes, after Bear: a rail naming what to look at, a list
choosing which note, and the page itself. Each sits on its own ground, darkest
to lightest, so the stack reads as depth rather than as three boxes. There is no
toolbar — the only chrome is a save state and an export menu floating over the
top-right of the page. Search lives at the head of the list rather than in an
overlay. Ctrl+K opens the command palette, Ctrl+Shift+F focuses search, and
Ctrl+N captures to the Inbox — on macOS the same bindings read ⌘K, ⌘⇧F and ⌘N,
because Windows is the primary target and macOS is the special case, not the
other way round.

Tags nest on slashes — `#research/materials/sb2se3` — and selecting a parent
finds everything beneath it. Renaming a tag brings its children with it, and
renaming onto a tag that already exists is a merge; both are one operation
because they are the same edit. Every retag records what each note's tags were,
so a merge can be undone exactly, which renaming back would not achieve.

There is no such thing as an unused tag here. A tag exists exactly while some
note carries it, so the brief's "unused-tag detection" has nothing to detect —
worth saying rather than shipping a button that can never fire.

## Sources and provenance

A source is a note. Not a Zotero key, not a row in the index — a note of
`type: source` in `Library/`, ordinary in every other way. Zotero is an import,
not a dependency, and the item key comes along so a second import updates that
note rather than making a duplicate.

That is the whole argument. Citations used to store a Zotero key and resolve it
live, which means a vault opened on another machine, or after Zotero is
uninstalled, has citations that resolve to nothing — the brief's "source
provenance is lost", reached by a route nobody would choose. Now a citation
names a note in the vault, and the details live in exactly one place, so six
notes citing one paper cannot drift into six different versions of it.

A citation lives in the citing note's own frontmatter with a page, a quote and
when it was captured. The quote is a field rather than prose because it is the
one piece of text in the note that is not yours.

`[@...]` in the body names a source note by its id, and that is the only
identifier a citation ever needs. Vaults written before this still hold
eight-character Zotero keys; those keep parsing, and render greyed as
`(not in Zotero: ABCD1234)` rather than vanishing, so nothing in a note is lost
by opening it. **Turn Zotero citations into sources** in the command palette
resolves each key once against a running Zotero, writes the source note, and
rewrites the body — a key Zotero no longer knows is left exactly as it was and
can be migrated later, and no note's `updated` is touched, because rewriting a
reference into the form that means the same thing is not an edit.

`## Source says`, `## My interpretation` and `## My question` are rendered as
three distinct voices. They are decorations on ordinary markdown headings, so
nothing reaches the file and the separation survives export and every other
editor. That makes it a convention the app can make obvious but cannot enforce —
which is the price of a format worth keeping, and worth paying.

## Scope

In: block editor, slash menu, drag handles, real folders, wikilinks, backlinks,
full-text search, tags, note types, an Inbox, a command palette, KaTeX + mhchem,
light/dark, autosave, keyboard shortcuts.

Deliberately out, and not up for discussion: database views, kanban boards,
relations, filtered tables. Sutra is a writing tool, not a database with a UI.
Also out: real-time collaboration, cloud sync, mobile apps, plugin systems.

Importing a source from Zotero reads its local API on 127.0.0.1, so nothing
leaves the machine. It must be enabled in Zotero → Settings → Advanced → "Allow
other applications on this computer to communicate with Zotero". Only importing
and migrating need it; once a source is a note, citing and reading it never ask
Zotero anything.

Export writes `.docx` directly. PDF goes through the system print dialog —
choose "Save as PDF" — with a print stylesheet doing the layout, so a PDF is
vector throughout: the formulas in it are the KaTeX ones from the screen.

Equations in a `.docx` are pictures, and they are vector pictures. KaTeX cannot
emit an image at all, so the export renderer is MathJax, which draws glyphs as
`<path>` elements rather than text in a font — an SVG that stands on its own.
Word since 2016 renders that SVG, so an exported equation stays sharp at any
zoom and prints at the printer's resolution.

Every such picture is written twice. Word's SVG support is an extension hanging
off a `<a:blip>` that must still name a PNG, so the package carries both, and a
reader that does not know the extension — LibreOffice, Word 2013 — falls back to
the raster copy. docx-rs cannot express this, so the finished package is read
back and rewritten; see `weave_vectors` in `export.rs`.

Both engines are correct TeX, so an exported formula is faithful to the editor's
but not identical in metrics.

## Build phases

Each phase ends with something that runs. No work starts on a later phase while
an earlier one is open.

- [x] **Phase 0 — toolchain.** Rust, Node, Tauri prerequisites. Git repo.
- [x] **Phase 1 — scaffold.** Tauri v2 + React + TS + Vite + Tailwind. A window
      opens, Source Sans 3 loads, both themes are defined and switchable.
- [x] **Phase 2 — block editor.** TipTap, core blocks, slash commands, drag
      handles, visual identity applied. In-memory only.
- [x] **Phase 3 — storage in Rust.** Vault selection, markdown + frontmatter
      read/write, autosave, file watching.
- [x] **Phase 4 — navigation.** SQLite index, sidebar tree, search, wikilinks,
      backlinks, breadcrumbs.
- [x] **Phase 5 — maths and chemistry.** KaTeX + mhchem, lossless markdown
      round-trip.
- [x] **Phase 6 — daily driver.** Shortcuts, tags, icons and covers, motion,
      empty states, error handling.
- [x] **Phase 7 — research tools.** Zotero, docx and PDF export.
