import { Node, mergeAttributes } from "@tiptap/core";
import type { MarkdownToken } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import CitationView from "./CitationView";

/**
 * A citation, stored as `[@KEY]` where KEY is a Zotero item key.
 *
 * The third node in this app built on the same principle: the file holds a
 * stable identifier and everything human is resolved at render time. A
 * wikilink stores a note id and shows its title; a citation stores a Zotero
 * key and shows "(Zhou et al., 2019)". Edit the reference in Zotero and every
 * citation of it updates, because nothing anywhere holds a copy.
 *
 * `[@KEY]` is close enough to Pandoc's citation syntax to be recognisable, and
 * it survives in the markdown whether or not Zotero is ever installed again.
 */
const CITATION = /^\[@([A-Z0-9]{8})\]/;

export const Citation = Node.create({
  name: "citation",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      itemKey: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-key") ?? "",
        renderHTML: (attributes) => ({
          "data-key": (attributes.itemKey as string) ?? "",
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-citation]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["span", mergeAttributes(HTMLAttributes, { "data-citation": "" })];
  },

  addNodeView() {
    return ReactNodeViewRenderer(CitationView);
  },

  markdownTokenizer: {
    name: "citation",
    level: "inline" as const,
    start: (src: string) => src.indexOf("[@"),
    tokenize: (src: string) => {
      const match = CITATION.exec(src);
      if (!match) return;
      return { type: "citation", raw: match[0], itemKey: match[1] };
    },
  },

  parseMarkdown: (token: MarkdownToken) => ({
    type: "citation",
    attrs: {
      itemKey: (token as MarkdownToken & { itemKey?: string }).itemKey ?? "",
    },
  }),

  renderMarkdown: (node: { attrs?: { itemKey?: string } }) =>
    node.attrs?.itemKey ? `[@${node.attrs.itemKey}]` : "",
});
