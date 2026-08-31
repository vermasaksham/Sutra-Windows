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
    ViewEditor.tsx    Building a saved query, as a form with a live preview
    SourcesPanel.tsx  What a note draws on, with page and quote
    SourceDetails.tsx A source note's paper, and what rests on it
    SourcePicker.tsx  The vault first, then Zotero
    folderStore.ts    The open note's folder, for the slash menu's attach
    Sidebar.tsx       Rail: the vault, the folder tree, the tag list
    NoteList.tsx      Middle column: rows and the search field
    FolderBar.tsx     Where the note lives, and the only way to move it
    BacklinksPanel.tsx
    ContextPanel.tsx  The fourth column: sources, backlinks, related, siblings
    DuplicateReview.tsx  Two notes side by side, and the three real answers
    DuplicateList.tsx  The vault-wide pass, run from the palette
    useWideEnough.ts  Whether the window can afford four columns
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
    views.rs          The typed query, and its compiler. Unit tested.
    related.rs        Why one note is near another, and how near. Unit tested.
    duplicates.rs     Whether two notes are the same note. Unit tested.
    claims.rs         Numeric claims in prose, and when two differ. Unit tested.
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

The window is four panes: a rail naming what to look at, a list choosing which
note, the page itself, and a context panel saying what is near it. The first
three are Bear's, and each sits on its own ground, darkest to lightest, so the
stack reads as depth rather than as boxes; the fourth is closable and stands
down on its own when the window is too narrow for it, so the ordinary case is
still Bear's three.

There is no toolbar — the only chrome is a save state and an export menu
floating over the top-right of the page. Search lives at the head of the list
rather than in an overlay. Ctrl+K opens the command palette, Ctrl+Shift+F
focuses search, Ctrl+N captures to the Inbox, and Ctrl+\ shows or hides the
context panel. On macOS the same bindings read ⌘K, ⌘⇧F, ⌘N and ⌘\, because
Windows is the primary target and macOS is the special case, not the other way
round.

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

## The context panel

The fourth column, on `Ctrl+\`. Four lists, in order of how directly each is a
fact about the open note: what it draws on, what points at it, what resembles
it, and what it sits beside. Hidden automatically when the window cannot afford
four columns, and remembered otherwise.

**Every related note carries a line saying why.** That is the design, not a
nicety. A ranked list of neighbours with no reasons is one the reader cannot
check, so the first time it is wrong they have no way to tell — and after that
they stop looking at the panel entirely. So relatedness here is never a single
opaque number: it is a set of reasons, each a fact about the two notes that can
be verified at a glance, and the score is their sum. The sentence under a
result is the same data the ranking used, not an explanation written afterwards
to sound plausible.

Five signals feed it:

| Signal | Reads as | Why it is weighted where it is |
| --- | --- | --- |
| Shared source | `cites Zhou 2019 too` | The strongest ordinary signal. Two notes citing one paper is a deliberate act by one person about one paper; it cannot happen by accident. |
| Shared tag | `shares #sb2se3` | Weighted by inverse document frequency, so a tag on three notes says far more than one on half the vault — and scaled, or two ordinary tags would outrank a citation. |
| Shared project | `both in PhD Thesis` | A project is a note and belonging to one is linking to it, so this falls out of the link table with no project field anywhere in the data model. |
| Shared link | `both link to Phonon transport` | Co-citation, in the bibliometric sense. |
| Shared prose | `shares 7 distinctive words` | The weakest and noisiest, so it is scaled down and capped — a long note shares words with everything, and without a ceiling length alone would decide the ranking. |

"Distinctive" is read from FTS5's own term dictionary rather than guessed at: a
word in no other note finds nothing, a word in a quarter of them says nothing,
and the window between moves with the vault instead of being a constant someone
picked once. That window is what stops a shared lab preamble — same instrument,
same standard, same corrections — from making every run a neighbour of every
other.

The folder is a tiebreak and never a reason given: `same folder` beside a shared
source reads as though the folder were the point, and a folder of forty notes
would otherwise make forty neighbours. Notes already shown elsewhere in the
panel are left out — a backlink is a backlink, and a source this note cites is
in Sources with its page and quote. And nothing appears below a floor, because
a panel padded with weak rows is one the reader learns to ignore, which costs
more than an empty panel ever could.

**Asking what is near a note changes nothing.** No cached results, no
`lastViewed`, no stamped `updated` — the same rule as a saved view, tested the
same way. The panel is also given the body from the editor rather than
re-reading the file, so typing about a subject brings its neighbours up before
autosave has run.

The weights are calibrated against one realistic vault, which is evidence and
not proof. `cargo test judge_the_panel_on_a_realistic_vault -- --ignored
--nocapture` prints what the panel would say for every note in it; that test is
ignored because its output is for a person to judge, and a number standing in
for that judgement would be measuring something else.

Not built, and deliberately: "recently edited", which the note list already
orders by, and would be the same rows in a second place.

## Duplicates, and numbers that differ

Two suggestions that could each be obnoxious, scoped so they are not.

### Notes written twice

Candidates come from FTS on the title's words and are then compared on three
dull measures: the **normalised title** — lowercased, punctuation dropped,
words sorted, so "Thermal conductivity Sb2Se3" and "Sb2Se3 thermal
conductivity" collapse to the same string — plus title overlap and body
overlap. Neither overlap counts alone: a shared title with nothing else is two
notes called "Meeting", and a shared body under a different title is usually a
quotation, so the two are multiplied rather than added.

