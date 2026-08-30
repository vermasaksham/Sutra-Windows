import { Extension } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion from "@tiptap/suggestion";
import { PluginKey } from "@tiptap/pm/state";
import type { SuggestionProps } from "@tiptap/suggestion";
import { computePosition, flip, offset, shift } from "@floating-ui/dom";
import type { ComponentProps } from "react";
import WikiLinkMenu, {
  type WikiLinkItem,
  type WikiLinkMenuHandle,
} from "./WikiLinkMenu";
import { searchTitles } from "./titleStore";

type MenuProps = ComponentProps<typeof WikiLinkMenu>;

/**
 * Typing `[[` offers the vault's notes by title and inserts the chosen one as
 * a `[[id]]` link.
 *
 * The user searches by title because that is what they remember; the document
 * stores the id because that is what survives a rename.
 */
export const WikiLinkSuggestion = Extension.create({
  name: "wikiLinkSuggestion",

  addProseMirrorPlugins() {
    return [
      Suggestion<WikiLinkItem>({
        editor: this.editor,
        // Distinct from the slash menu's key; see the note there.
        pluginKey: new PluginKey("sutraWikiLink"),
        char: "[[",
        // Without this the `[` typed a moment earlier counts as a prefix and
        // the menu never opens.
        allowedPrefixes: null,
        items: ({ query }) => searchTitles(query, 20),

        render: () => {
          let renderer: ReactRenderer<WikiLinkMenuHandle, MenuProps> | null =
            null;
          let container: HTMLDivElement | null = null;
          let latest: SuggestionProps<WikiLinkItem> | null = null;

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

          const select = (item: WikiLinkItem) => {
            if (!latest) return;
            // `range` covers the `[[` and everything typed after it, so the
            // trigger text is replaced by the node rather than left behind.
            latest.editor
              .chain()
              .focus()
              .deleteRange(latest.range)
              .insertContent([
                { type: "wikiLink", attrs: { targetId: item.id } },
                // A trailing space, so the caret lands after the link rather
                // than glued to an atom the cursor cannot enter.
                { type: "text", text: " " },
              ])
              .run();
          };

          const menuProps = (
            props: SuggestionProps<WikiLinkItem>,
          ): MenuProps => ({ items: props.items, onSelect: select });

          return {
            onStart: (props) => {
              latest = props;
              renderer = new ReactRenderer<WikiLinkMenuHandle, MenuProps>(
                WikiLinkMenu,
                { props: menuProps(props), editor: props.editor },
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
            },

            onUpdate: (props) => {
              latest = props;
              renderer?.updateProps(menuProps(props));
              place();
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
              window.removeEventListener("scroll", place, true);
              window.removeEventListener("resize", place);
              renderer?.destroy();
              container?.remove();
              renderer = null;
              container = null;
              latest = null;
            },
          };
        },
      }),
    ];
  },
});
