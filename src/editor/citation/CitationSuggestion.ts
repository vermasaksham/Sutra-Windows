import { Extension } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion from "@tiptap/suggestion";
import type { SuggestionProps } from "@tiptap/suggestion";
import { PluginKey } from "@tiptap/pm/state";
import { computePosition, flip, offset, shift } from "@floating-ui/dom";
import type { ComponentProps } from "react";
import CitationMenu, { type CitationMenuHandle } from "./CitationMenu";
import { remember } from "./citationStore";
import { zoteroApi, type Reference } from "../../vault/api";

type MenuProps = ComponentProps<typeof CitationMenu>;

/**
 * Typing `@` searches Zotero and inserts the chosen reference as `[@KEY]`.
 *
 * Unlike the slash menu and the `[[` picker, the candidate list is not in
 * memory — every keystroke is a request to another program. So results are
 * debounced, and a request that returns after a newer one started is dropped
 * rather than allowed to overwrite it.
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
          let renderer: ReactRenderer<CitationMenuHandle, MenuProps> | null = null;
          let container: HTMLDivElement | null = null;
          let latest: SuggestionProps<never> | null = null;
          let timer: ReturnType<typeof setTimeout> | null = null;
          /** Increments per search; a stale response carries an old number. */
          let generation = 0;
          let results: Reference[] = [];
          let state: "idle" | "loading" | "error" = "idle";
          let error: string | null = null;

          const place = () => {
            const rect = latest?.clientRect?.();
            if (!container || !rect) return;
            void computePosition({ getBoundingClientRect: () => rect }, container, {
              placement: "bottom-start",
              strategy: "fixed",
              middleware: [offset(6), flip({ padding: 8 }), shift({ padding: 8 })],
            }).then(({ x, y }) => {
              if (!container) return;
              container.style.left = `${x}px`;
              container.style.top = `${y}px`;
            });
          };

          const select = (reference: Reference) => {
            if (!latest) return;
            // Cache it so the node renders its label immediately rather than
            // asking Zotero again for something we just received.
            remember(reference);
            latest.editor
              .chain()
              .focus()
              .deleteRange(latest.range)
              .insertContent([
                { type: "citation", attrs: { itemKey: reference.key } },
                { type: "text", text: " " },
              ])
              .run();
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
            if (query.trim().length < 2) {
              // One character matches most of a library; wait for a second.
              results = [];
              state = "idle";
              rerender();
              return;
            }
            state = "loading";
            rerender();

            const mine = ++generation;
            timer = setTimeout(() => {
              zoteroApi
                .search(query)
                .then((found) => {
                  if (mine !== generation) return; // A newer search won.
                  results = found;
                  state = "idle";
                  error = null;
                  rerender();
                })
                .catch((cause: unknown) => {
                  if (mine !== generation) return;
                  results = [];
                  state = "error";
                  error = cause instanceof Error ? cause.message : String(cause);
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