The floor is high on purpose. A false duplicate costs attention and, acted on
carelessly, a note; a missed one costs nothing, because the vault goes on
working exactly as it did.

A pair opens a **comparison with three buttons**, and both notes are shown in
full — deciding whether two notes are the same note is precisely the decision
an excerpt cannot support:

- **They are different notes** writes the pair into both files' frontmatter, so
  it is never offered again and the fact survives the index being deleted. It
  does not touch either note's `updated`: dismissing a suggestion is not an
  edit.
- **Merge** appends one body to the other under a `## Merged from …` heading
  rather than interleaving them, unions the tags and citations, repoints every
  `[[link]]` that pointed at the absorbed note, and puts it in the vault's
  trash. Nothing is deleted outright and which half was which stays legible.
  Which note survives is the reader's choice, made in the dialog.
- **Not now** does nothing, because "I will decide later" has to be available
  or the dialog becomes a thing to escape rather than to use.

`Find notes written twice` in the palette runs the same comparison across the
vault. It is a command and never a background nag.

### Numbers that differ

**Not contradiction detection.** Deciding that two passages of prose disagree
is a research problem; deciding that two numbers written as the same quantity
in the same unit differ by a factor is arithmetic. Only the arithmetic ships,
and the panel says only that: *two numeric claims differ*, with both quoted as
written and the ratio between them. Which is right — or whether they are even
about the same measurement — is not knowable from the text and is not claimed.

A claim is `label = value unit`: `κ = 0.037 W m⁻¹ K⁻¹`. The separator is
required, and that restriction is what makes the feature usable. Prose is full
of numbers — "ramped to 800 K", "the third run", "see page 6" — and a number
nobody wrote as an assertion is a number nobody was asserting; picking those up
would flag a ramp's start against its end and teach the reader to ignore the
panel by the second note. The label is required for the same reason: two
temperatures in kelvin are not in disagreement for being different
temperatures.

Units are canonicalised structurally — superscripts expanded, everything after
a solidus inverted, factors split and sorted — so `W m⁻¹ K⁻¹`, `W/mK` and
`W/(m·K)` all compare, while `m` and `M`, `s` and `S` stay distinct. Two
claims are only compared when both the label and the canonical unit match, and
only between notes that share a tag, a source or a link. Ranges are not claims,
`==` and `:=` are code, and a bare number only ever compares with another bare
number.

What it will miss: anything not written as an assignment, and any unit spelled
in a way the canonicaliser cannot line up with the other spelling — `mK` is
read as metre-kelvin, so a millikelvin claim only compares with another. Those
are false negatives, which cost nothing. The false positive is the one that
costs attention on every note, and every choice above errs away from it.

## Saved views

A view is a note. `type: view` in `Views/`, with the query in its own
frontmatter — so a view is backed up, synced, diffed and readable as plain text
like everything else, and deleting the index cannot lose one. Its body is yours:
the place to write down why the view exists, which is what keeps a saved search
from rotting into a list nobody remembers the purpose of.

The query is typed, not a string language:

```yaml
view:
  all:
    - under: Research/Sb2Se3
    - tag: method/xrd
  none:
    - tag: archive
  sort: title
```

`tag:xrd AND (type:literature OR type:experiment) -tag:archive` would be a
parser, a syntax to teach, an error-message design, and an escaping problem the
first time a tag has a space in it. A small tree of typed terms instead means
YAML does the parsing, the editor is a form rather than a text box, and every
term compiles to SQL that can use an index. The price is that the expressible
queries are the enumerated ones and no others, which is a boundary worth having.

`under:` and `tag:` compile to a half-open range rather than a `LIKE` or `GLOB`
prefix — both would need escaping (`Data [raw]` is an ordinary folder name) and
neither reliably seeks the index. A view over 5,000 notes is answered in under a
millisecond, from indexes only, with no table scanned; both are asserted, the
second by reading SQLite's own query plan.

**Evaluating a view touches no file.** A view is a question, and asking one must
not change the answer — no cached results, no `lastViewed`, no stamped
`updated`. That is a test, not an intention.

A term this build cannot read — a view written by a newer Sutra, or a typo — is
kept verbatim, written back unchanged, and left out of the results, with the
list saying so. Silently dropping half of someone's query on the next save is
data loss.

### The line this does not cross

A view is a **saved query**, not a database view. It has no schema of its own,
no columns and no per-view fields; nothing can be edited from inside one; and
its results are the notes themselves, opened in their real folders. Location,
tags and views stay three independent axes — selecting a view replaces the list
rather than filtering within a folder, because intersecting them would answer a
question nobody asked.

## Scope

In: block editor, slash menu, drag handles, real folders, wikilinks, backlinks,
full-text search, tags, note types, an Inbox, a command palette, KaTeX + mhchem,
light/dark, autosave, keyboard shortcuts.

In too, since Phase 7: hierarchical tags, note types, sources as notes, saved
views, the context panel, duplicate candidates, and numeric claims that
differ.

Deliberately out, and not up for discussion: database views, kanban boards,
relations, filtered tables. Sutra is a writing tool, not a database with a UI.
Saved views are the near miss — they are a saved query, which is a different
thing, and the paragraph above draws the line they may not cross. Also out:
real-time collaboration, cloud sync, mobile apps, plugin systems.

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
