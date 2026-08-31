import { Extension } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion from "@tiptap/suggestion";
import type { SuggestionProps } from "@tiptap/suggestion";
import { PluginKey } from "@tiptap/pm/state";
import { computePosition, flip, offset, shift } from "@floating-ui/dom";
import type { ComponentProps } from "react";
import CitationMenu, {
  type Candidate,
  type CitationMenuHandle,
} from "./CitationMenu";
import { remember, vaultCandidates } from "./citationStore";
import { sourcesApi, zoteroApi, type Reference } from "../../vault/api";

type MenuProps = ComponentProps<typeof CitationMenu>;

/**
 * Typing `@` cites a source, and always ends up citing a note in the vault.
 *
 * Two places to look. Sources already here are matched from memory, so they
 * appear on the first keystroke and work with nothing else installed — and
 * they come first, because a paper is read once and cited a dozen times.
 * Zotero is asked as well, debounced, and a response that arrives after a
 * newer request started is dropped rather than allowed to overwrite it.
 *
 * Picking a Zotero item copies it into the vault before the citation is
 * inserted. Nothing ever writes a Zotero key into a note again.
 */
export const CitationSuggestion = Extension.create({
  name: "citationSuggestion",

  addProseMirrorPlugins() {
    return [
      Suggestion({
        editor: this.editor,
        pluginKey: new PluginKey("sutraCitation"),
        char: "@",
        // Items are fetched asynchronously below, so the plugin itself never
        // has any.
        items: () => [],

        render: () => {
          let renderer: ReactRenderer<CitationMenuHandle, MenuProps> | null =
            null;
          let container: HTMLDivElement | null = null;
          let latest: SuggestionProps<never> | null = null;
          let timer: ReturnType<typeof setTimeout> | null = null;
          /** Increments per search; a stale response carries an old number. */
          let generation = 0;
          let results: Candidate[] = [];
          let state: "idle" | "loading" | "error" = "idle";
          let error: string | null = null;

          const place = () => {
            const rect = latest?.clientRect?.();
            if (!container || !rect) return;
            void computePosition(
              { getBoundingClientRect: () => rect },
              container,
              {
                placement: "bottom-start",
                strategy: "fixed",
                middleware: [
                  offset(6),
                  flip({ padding: 8 }),
                  shift({ padding: 8 }),
                ],
              },
            ).then(({ x, y }) => {
              if (!container) return;
              container.style.left = `${x}px`;
              container.style.top = `${y}px`;
            });
          };

          const insert = (id: string) => {
            if (!latest) return;
            latest.editor
              .chain()
              .focus()
              .deleteRange(latest.range)
              .insertContent([
                { type: "citation", attrs: { ref: id } },
                { type: "text", text: " " },
              ])
              .run();
          };

          const select = (candidate: Candidate) => {
            if (candidate.kind === "source") return insert(candidate.id);

            // A Zotero item has to become a note first. Remember the reference
            // meanwhile so nothing flashes as unresolved, and take the range
            // now because the menu closes before the import returns.
            remember(candidate.reference);
            const range = latest?.range;
            const editor = latest?.editor;
            void sourcesApi
              .importZotero(candidate.reference.key)
              .then((source) => {
                if (!editor || !range) return;
                editor
                  .chain()
                  .focus()
                  .deleteRange(range)
                  .insertContent([
                    { type: "citation", attrs: { ref: source.id } },
                    { type: "text", text: " " },
                  ])
                  .run();
              })
              .catch(() => {
                // The import failed — Zotero went away mid-pick, or the vault
                // refused. Leaving the typed text alone is better than
                // inserting a citation of nothing.
              });
          };

          const props = (): MenuProps => ({
            items: results,
            state,
            error,
            onSelect: select,
          });

          const rerender = () => renderer?.updateProps(props());

          const runSearch = (query: string) => {
            if (timer) clearTimeout(timer);
            // Sources in the vault are in memory, so they can be shown at once
            // rather than waiting on a debounce for another program.
            const local = vaultCandidates(query);
            if (query.trim().length < 2) {
              // One character matches most of a library; wait for a second
              // before asking Zotero, but show what is already here.
              results = local;
              state = "idle";
              rerender();
              return;
            }
            results = local;
            state = "loading";
            rerender();

            const mine = ++generation;
            timer = setTimeout(() => {
              zoteroApi
                .search(query)
                .then((found: Reference[]) => {
                  if (mine !== generation) return; // A newer search won.
                  // Anything already in the vault is offered from there, not
                  // twice: importing a second copy is the thing to avoid.
                  const here = new Set(
                    vaultCandidates("")
                      .map((c) => (c.kind === "source" ? c.zotero : null))
                      .filter(Boolean),
                  );
                  results = [
                    ...vaultCandidates(query),
                    ...found
                      .filter((reference) => !here.has(reference.key))
                      .map<Candidate>((reference) => ({
                        kind: "zotero",
                        reference,
                        title: reference.title,
                        detail: [
                          reference.creators,
                          reference.year,
                          reference.container,
                        ]
                          .filter(Boolean)
                          .join(" · "),
                      })),
                  ];
                  state = "idle";
                  error = null;
                  rerender();
                })
                .catch((cause: unknown) => {
                  if (mine !== generation) return;
                  // Zotero being off is not a failure of the whole menu: the
                  // vault's own sources are still perfectly citable.
                  results = vaultCandidates(query);
                  state = results.length > 0 ? "idle" : "error";
                  error =
                    cause instanceof Error ? cause.message : String(cause);
                  rerender();
                });
            }, 200);
          };

          return {
            onStart: (start) => {
              latest = start as SuggestionProps<never>;
              renderer = new ReactRenderer<CitationMenuHandle, MenuProps>(
                CitationMenu,
                { props: props(), editor: start.editor },
              );
              container = document.createElement("div");
              container.style.position = "fixed";
              container.style.top = "0";
              container.style.left = "0";
              container.style.zIndex = "50";
              if (renderer.element) container.appendChild(renderer.element);
              document.body.appendChild(container);
              place();
              window.addEventListener("scroll", place, true);
              window.addEventListener("resize", place);
              runSearch(start.query);
            },

            onUpdate: (update) => {
              latest = update as SuggestionProps<never>;
              place();
              runSearch(update.query);
            },

            onKeyDown: (props) => {
              if (props.event.key === "Escape") {
                container?.remove();
                container = null;
                return true;
              }
              return renderer?.ref?.onKeyDown(props.event) ?? false;
            },

            onExit: () => {
              if (timer) clearTimeout(timer);
              // Bump the generation so a request already in flight is ignored
              // when it lands.
              generation += 1;
              window.removeEventListener("scroll", place, true);
              window.removeEventListener("resize", place);
              renderer?.destroy();
              container?.remove();
              renderer = null;
              container = null;
              latest = null;
              results = [];
              state = "idle";
              error = null;
            },
          };
        },
      }),
    ];
  },
});
