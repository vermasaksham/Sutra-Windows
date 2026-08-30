import { Extension } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion from "@tiptap/suggestion";
import { PluginKey } from "@tiptap/pm/state";
import type { SuggestionProps } from "@tiptap/suggestion";
import { computePosition, flip, offset, shift } from "@floating-ui/dom";
import type { ComponentProps } from "react";
import SlashMenu, { type SlashMenuHandle } from "./SlashMenu";
import { filterItems, type SlashItem } from "./items";

type MenuProps = ComponentProps<typeof SlashMenu>;

/**
 * The `/` menu.
 *
 * TipTap's Suggestion plugin does the text-matching half: it watches for the
 * trigger character, tracks the query typed after it, and reports the document
 * range to replace. Everything visual is ours — a React component in a
 * fixed-position element on `document.body`, placed with Floating UI so it
 * flips above the cursor near the bottom of the window instead of being clipped.
 *
 * Suggestion's default `allowedPrefixes` is `[' ']`, so the menu opens on a
 * slash at the start of a block or after a space, and stays out of the way when
 * someone types "and/or".
 */
export const SlashCommand = Extension.create({
  name: "slashCommand",

  addProseMirrorPlugins() {
    return [
      Suggestion<SlashItem>({
        editor: this.editor,
        // Every Suggestion instance defaults to the same ProseMirror plugin
        // key, and ProseMirror rejects two plugins sharing one. With the
        // wikilink autocomplete also using Suggestion, both need their own.
        pluginKey: new PluginKey("sutraSlashCommand"),
        char: "/",
        items: ({ query }) => filterItems(query),

        render: () => {
          let renderer: ReactRenderer<SlashMenuHandle, MenuProps> | null = null;
          let container: HTMLDivElement | null = null;
          let latest: SuggestionProps<SlashItem> | null = null;
          // Escape hides the menu but leaves the typed text alone; it stays
          // hidden until the suggestion itself ends.
          let dismissed = false;

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

          const hide = () => {
            dismissed = true;
            if (container) container.style.display = "none";
          };

          const select = (item: SlashItem) => {
            if (!latest) return;
            // `range` covers the slash and everything typed after it, so the
            // item's own command deletes the trigger text as it inserts.
            // Some items are async (the image picker); nothing here waits on
            // them, and the menu closes either way.
            void item.run(latest.editor, latest.range);
          };

          const menuProps = (props: SuggestionProps<SlashItem>): MenuProps => ({
            items: props.items,
            onSelect: select,
            onDismiss: hide,
          });

          return {
            onStart: (props) => {
              latest = props;
              dismissed = false;

              renderer = new ReactRenderer<SlashMenuHandle, MenuProps>(
                SlashMenu,
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
              if (!dismissed) place();
            },

            onKeyDown: (props) => {
              if (dismissed) return false;
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
              dismissed = false;
            },
          };
        },
      }),
    ];
  },
});
