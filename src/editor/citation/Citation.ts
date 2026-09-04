import { Node, mergeAttributes } from "@tiptap/core";
import type { MarkdownToken } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import CitationView from "./CitationView";

/**
 * A citation, stored as `[@ref]`.
 *
 * The third node in this app built on the same principle: the file holds a
 * stable identifier and everything human is resolved at render time. A
 * wikilink stores a note id and shows its title; a citation stores a source's
 * id and shows "(Zhou et al., 2019)". Edit the source and every citation of it
 * updates, because nothing anywhere holds a copy.
 *
 * Two kinds of ref, told apart by length. Twenty-six characters is a source
 * note in this vault. Eight is a Zotero item key, which is how citations were
 * written before a source became a note — those only resolve while Zotero is
 * running, which is why there is a migration, and why both are recognised
 * until it has been run.
 *
 * `[@ref]` is close enough to Pandoc's citation syntax to be recognisable, and
 * it survives in the markdown whichever programs are installed.
 */
const CITATION = /^\[@([0-9A-Z]{8}|[0-9A-Z]{26})\]/;

export const Citation = Node.create({
  name: "citation",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      ref: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-key") ?? "",
        renderHTML: (attributes) => ({
          "data-ref": (attributes.ref as string) ?? "",
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
      return { type: "citation", raw: match[0], ref: match[1] };
    },
  },

  parseMarkdown: (token: MarkdownToken) => ({
    type: "citation",
    attrs: {
      ref: (token as MarkdownToken & { ref?: string }).ref ?? "",
    },
  }),

  renderMarkdown: (node: { attrs?: { ref?: string } }) =>
    node.attrs?.ref ? `[@${node.attrs.ref}]` : "",
});
