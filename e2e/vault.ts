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
  theme?: "light" | "dark";
  palette?: string;
  /** Which edge the editing toolbar starts on. */
  dock?: "top" | "bottom" | "left" | "right";
  /** What a check for updates should report, or "fail" to make it error. */
  update?: { current: string; latest: string; newer: boolean } | "fail";
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
      source: undefined as undefined | Record<string, unknown>,
    }));

    if (opts.theme) localStorage.setItem("sutra.theme", opts.theme);
    if (opts.palette) localStorage.setItem("sutra.palette", opts.palette);
    if (opts.dock) localStorage.setItem("sutra.toolbar", opts.dock);

    // Every body the app has written, newest last. The assertion that matters
    // most in this suite is what reaches disk, not what is on screen.
    const saved: string[] = [];
    (window as unknown as { __saved: string[] }).__saved = saved;

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
          case "set_citations":
            return notes[0] ? summary(notes[0]) : null;

          case "ai_status":
            return { ready: false, reason: "off in tests" };
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
              style: "acs",
              locale: "en-US",
              hasKey: false,
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
          case "migration_needed":
            return false;

          case "app_version":
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
export async function lastSaved(page: Page): Promise<string> {
  return page.evaluate(() => {
    const saved = (window as unknown as { __saved: string[] }).__saved;
    return saved.at(-1) ?? "";
  });
}
