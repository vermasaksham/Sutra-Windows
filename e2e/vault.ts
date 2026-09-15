import type { Page } from "@playwright/test";

/**
 * A vault in memory, injected in place of Tauri's IPC.
 *
 * The app talks to Rust through `window.__TAURI_INTERNALS__.invoke`, which does
 * not exist in a browser. Everything below answers those calls from a plain
 * object, so the real React app — the real editor, the real stores, the real
 * markdown round-trip — runs against a vault the test controls.
 *
 * What this does NOT cover, said plainly so nobody reads a green run as more
 * than it is: none of the Rust is exercised. Atomic writes, the SQLite index,
 * frontmatter parsing and the Zotero client have their own tests in
 * `src-tauri`. This suite covers the half of the app those tests cannot see.
 */

export type Note = {
  id: string;
  type?: string;
  title: string;
  folder?: string;
  tags?: string[];
  body: string;
  /** What a source note records about its paper, including what the library
   *  has rendered for it. `styled` is keyed by style id. */
  source?: {
    authors?: string;
    year?: string;
    doi?: string | null;
    /** The Zotero item key it was imported from. */
    zotero?: string;
    /** The attachment's title, as the library names it. */
    pdf?: string;
    styled?: Record<string, { citation?: string; bib?: string }>;
  };
  /** The note's recorded citations, as frontmatter holds them. */
  sources?: Array<{
    eid?: string;
    id: string;
    page?: string;
    quote?: string;
    /** The reader's own words. Never merged into `quote`. */
    comment?: string;
    /** Zotero's annotation key, when this came from one. */
    annotation?: string;
    colour?: string;
    /** The nth page of the file. Not `page`, which is the number printed on
     *  the paper — a selection knows the first and not the second. */
    page_index?: number;
    /** "selection", "annotation" or "manual". */
    origin?: string;
    /** `"source"` when the record lives on the paper and this is a reference. */
    at?: string;
  }>;
  /** On a note of `type: source`: evidence taken from this paper, shared so
   *  more than one note can rest on it. */
  evidence?: Array<{
    eid: string;
    page?: string;
    page_index?: number;
    quote?: string;
    kind?: string;
    origin?: string;
  }>;
  /** On a note of `type: chapter`: the notes it assembles, in order. */
  sequence?: string[];
};

export type Reference = {
  key: string;
  title: string;
  creators: string;
  year: string;
  itemType: string;
  doi?: string | null;
};

export type VaultOptions = {
  notes: Note[];
  /** Zotero items the fake library will return. Empty means "nothing found". */
  library?: Reference[];
  /** Make every Zotero call fail, the way a closed Zotero does. */
  zoteroDown?: boolean;
  /** Let the library be searched, but fail the import a pick triggers.
   *  Zotero going away between the search and the Enter, which is the one
   *  window where a failure has no menu left to be shown on. */
  importFails?: boolean;
  theme?: "light" | "dark";
  palette?: string;
  /** Which edge the editing toolbar starts on. */
  dock?: "top" | "bottom" | "left" | "right";
  /** The citation style in force. Must match the keys in a note's
   *  `source.styled` for the library's rendering to be used. */
  style?: string;
  /** Source note id -> how many notes cite it, for the research overview. */
  citations?: Record<string, number>;
  withPage?: number;
  withQuote?: number;
  /** What a check for updates should report, or "fail" to make it error. */
  update?: { current: string; latest: string; newer: boolean } | "fail";
  /** Make `app_version` fail, the way a broken IPC call would. */
  versionFails?: boolean;
  /** Files claiming an id another file already claims. */
  idClashes?: Array<{ id: string; opened: string; shadowed: string }>;
  /** What reading a PDF should produce. Every ending is a value, so a test
   *  names the state it wants rather than arranging for a failure. Defaults to
   *  one page of text. */
  pdf?:
    | { state: "text"; pages: { number: number; text: string }[] }
    | { state: "noTextLayer" }
    | { state: "notAttached" }
    | { state: "unresolved"; why: string }
    | { state: "missing"; detail: string }
    | { state: "locked" }
    | { state: "failed"; detail: string };
  /** Zotero's highlights on the paper. */
  annotations?: Array<{
    key: string;
    kind?: string;
    text?: string;
    comment?: string;
    colour?: string;
    page?: string;
    sortIndex?: string;
  }>;
};

