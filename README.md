# Sutra

**सूत्र** — "thread". A local-first, block-based notes app for materials-chemistry
research. Notion's writing experience, but the source of truth is plain markdown
files on disk, and LaTeX maths and chemical equations are first-class.

## The one architectural rule

**Markdown files are the source of truth. SQLite is a disposable index.**

- Every note is one `.md` file in a flat vault directory. No folder nesting.
- Hierarchy lives in YAML frontmatter, not the filesystem.
- SQLite holds only derived data: full-text search, the page tree, backlinks.
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

Rust owns the filesystem, markdown parsing and serialisation, the SQLite index,
search, backlinks, file watching, and export. React owns everything visual,
editor state, and navigation. They meet only at Tauri commands — no filesystem
paths and no SQL cross that boundary.

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
  App.tsx             Phase 1 shell — replaced by the editor in Phase 2
  components/
  theme/theme.ts      Theme preference, persistence, and OS following
  styles/
    tokens.css        Every colour in the app. The source of truth.
    index.css         Fonts, Tailwind, token bridging, base styles
src-tauri/
  src/main.rs         Tauri builder and commands
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
off-white in dark mode, never pure white. Indigo is for links and interactive
accents; saffron is reserved strictly for highlights and tags.

## Scope

In: block editor, slash menu, drag handles, nested pages, breadcrumbs,
wikilinks, backlinks, full-text search, tags, KaTeX + mhchem, light/dark,
autosave, keyboard shortcuts.

Deliberately out, and not up for discussion: database views, kanban boards,
relations, filtered tables. Sutra is a writing tool, not a database with a UI.
Also out: real-time collaboration, cloud sync, mobile apps, plugin systems.

Deferred: Zotero citations, `.docx` and PDF export.

## Build phases

Each phase ends with something that runs. No work starts on a later phase while
an earlier one is open.

- [x] **Phase 0 — toolchain.** Rust, Node, Tauri prerequisites. Git repo.
- [x] **Phase 1 — scaffold.** Tauri v2 + React + TS + Vite + Tailwind. A window
      opens, Source Sans 3 loads, both themes are defined and switchable.
- [ ] **Phase 2 — block editor.** TipTap, core blocks, slash commands, drag
      handles, visual identity applied. In-memory only.
- [ ] **Phase 3 — storage in Rust.** Vault selection, markdown + frontmatter
      read/write, autosave, file watching.
- [ ] **Phase 4 — navigation.** SQLite index, sidebar tree, search, wikilinks,
      backlinks, breadcrumbs.
- [ ] **Phase 5 — maths and chemistry.** KaTeX + mhchem, lossless markdown
      round-trip.
- [ ] **Phase 6 — daily driver.** Shortcuts, tags, icons and covers, motion,
      empty states, error handling.
- [ ] **Phase 7 — research tools.** Zotero, docx and PDF export.
