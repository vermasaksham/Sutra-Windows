# Sutra — the manual

**सूत्र** — "thread". A notes app for materials-chemistry research that keeps
your notes as plain markdown files on your own disk.

This manual covers what the app does and how to make it do it. It is written
against the app as it actually behaves; where something is a limitation, it
says so rather than leaving you to find out.

---

## Contents

1. [First run](#1-first-run)
2. [The three columns](#2-the-three-columns)
3. [Writing](#3-writing)
4. [Maths and chemistry](#4-maths-and-chemistry)
5. [Tables](#5-tables)
6. [Links between notes](#6-links-between-notes)
7. [Tags](#7-tags)
8. [Note types](#8-note-types)
9. [Zotero, sources and citations](#9-zotero-sources-and-citations)
10. [Views — saved questions about the vault](#10-views--saved-questions-about-the-vault)
11. [The context panel](#11-the-context-panel)
12. [Duplicates and numbers that differ](#12-duplicates-and-numbers-that-differ)
13. [Export](#13-export)
14. [Assistance (optional AI)](#14-assistance-optional-ai)
15. [Appearance — themes and dark mode](#15-appearance--themes-and-dark-mode)
16. [Keyboard shortcuts](#16-keyboard-shortcuts)
17. [The command palette](#17-the-command-palette)
18. [What is on disk, and where](#18-what-is-on-disk-and-where)
19. [When something goes wrong](#19-when-something-goes-wrong)
20. [Known limits](#20-known-limits)

---

## 1. First run

The app opens on **Choose a vault**.

A vault is an ordinary folder. Pick an existing one or make a new one — your
Documents folder, a folder in OneDrive, anywhere you can write. Every note
becomes one `.md` file inside it, readable and editable by anything else on
your machine. Sutra keeps no copy of its own.

Choosing an empty folder is fine; you start with nothing and write the first
note. Choosing a folder that already has markdown in it is also fine — Sutra
reads what is there.

You can point Sutra at a different vault later; it remembers the last one you
opened.

---

## 2. The three columns

**The rail** (left) holds your collections:

- **Inbox**, pinned at the top. An ordinary folder, and where a capture lands
  when you have not said where the note belongs.
- **Folders** — real directories in the vault, up to four levels deep. The
  cap is deliberate: it keeps Windows paths under the 260-character limit once
  a long title and a `Documents\…` prefix are accounted for.
- **Views** — saved queries. See [§10](#10-views--saved-questions-about-the-vault).
- **Tags** — every tag in use, with counts.

At the foot of the rail: the Light / Dark / System switch, and a gear that
opens Settings ([§15](#15-appearance--themes-and-dark-mode)).

**The list** (middle) shows the notes in whatever you selected, with a search
field at the top (`Ctrl+Shift+F`). Search runs over the full text of every
note, not just titles.

**The page** (right) is the note itself: icon, cover, title, type, tags, then
the body.

**The context panel** slides in on the far right (`Ctrl+\`). It only appears
when the window is wide enough to hold it — on a narrow window there is
nowhere to put it, so the button is hidden rather than offered and doing
nothing.

---

## 3. Writing

Type. Everything saves itself; there is no save button. (`Ctrl+S` exists
anyway, for the muscle memory and to stop the browser's own save dialog
appearing.)

**The slash menu.** Press `/` on an empty line — or anywhere — and a menu
opens. Keep typing to filter it; press `Enter` to insert.

| Group  | Items                                                           |
| ------ | --------------------------------------------------------------- |
| Basic  | Text, Heading 1, Heading 2, Heading 3                           |
| Lists  | Bulleted list, Numbered list, To-do list                        |
| Blocks | Code, Quote, Divider, Table, Equation, Chemical equation, Image |
| Table  | _(only inside a table — see [§5](#5-tables))_                   |

The menu matches on the item's title, its hint, and extra keywords, so `/h1`,
`/title`, and `/heading` all reach Heading 1, and `/$$`, `/latex` or `/formula`
all reach Equation.

**One thing to know:** the menu closes when you type a space. Filter with a
single word — `/bullet`, not `/bulleted list`.

**Markdown shortcuts work as you type.** `# ` for a heading, `- ` for a
bullet, `> ` for a quote, ` ` ```for code,`---`for a divider. Bold,
italic, undo and the rest are the usual`Ctrl` combinations.

**Images.** `/image` opens a native file picker. The chosen file is _copied
into the vault_, into a hidden `.attachments` folder beside the note that uses
it — so the vault stays self-contained and a picture does not break when the
original moves.

**Icon and cover.** Click above the title to add an emoji icon or a cover
image. Both are optional and both live in the note's frontmatter.

---

## 4. Maths and chemistry

Maths is first-class here, rendered with KaTeX, with mhchem loaded for
chemical notation.

**Inline maths.** Type `$`, the LaTeX, then the closing `$`. The moment you
type the closing dollar, the whole thing becomes a rendered formula:

```
$E_g = 1.1\,\mathrm{eV}$
$\ce{Sb2Se3}$
```

The rule is deliberately narrow so it does not fire on prices or stray
dollars: there must be no space directly after the opening `$` or directly
before the closing one, and it must be on one line.

**Display maths.** Type `$$` at the start of a line and it becomes an empty
equation block, with the caret already inside it. Or use `/equation`.

**Chemistry.** `/chemistry` (also `/ce`, `/reaction`, `/mhchem`) inserts a
display block pre-filled with `\ce{}` and puts the caret inside the braces —
`\ce{}` is the part nobody remembers, and the formula is the part you came to
write. Inline, just type `$\ce{Sb2Se3}$`.

**Editing a formula.** Click any rendered formula to see its LaTeX; click away
to re-render. The source appears where the formula was — in the line for an
inline formula, on its own line for an equation — set in a monospaced accent
colour rather than inside an input box, so a sentence you are part-way through
still reads as a sentence while you fix the formula in the middle of it.

Press `Esc` when you are done and the caret returns to the document, just
after the formula, so you can carry on writing. After an equation that is a
fresh line below it; after an inline formula it is the rest of the sentence.

Everything round-trips to markdown as `$…$` and `$$…$$`, so a formula written
here is a formula in the file.

---

## 5. Tables

`/table` inserts a 3×3 grid with a header row.

**Removing one.** A table is the one block that Backspace will not remove —
ProseMirror empties the cells and leaves the grid. So put the caret in the
table, type `/`, and a **Table** group appears in the menu with:

- **Delete this table** — removes the whole grid
- Add a row below · Delete this row
- Add a column to the right · Delete this column

These five only appear while the caret is inside a table. Everywhere else they
would be menu items that do nothing.

---

## 6. Links between notes

Type `[[` and a menu of your notes opens; keep typing to filter, `Enter` to
link.

On disk the link is `[[<id>]]` — the target's permanent identifier, not its
title. In the editor it renders as the target's current title. **Renaming a
note therefore never breaks a link to it.** A link whose target no longer
exists is shown struck through rather than silently dropped; the text is still
in the file.

Click a link to follow it. The **Backlinks** section of the context panel
([§11](#11-the-context-panel)) shows every note that links here.

---

## 7. Tags

Tags sit under the title. Click to add one.

- Lowercased and de-duplicated, so the same tag typed two ways is one tag.
- Use slashes for hierarchy: `method/xrd`, `method/dsc`, `sample/sb2se3`.
- `Enter` **or** a comma commits a tag — people type tag lists with commas out
  of habit.
- `Tab` completes to the highlighted suggestion without committing it, so you
  can still refine it.
- Suggestions come from tags already in your vault. Before you have typed
  anything, it offers the tags you use most that this note does not have yet.

**Managing them.** `Ctrl+K` → _Manage tags — rename, merge, tidy_. Renaming a
tag rewrites it in every note that carries it; merging folds one into another.

---

## 8. Note types

Every note has a type, shown as a chip beside the title. It is a label with
consequences only where you ask for them — views can filter on it, and the
context panel treats projects and sources specially.

Note · Literature · Idea · Research question · Experiment · Project ·
Meeting · Task · Daily · Source

**View** is an eleventh type you cannot pick from the dropdown. A view is a
saved query, and a note turned into one by a dropdown would be a view with
nothing to run — views are made by saving a query.

---

## 9. Zotero, sources and citations

Zotero stores the literature. Sutra stores what you think about it. The two
are kept apart on purpose, and this section is the bridge between them.

```
Zotero item  →  Source note  →  Evidence  →  Interpretation  →  Question
```

### The fastest way in

`Ctrl+Shift+Z`, or **Cite a paper** at the foot of the rail, or `Ctrl+K` →
_Cite a paper — search Zotero_. Search your library by title, author, year,
DOI or journal. Each result offers three different things:

| Action              | What it does                                                     |
| ------------------- | ---------------------------------------------------------------- |
| **Literature note** | The note you write _about_ the paper, plus the source behind it. |
| **Add as source**   | Brings the paper in so it can be cited, and stops there.         |
| **Open in Zotero**  | Raises the item in Zotero itself.                                |

### Two ways to connect

**Settings → References** offers a choice, and it matters:

| Connection                  | What it needs                                                                  | What leaves your machine                                   |
| --------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| **Zotero on this computer** | Zotero running, with _Allow other applications…_ on in its Settings → Advanced | Nothing. This is the default.                              |
| **My Zotero account**       | An API key from `zotero.org/settings/keys` — nothing else                      | Your search terms, and the library entries that come back. |

The account connection works with Zotero closed, and on a machine where it is
not installed at all — which is the point of it. It is off unless you configure
it, and a half-configured account (box ticked, key not yet pasted) quietly keeps
using the local connector rather than failing on every keystroke.

**Paste the key and press Connect. That is the whole setup.** Sutra asks Zotero
who the key belongs to and fills in your numeric user ID itself. You never have
to find it — which matters, because the API needs a _number_ and Zotero's own
documentation warns that user IDs are not usernames. Entering a username
produces a 404 that reads exactly like an empty library, and that is the single
most likely reason a connection appears not to work.

If something is still wrong, the panel says which thing:

| Message                          | What to change                                           |
| -------------------------------- | -------------------------------------------------------- |
| _Zotero rejected the API key_    | The key. Check it copied whole, and still exists.        |
| _no read access to your library_ | Make a new key with **Allow library access** ticked.     |
| _could not reach zotero.org_     | The network, not the key.                                |
| _no library with that user ID_   | A stored ID from an older version — press Connect again. |

A key typed into the panel is stored in plain text in the app's config file.
Setting the `ZOTERO_API_KEY` environment variable instead stores nothing at all,
and takes precedence over anything typed. Read-only permission on the key is
enough — Sutra never writes to your library.

### Two notes, not one

Asking for a literature note creates **two** things, and the split is the whole
point:

- A **source note** holds the paper's details — authors, year, journal, DOI,
  abstract, citation key, item type, Zotero collections, and whether a PDF
  exists. Copied in once, so they survive Zotero being closed, uninstalled, or
  never installed on this machine again.
- A **literature note** holds your reading of it, cites the source, and starts
  as empty headings: Summary, Key Evidence, Important Quotes, My
  Interpretation, Research Questions, Limitations, Related Notes.

Nothing in those headings is filled in for you. The app supplies the shape of a
reading, never the reading.

### The abstract, and everything else Zotero said

The abstract is included, marked **as published** and set off from your own
text — so a summary the publisher wrote can never be mistaken at a glance for
one you wrote.

Everything shown about a paper is what Zotero actually returned. Nothing is
guessed at. If your library has no citation key — Zotero only has them when
Better BibTeX is installed — the source says **None in Zotero** rather than
showing an invented one, because a fabricated `@Ko2024` reads correctly in a
draft and then fails at the bibliography. The same rule holds for the DOI, the
year, the journal and the PDF: present, or plainly marked absent.

### Citing in the text

Type `@` in the body and a menu opens covering both the sources already in your
vault and your Zotero library. Picking a Zotero item brings it in first, so a
citation always points at a note in your vault rather than at an item in
another program.

### Citation styles

**Sutra does not format citations. Zotero does.**

That is the whole design. Formatting a citation correctly is the Citation Style
Language — a full specification with thousands of styles — and Zotero already
contains a complete implementation of it. So Sutra asks Zotero to render each
paper and caches the answer, which means every style in
`zotero.org/styles` works and the result matches what the same library produces
in Word.

Pick one in **Settings → References**:

| Style         | For                                     |
| ------------- | --------------------------------------- |
| **ACS**       | American Chemical Society. The default. |
| **RSC**       | Royal Society of Chemistry              |
| **Nature**    | Nature journals                         |
| **IEEE**      | Engineering                             |
| **APA 7th**   | Social sciences                         |
| **Vancouver** | Medicine                                |
| **Chicago**   | Notes and bibliography                  |
| **Harvard**   | Cite Them Right                         |

Any other CSL id works too — choose _Another style…_ and type it, for example
`elsevier-harvard`.

**The rendered citations are cached**, per style, on the source note. Three
consequences worth knowing:

- A styled citation still reads correctly with Zotero closed and no network.
- Switching to a style you have used before costs nothing and needs no
  connection.
- Changing the style only affects sources imported afterwards. **Settings →
  References → Restyle existing sources** brings the rest over, and tells you
  how many it re-rendered.

Anything not yet rendered in the current style falls back to a plain
`Author, Year` label and is marked as a fallback rather than passed off as the
style — because a citation that looks finished and is not is worse than one
that admits it.

### Bibliography

The context panel shows the reference list for the note, in the chosen style,
in **citation order** — not alphabetical, because ACS and Nature number by
first appearance and sorting would make the list disagree with the numbers in
your prose. One paper appears once however many times you cite it: three pages
of Ko 2024 is three pieces of evidence and one reference.

**copy** puts the whole list on the clipboard. Entries the library has not
rendered in this style are flagged in the list itself, so nothing unfinished
reaches a manuscript unnoticed.

### Evidence: the page, and their words

In the context panel (`Ctrl+\`), each citation carries:

- **Page** — a string, because "S12", "6-8" and "iv" are all real page
  references
- **Source evidence — their words** — the quote, marked and set off, because it
  is the one piece of text in the note that is not yours
- **Evidence** — what kind it is: experimental, computational, theoretical,
  review or observation

That is the provenance record. "Ko 2024 says thermal conductivity is low" is
not traceable; "Ko 2024, p. 6, _κ = 0.037 W m⁻¹K⁻¹_, experimental" is.

### Three voices, never mixed

A heading opening with **Source**, **Key Evidence**, **Important Quotes** or
**Abstract** is the paper's voice. One opening with **My interpretation** is
yours. One opening with **Research questions** or **My question** is what you
want to know. The editor renders the three differently, and they are ordinary
markdown headings, so the separation survives export and this program not
existing.

### Searching your library from the note list

Search the vault and any matching papers in your Zotero library appear
underneath, in their own group with a dashed border, never disguised as notes
you have already written. "Have I read anything about this?" and "have I
written anything about this?" are the same question asked twice.

### When Zotero is not running

Everything already imported keeps working: metadata, citation keys, evidence,
page references, quotes, and every note relationship. Nothing is deleted and
nothing is hidden. Only the live library is missing, and the app says so in
those words.

### Zotero collections are not folders

An item can sit in three Zotero collections while your notes about it live in
one folder somewhere else. Sutra records which collections a paper is in and
never mirrors them into your folder tree — the two hierarchies are independent,
and making either follow the other destroys that.

### What Sutra never does to your library

It reads. It does not edit your Zotero data, does not copy PDFs into the vault,
and does not generate bibliographies. If you had citations from an older build
that still point at Zotero keys, the command palette offers **Turn Zotero
citations into sources**, with a count of how many are left.

---

## 10. Views — saved questions about the vault

A view is a saved query that runs live. "Every experiment tagged
`method/xrd` since March that cites Zhang 2023" is a view.

Make one with `Ctrl+K` → _New view_, or run a search and then `Ctrl+K` → _Save
this search as a view_ — the moment you have just searched for something is
the moment you know what you want a view to find.

A view is built from three lists of conditions:

- **All of these** must hold
- **Any of these** — at least one must hold (empty means no such requirement)
- **None of these** may hold

The conditions available:

| Condition             | Matches                              |
| --------------------- | ------------------------------------ |
| In folder (and below) | The folder and everything beneath it |
| In folder (exactly)   | That folder only                     |
| Tagged                | Notes carrying a tag                 |
| Of type               | Notes of one type                    |
| Mentioning            | Full-text search                     |
| Cites source          | Notes citing a given source          |
| Links to note         | Notes linking to a given note        |
| Edited since          | Changed on or after a date           |
| Untouched since       | Not changed since a date             |

Then a **sort** — recently edited first, least recently edited first, by
title, or by folder — and a **limit** (200 by default).

Results run underneath the editor as you build it: a query you cannot see the
effect of is a guess.

A view is stored as a note, in its own frontmatter, so it lives in the vault
with everything else and syncs with it. A term written by a newer build that
this one cannot read is passed back through untouched rather than quietly
dropped.

---

## 11. The context panel

`Ctrl+\`. Everything the vault knows about the note you have open:

- **Sources** — the citations in this note, with page and quote.
- **Backlinks** — every note that links here.
- **Possibly the same note** — see [§12](#12-duplicates-and-numbers-that-differ).
- **Numbers that differ** — see [§12](#12-duplicates-and-numbers-that-differ).
- **Related** — notes that are near this one, each with _why_: a shared tag, a
  shared source, the same project, a note both link to, shared uncommon terms,
  or the same folder. A shared citation counts for more than a shared common
  tag; a shared rare term counts for more than a shared common one. The reason
  is always shown, so you can judge whether the suggestion is worth following.
- **In this folder** — the neighbours.

---

## 12. Duplicates and numbers that differ

**Possibly the same note.** Two notes with the same title, or with strongly
overlapping titles _and_ bodies, are surfaced as candidates. Nothing is ever
merged automatically. Open a pair and you get both side by side, and two
choices: **merge** into whichever you keep, or say they are **not the same** —
which is recorded in both notes' frontmatter, so it survives the index being
deleted and the suggestion never comes back.

Vault-wide: `Ctrl+K` → _Find notes written twice_.

**Numbers that differ.** When two notes state the same quantity in the same
unit and the values are at least **2× apart**, both are shown with the factor
between them.

The heading says _differ_, not _contradict_, and that is exact. Detecting that
two passages of prose disagree is a research problem; detecting that two
numbers written as the same quantity in the same unit differ is arithmetic,
and only the arithmetic is shipped. Which one is right — or whether they are
even about the same measurement — is not knowable from the text and is not
claimed. Units are compared after normalisation, so `W m⁻¹K⁻¹`, `W/mK` and
`W/(m·K)` are the same unit.

---

## 13. Export

The export menu on a note offers two routes, and they are genuinely different:

- **Word (.docx)** — writes a file. Equations become images (both a raster and
  a vector copy, so they stay sharp when printed). Pictures come along.
- **PDF** — hands off to the system print dialog. Choose _Save as PDF_ there.

---

## 14. Assistance (optional AI)

**Off by default, and off means off** — when assistance is disabled, the code
that could reach the network is never built. Nothing about your notes leaves
your machine.

Turn it on with `Ctrl+K` → _Turn on assistance_. You need an Anthropic API key,
supplied either in the settings panel or in the `ANTHROPIC_API_KEY`
environment variable — the environment wins where both are set, and is the
better place for it (see [§20](#20-known-limits)).

Three things it will do, for the note you have open:

- **Summarise** — a few sentences saying what the note is about
- **Tags** — up to five, strongly preferring tags your vault already uses
- **Questions** — up to four questions the note raises and does not answer

Every result is a **draft**. Nothing is applied until you accept it.

**What leaves this machine:** one note — the one you asked about — and, when
asking for tags, the vault's list of tag names. Not the vault, not the
neighbouring notes, not the file paths. A research vault is years of
unpublished work, and the amount of it that reaches a third party is kept to
the minimum the question needs.

The model defaults to Claude Opus 5 and can be changed in the same panel.

---

## 15. Appearance — themes and dark mode

Two separate choices, and neither decides the other.

**Mode** — Light, Dark, or System. System follows Windows and changes with it,
including when Windows switches on a schedule.

**Palette** — which colours. Every palette is drawn for both light and dark, so
switching to dark never throws away your palette and choosing a palette never
decides whether the app is light.

| Palette      | What it is                                                    |
| ------------ | ------------------------------------------------------------- |
| **Sutra**    | Persimmon and teal on warm paper. The default.                |
| **Indigo**   | The original design brief — indigo links, saffron highlights. |
| **Slate**    | Cool neutrals and one quiet blue. Nearly monochrome.          |
| **Contrast** | Pure grounds and heavy edges, for a lit room or a projector.  |

Open **Settings** with the gear at the bottom of the rail, or `Ctrl+K` →
_Settings — appearance and themes_. Each palette is previewed in its own
colours, in whichever mode you are currently in, so the swatches show what the
window would actually look like rather than a generic sample.

The quick Light / Dark / System switch stays at the bottom of the rail, since
that is the one appearance choice made often enough to deserve a permanent
control.

### Typography

**Settings → Typography** sets two fonts independently, because they are two
different jobs:

- **Reading font** — the note itself.
- **Interface font** — the rail, the list and the menus.

Someone who wants to write in a serif rarely wants their sidebar in one, and
tying the two together is why so many editors are set in a font nobody chose.
Below them: text size, line spacing and line width, with a specimen underneath
showing real subscripts and units rather than lorem ipsum — so you can see
whether Sb₂Se₃ and W m⁻¹K⁻¹ survive a font before committing to it.

Pick from the list — Sutra's own Source Sans 3, plus the faces Windows ships —
or choose _Another font…_ and type any family name your system knows.

### Bringing your own fonts

Name the font, press **Choose a file…**, and pick a `.woff2`, `.woff`, `.ttf`
or `.otf`. The file is **copied into Sutra's own folder**, not linked: a font
referenced where it happens to sit today stops working the moment that folder
moves, and nothing would be able to tell you why. The copy lives beside the
app's settings rather than in the vault, for the same reason the theme does — a
typeface belongs to the screen you are looking at, not to your notes, so
syncing a vault between two machines will not drag one machine's typeface onto
the other.

Imported fonts appear in both pickers. **remove** deletes the file, and
anything that was using it falls back to Sutra's own font rather than to a name
nothing can resolve.

### The editing toolbar

A toolbar sits with the note, carrying undo and redo, bold, italic, underline,
strikethrough and inline code, the three heading levels and body text, the
three list kinds, quote, code block, divider, equation, chemical equation,
table, and clear formatting.

It shows only what the editor can actually do where your caret is — a button
that would do nothing is not drawn, because a dead button reads as a bug in the
document rather than in the toolbar.

**Drag the handle** — the dots at its end — to any edge: top, bottom, left or
right. A shaded band shows where it will land before you let go, and the choice
is remembered between launches. It snaps to an edge rather than floating free,
so it can never end up half off-screen where you would have to rescue it.

Wherever you put it, it stays with you as you scroll.

- **Top.** It starts in the note, between the tags and the first line, and
  pins itself just under the status bar once the note scrolls up past it.
- **Bottom.** It floats at the bottom of the note area. The note keeps a little
  extra room at its end so the last line never ends up underneath it.
- **Left or right.** It floats beside the note, vertically centred on whatever
  you are reading. The note area keeps a clear strip for it, so it sits next to
  your text rather than over the first character of every line — and never over
  the sidebar.

Open or close the context panel, or resize the window, and a floating toolbar
moves with the note area rather than staying where it was.

**Two things worth knowing.**

Your choice is remembered per machine, in the app's own storage rather than in
the vault. It is a property of the screen you are looking at, not of your
notes, so syncing a vault between a desktop and a laptop does not drag one
machine's theme onto the other.

Printing and Word export ignore the theme entirely. Both always produce black
text on white, so exporting from dark mode does not hand you a black page.

### Motion

Sutra's movement is deliberately small — things fade and settle over about a
fifth of a second, and nothing slides across the screen or bounces.

If you have turned on **Show animations in Windows → off** (Settings →
Accessibility → Visual effects), Sutra follows it: every transition drops to
zero and states change instantly. Nothing else about the app changes.

## 16. Keyboard shortcuts

These are the app's own. The editor keeps all its usual ones — bold, italic,
lists, undo — unchanged.

| Shortcut       | Does                                |
| -------------- | ----------------------------------- |
| `Ctrl+K`       | Command palette                     |
| `Ctrl+Shift+F` | Focus the search field              |
| `Ctrl+N`       | Capture to Inbox                    |
| `Ctrl+\`       | Show or hide the context panel      |
| `Ctrl+S`       | Flush to disk (it autosaves anyway) |

`Cmd` works in place of `Ctrl` if you are on a Mac. Every binding uses a
modifier, so none of them can steal a keystroke mid-sentence.

If your reflex is that `Ctrl+K` opens search — it used to — the palette's
first offer for any text you type is to search the vault for it, so the reflex
still gets you there.

---

## 17. The command palette

`Ctrl+K`. Type to filter; it searches both commands and your notes by title,
so it doubles as a jump-to-note.

**Create** — Capture to Inbox · New note in the current folder · Cite a paper — search Zotero

**This note** _(when one is open)_ — Set type to … (each of the ten kinds) ·
Export as Word (.docx) · Print, or save as PDF

**Vault** — Manage tags · Find notes written twice · New view · Save this
search as a view _(when there is a search to save)_ · Turn Zotero citations
into sources _(when there are any)_ · Settings — appearance and themes ·
Assistance settings · Rebuild the search index

---

## 18. What is on disk, and where

**In your vault folder:**

```
MyVault/
  Inbox/                    ← where captures land; an ordinary folder
  Library/                  ← where new source notes are put, by convention
  Views/                    ← where saved views are kept, by convention
  Experiments/
    DSC on Sb2Se3.md        ← one note, one markdown file, named for its title
    .attachments/           ← pictures used by notes in this folder
  .sutra/
    trash/                  ← deleted notes, kept
    backups/                ← a timestamped copy before any bulk rewrite
```

A note file is YAML frontmatter followed by markdown:

```markdown
---
id: 01J8ZQK9V3R7M2C4X6P8N0T5W1
type: experiment
title: DSC on Sb2Se3, second batch
created: 2026-08-21T10:14:00Z
updated: 2026-09-01T09:02:11Z
tags:
  - method/dsc
  - sb2se3
sources:
  - id: 01J8Z…
    page: "4"
---

The body, as ordinary markdown. Maths as $\ce{Sb2Se3}$ and $$…$$.
```

The **file name** follows the title, so the folder is browsable in Explorer —
rename a note and the file is renamed with it, with a numeric suffix if that
name is taken. The `id` in the frontmatter is what never changes: it is the
note's real identity, and what `[[links]]` point at. That is the whole reason
renaming is safe.

Everything else is editable, by Sutra or by you in any editor — Sutra watches
the folder and picks up outside changes.

**Not in your vault** — machine-local, and safe to delete:

- The **search index**, a SQLite file under the app's data directory. It holds
  only derived data: full-text search, the folder tree, backlinks. Delete it
  and the app rebuilds it from your markdown, losing nothing. It is kept out
  of the vault on purpose — a half-synced SQLite file is worse than no index.
- **`sutra.json`**, in the app's config directory: which vault you last
  opened, and your assistance settings.

**The architectural rule, stated once:** markdown files are the source of
truth, and SQLite is disposable. Nothing is ever stored in the index that does
not also exist in a markdown file.

---

## 19. When something goes wrong

**"… changed on disk".** The file was edited by something else — another
editor, a sync client — while you had unsaved edits here. Choose which version
wins. If you see this box on notes you alone are editing, that is a bug; it was
one, and it is fixed.

**Search is missing something, or a note is stale.** `Ctrl+K` → _Rebuild the
search index_. Safe: it is rebuilt from the markdown.

**A deleted note.** Look in `.sutra/trash` inside the vault.

**Before any bulk rewrite** — a tag rename across the vault, a citation
migration — every markdown file is copied into a timestamped folder under
`.sutra/backups` first.

**A picture will not display.** Attachments must live in a `.attachments`
folder beside the note. Use `/image` rather than pasting in a path to a file
elsewhere on the disk.

---

## 20. Known limits

Stated plainly, so none of them is a surprise:

- **The installer is unsigned.** Windows SmartScreen will warn on first run —
  _More info_ → _Run anyway_. Signing needs a certificate; `docs/releasing.md`
  says which kind and why it is not just a build step.
- **Updates are not automatic.** Sutra can tell you a newer version exists —
  Settings → Version → _Check for updates_ — but it cannot install one, and it
  never checks on its own. Installing an update means downloading it from the
  releases page, which leaves your vault untouched.
- **The API key, if you put it in the settings panel, is stored in plain text**
  in `sutra.json`. Use the `ANTHROPIC_API_KEY` environment variable instead if
  that matters to you.
- **No version history.** The file on disk is the current version. Put the
  vault in git, OneDrive or Dropbox if you want history — it is plain text, so
  all three work well on it.
- **No OCR, and no PDF reading.** A PDF can be attached; its text is not
  indexed.
- **No mobile app, and no sync of its own.** Sync the vault folder with
  whatever you already use.
- **Folders go four levels deep**, no further.
- **The bibliography is a list, not a laid-out document.** The reference list
  is rendered by Zotero in your chosen style and can be copied, but Word export
  does not place a formatted bibliography section for you — paste it where you
  want it.
- **A styled citation needs the library once.** Until a source has been
  rendered, its citation falls back to a plain `Author, Year` label. Settings →
  References → Restyle existing sources fixes that in one go.