/**
 * Install the stub. Must be called before `page.goto`, because the app reads
 * the vault during its first render.
 */
export async function useVault(page: Page, options: VaultOptions) {
  await page.addInitScript((opts: VaultOptions) => {
    const notes = opts.notes.map((n) => ({
      id: n.id,
      type: n.type ?? "note",
      title: n.title,
      folder: n.folder ?? "",
      position: 0,
      tags: n.tags ?? [],
      icon: null,
      cover: null,
      excerpt: n.body.slice(0, 60),
      updated: "2026-08-21T10:14:00Z",
      body: n.body,
      source: n.source as undefined | Record<string, unknown>,
      sources: n.sources ?? [],
      // On a source note: evidence the paper owns, which more than one note
      // can rest on. Empty everywhere else, exactly as Rust has it.
      evidence: n.evidence ?? [],
      // Mutable: reordering a chapter writes it back through `save_sequence`,
      // the way the real one writes frontmatter.
      sequence: n.sequence ?? [],
    }));

    if (opts.theme) localStorage.setItem("sutra.theme", opts.theme);
    if (opts.palette) localStorage.setItem("sutra.palette", opts.palette);
    if (opts.dock) localStorage.setItem("sutra.toolbar", opts.dock);

    // Every body the app has written, newest last. The assertion that matters
    // most in this suite is what reaches disk, not what is on screen.
    const saved: string[] = [];
    (window as unknown as { __saved: string[] }).__saved = saved;
    // The same array the handlers mutate, so a test can ask where evidence
    // landed rather than inferring it from what is on screen.
    (window as unknown as { __notes: unknown }).__notes = notes;

    // Every document handed to the exporter. Rust is what turns one of these
    // into a .docx and has its own tests for that; what this suite can see —
    // and what the v0.2 export defects all lived in — is whether the frontend
    // put the note's content into the document at all.
    const exported: unknown[] = [];
    (window as unknown as { __exported: unknown[] }).__exported = exported;

    const summary = (n: (typeof notes)[number]) => {
      const { body, ...rest } = n;
      void body;
      return rest;
    };

    const library = opts.library ?? [];
    const find = (id: string) => notes.find((n) => n.id === id);

    (
      window as unknown as { __TAURI_INTERNALS__: unknown }
    ).__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
        const zotero = () => {
          if (opts.zoteroDown) throw new Error("could not reach Zotero");
        };
        switch (cmd) {
          case "current_vault":
            return { name: "test-vault" };
          case "list_notes":
            return notes.map(summary);
          // Served from the same array as list_notes, so a source imported
          // mid-test resolves to a label instead of "not in this vault".
          // A substring match over title and body. Not FTS5 — that is Rust's
          // job and has its own tests — but enough that a test can search and
          // get the note it is looking for.
          // The vault-wide scan behind the research overview. Headings are
          // returned unclassified — voice is decided in TypeScript, in one
          // place — exactly as Rust does it.
          // The same join Rust does, over the same array: shared records
          // first, then who uses them, so one quotation is one item however
          // many notes rest on it.
          // Moves the record onto the paper and reduces the note's entry to
          // a reference — the same two writes, in the same order, as Rust.
          case "share_evidence": {
            const note = find(args.id as string);
            const record = note?.sources?.find((c) => c.eid === args.eid);
            if (!note || !record || record.at)
              return note ? summary(note) : null;
            const paper = notes.find((n) => n.id === record.id);
            if (!paper) return summary(note);
            paper.evidence = paper.evidence ?? [];
            if (!paper.evidence.some((e) => e.eid === record.eid)) {
              paper.evidence.push({
                eid: record.eid!,
                page: record.page,
                page_index: record.page_index,
                quote: record.quote,
                kind: record.kind,
                origin: record.origin,
              });
            }
            record.at = "source";
            delete record.page;
            delete record.page_index;
            delete record.quote;
            delete record.kind;
            delete record.origin;
            return summary(note);
          }
          case "all_evidence": {
            const items = new Map<string, Record<string, unknown>>();
            for (const note of notes) {
              for (const record of note.evidence ?? []) {
                items.set(record.eid, {
                  ...record,
                  source: note.id,
                  source_title: note.title,
                  shared: true,
                  used_by: [],
                });
              }
            }
            for (const note of notes) {
              for (const citation of note.sources ?? []) {
                const use = {
                  note: note.id,
                  title: note.title,
                  comment: citation.comment ?? null,
                };
                const held = items.get(citation.eid ?? "");
                if (held) {
                  (held.used_by as unknown[]).push(use);
                  continue;
                }
                const paper = notes.find((n) => n.id === citation.id);
                items.set(citation.eid ?? `${note.id}:${citation.id}`, {
                  ...citation,
                  source: citation.id,
                  source_title: paper?.title ?? "Source note missing",
                  shared: false,
                  used_by: [use],
                });
              }
            }
            return [...items.values()];
          }
          case "research_overview": {
            const headings = notes.flatMap((n) => {
              const found: Array<{
                note: string;
                noteTitle: string;
                text: string;
                words: number;
              }> = [];
              for (const line of n.body.split("\n")) {
                const heading = /^#{1,6}\s+(.*)$/.exec(line.trim());
                if (heading) {
                  found.push({
                    note: n.id,
                    noteTitle: n.title,
                    text: heading[1]!.trim(),
                    words: 0,
                  });
                } else if (found.length > 0) {
                  found[found.length - 1]!.words += line
                    .trim()
                    .split(/\s+/)
                    .filter(Boolean).length;
                }
              }
              return found;
            });
            const sourceNotes = notes.filter((n) => n.type === "source");
            return {
              headings,
              citations: opts.citations ?? {},
              sources: sourceNotes.map(summary),
              withPage: opts.withPage ?? 0,
              withQuote: opts.withQuote ?? 0,
            };
          }

          case "search_notes": {
            const q = String(args.query ?? "").toLowerCase();
            if (!q) return [];
            return notes
              .filter((n) => (n.title + " " + n.body).toLowerCase().includes(q))
              .map((n) => ({ id: n.id, title: n.title, excerpt: n.excerpt }));
          }
          case "list_sources":
            return notes.filter((n) => n.type === "source").map(summary);
          case "read_note": {
            const note = find(args.id as string);
            return note
              ? { ...summary(note), body: note.body, adopted: false }
              : null;
          }
          case "save_note": {
            const note = find(args.id as string);
            if (note) note.body = args.body as string;
            saved.push(args.body as string);
            return note ? summary(note) : null;
          }
          case "set_note_meta":
          case "set_note_type":
          case "set_source_meta":
            return notes[0] ? summary(notes[0]) : null;

          // Real rather than a stub: capture is asserted through it, and a
          // stub would let a test pass while recording nothing.
          case "set_citations": {
            const note = find(args.id as string);
            if (note) {
              note.sources = (
                args.citations as NonNullable<Note["sources"]>
              ).map((c, i) => ({ ...c, eid: c.eid || `01EVIDENCE${i}` }));
            }
            return note ? summary(note) : null;
          }

          case "extract_vault_pdf":
          case "extract_zotero_pdf":
            return (
              opts.pdf ?? {
                state: "text",
                pages: [
                  {
                    number: 1,
                    text: "Sb2Se3 ribbons grow along the [001] direction.",
                  },
                  { number: 2, text: "Carrier lifetime was 1.2 ns." },
                ],
                ownership: "external",
                cached: false,
              }
            );

          case "clear_pdf_text_cache":
            return null;

          // Takes an item key now: the backend finds the PDF attachment and
          // asks for *its* children. The harness answers per item, which is
          // what the caller has.
          case "zotero_annotations":
            zotero();
            return opts.annotations ?? [];

          case "capture_annotations": {
            const note = find(args.id as string);
            const offered = args.annotations as NonNullable<
              VaultOptions["annotations"]
            >;
            if (!note) return { added: 0, alreadyHere: 0, empty: 0 };
            const sources = note.sources ?? (note.sources = []);
            const already = new Set(
              sources.map((c) => c.annotation).filter(Boolean),
            );
            const empty = offered.filter((a) => !a.text && !a.comment).length;
            let added = 0;
            for (const a of offered) {
              if (already.has(a.key) || (!a.text && !a.comment)) continue;
              sources.push({
                eid: `01EVIDENCE${a.key}`,
                id: args.sourceId as string,
                page: a.page,
                quote: a.text,
                comment: a.comment,
                colour: a.colour,
                annotation: a.key,
              });
              added += 1;
            }
            return {
              added,
              alreadyHere: offered.length - added - empty,
              empty,
            };
          }

          case "export_docx":
            exported.push(args.document);
            // The path a real save dialog would return.
            return "C:\\Users\\test\\note.docx";
          case "ai_status":
            return { ready: false, reason: "off in tests", keyStorage: "none" };
          case "reference_status":
            return opts.zoteroDown
              ? {
                  ready: false,
                  providerId: "zotero-local",
                  provider: "Zotero",
                  reason: "not running",
                }
              : {
                  ready: true,
                  providerId: "zotero-local",
                  provider: "Zotero",
                  reason: null,
                };
          case "reference_config":
            return {
              provider: "local",
              userId: "",
              style: opts.style ?? "acs",
              locale: "en-US",
              hasKey: false,
              keyInEnvironment: false,
              keyStorage: "none",
            };
          case "typography":
            return {
              reading: "",
              interface: "",
              size: 16,
              leading: 1.6,
              width: 720,
              fonts: [],
            };
          case "list_chapters":
            return notes.filter((n) => n.type === "chapter").map(summary);
          case "read_chapter": {
            const chapter = find(args.id as string);
            // Every position comes back, including the ones whose note is gone
            // — the same contract the Rust has, so a test can see the gap.
            return (chapter?.sequence ?? []).map((id) => {
              const note = find(id);
              return note ? { id, note: summary(note) } : { id };
            });
          }
          case "save_sequence": {
            const chapter = find(args.id as string);
            if (chapter) {
              chapter.type = "chapter";
              chapter.sequence = args.sequence as string[];
            }
            return chapter ? summary(chapter) : null;
          }
          case "chapter_sections": {
            const chapter = find(args.id as string);
            if (!chapter) return [];
            const sections = [
              {
                id: chapter.id,
                title: chapter.title,
                body: chapter.body,
                heading: false,
              },
            ];
            for (const id of chapter.sequence ?? []) {
              const note = find(id);
              // Skipped, not held open: an exported document cannot have a hole.
              if (!note) continue;
              sections.push({
                id: note.id,
                title: note.title,
                body: note.body,
                heading: true,
              });
            }
            return sections;
          }
          case "chapters_using": {
            const id = args.id as string;
            return notes
              .filter((n) => n.type === "chapter" && n.sequence?.includes(id))
              .map((n) => ({
                id: n.id,
                title: n.title,
                position: (n.sequence ?? []).indexOf(id),
                of: (n.sequence ?? []).length,
              }));
          }
          case "create_chapter": {
            const created: Note = {
              id: `chapter-${notes.length + 1}`,
              type: "chapter",
              title: args.title as string,
              body: "",
              sequence: [],
            };
            notes.push(created);
            return { ...summary(created), body: "", adopted: false };
          }

          case "id_clashes":
            return opts.idClashes ?? [];
          case "migration_needed":
            return false;

          case "app_version":
            if (opts.versionFails) throw new Error("no version available");
            return opts.update && opts.update !== "fail"
              ? opts.update.current
              : "0.1.0";
          case "check_for_updates": {
            if (opts.update === "fail") {
              throw new Error("could not reach GitHub to check for updates");
            }
            if (!opts.update) {
              return {
                current: "0.1.0",
                latest: "0.1.0",
                newer: false,
                url: "",
              };
            }
            return {
              current: opts.update.current,
              latest: opts.update.latest,
              newer: opts.update.newer,
              url: `https://github.com/vermasaksham/Sutra-Windows/releases/tag/v${opts.update.latest}`,
            };
          }

          case "zotero_search": {
            zotero();
            const q = String(args.query ?? "").toLowerCase();
            return library.filter((r) =>
              (r.title + r.creators + r.year).toLowerCase().includes(q),
            );
          }
          // Picking a Zotero item brings it into the vault as a source note
          // first, so the citation points at a note rather than at an item
          // in another program. Returning an empty list here — the old
          // catch-all — produced a citation with no key at all.
          case "import_zotero_source": {
            zotero();
            if (opts.importFails) throw new Error("could not reach Zotero");
            const item = library.find((r) => r.key === args.key);
            if (!item) throw new Error("no such item");
            const existing = notes.find(
              (n) => n.source?.citationKey === item.key,
            );
            if (existing) return summary(existing);
            const source = {
              id: `01SOURCE${item.key}`.padEnd(26, "0").slice(0, 26),
              type: "source",
              title: item.title,
              folder: "Library",
              position: 0,
              tags: [] as string[],
              icon: null,
              cover: null,
              excerpt: item.title,
              updated: "2026-08-21T10:14:00Z",
              body: "",
              source: {
                authors: item.creators,
                year: item.year,
                doi: item.doi ?? undefined,
                itemType: item.itemType,
                citationKey: item.key,
              },
            };
            notes.push(source as (typeof notes)[number]);
            return summary(source as (typeof notes)[number]);
          }

          case "zotero_by_keys": {
            zotero();
            const keys = (args.keys as string[]) ?? [];
            return library.filter((r) => keys.includes(r.key));
          }

          // Everything else is a list the panels iterate over. An empty
          // array is the honest answer for a vault this small, and a `null`
          // here is what made earlier ad-hoc stubs crash the app rather than
          // the test.
          default:
            return [];
        }
      },
    };
  }, options);
}

