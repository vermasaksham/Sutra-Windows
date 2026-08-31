import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { voiceOf } from "./voiceRules";

/**
 * Renders the three voices of a literature note differently.
 *
 * Decorations, not node types. A custom node would mean custom markdown, and
 * then the separation between what a paper claims and what you concluded would
 * only exist inside this program — which is the opposite of the point. This
 * adds a class to the heading and to everything under it until the next
 * heading, changes nothing about the document, and writes nothing to the file.
 *
 * A consequence worth stating: it is a reading aid, not a guarantee. Nothing
 * stops an interpretation being typed under "Source says". The app can make the
 * two look different enough that doing it feels wrong, and that is the whole of
 * what a portable format allows.
 */
export const Voices = Extension.create({
  name: "voices",

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey("sutraVoices"),
        props: {
          decorations: ({ doc }) => decorate(doc),
        },
      }),
    ];
  },
});

function decorate(doc: ProseMirrorNode): DecorationSet {
  const decorations: Decoration[] = [];
  let current: string | null = null;

  doc.forEach((node, offset) => {
    if (node.type.name === "heading") {
      // Any heading ends the previous voice, so a section that is not one of
      // the three is not silently swept into the one above it.
      current = voiceOf(node.textContent);
      if (current) {
        decorations.push(
          Decoration.node(offset, offset + node.nodeSize, {
            class: `sutra-voice sutra-voice-${current} sutra-voice-heading`,
          }),
        );
      }
      return;
    }
    if (current) {
      decorations.push(
        Decoration.node(offset, offset + node.nodeSize, {
          class: `sutra-voice sutra-voice-${current}`,
        }),
      );
    }
  });

  return DecorationSet.create(doc, decorations);
}