/** The markdown the app last wrote for the open note. */
/**
 * The notes as the fake backend now holds them.
 *
 * Reading the screen cannot answer "which note did that evidence land on",
 * because only one note is shown at a time — and that question is the whole
 * point of the rule that a paper never records evidence about itself.
 */
export async function notesNow(page: Page): Promise<Note[]> {
  return page.evaluate(
    () => (window as unknown as { __notes: Note[] }).__notes ?? [],
  );
}

export async function lastSaved(page: Page): Promise<string> {
  return page.evaluate(() => {
    const saved = (window as unknown as { __saved: string[] }).__saved;
    return saved.at(-1) ?? "";
  });
}

/** The document most recently handed to the exporter. */
export async function lastExported<T = ExportedDocument>(
  page: Page,
): Promise<T> {
  return page.evaluate(() => {
    const all = (window as unknown as { __exported: unknown[] }).__exported;
    return all[all.length - 1];
  }) as Promise<T>;
}

/** The shape `buildDocument` produces, as much of it as these tests read. */
export type ExportedRun = {
  text: string;
  bold?: boolean;
  italic?: boolean;
  code?: boolean;
  image?: { data: string; width: number; height: number };
};

export type ExportedBlock =
  | { kind: "heading"; level: number; runs: ExportedRun[] }
  | { kind: "paragraph"; runs: ExportedRun[] }
  | { kind: "quote"; runs: ExportedRun[] }
  | { kind: "table"; rows: ExportedRun[][][]; headerRow: boolean }
  | { kind: string; runs?: ExportedRun[] };

export type ExportedDocument = {
  title: string;
  blocks: ExportedBlock[];
  references: ExportedRun[][];
};

/** All the text in a block's runs, joined. */
export function textOf(block: { runs?: ExportedRun[] }): string {
  return (block.runs ?? []).map((r) => r.text).join("");
}
