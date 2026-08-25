import { Node, mergeAttributes } from "@tiptap/core";
import type { MarkdownToken } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import WikiLinkView from "./WikiLinkView";

/** `[[` followed by a 26-character ULID and `]]`, anchored at the cursor. */
const WIKILINK = /^\[\[([0-9A-Z]{26})\]\]/;

/**
 * A link to another note.
 *
 * Stored as `[[id]]` and displayed as the target's current title, which is why
 * renaming a note can never break a link — the title is resolved at render
 * time and the file only ever holds the id.
 *
 * An atom: the editor treats it as one indivisible thing, so backspace removes
 * the whole link rather than leaving a half-eaten ULID behind.
 *
 * The markdown spec lives here, next to the schema. That is the pattern the
 * whole architecture was chosen for, and it is the same shape the maths and
 * chemistry nodes will take in Phase 5: one place defines the syntax, so parse
 * and render cannot drift apart.
 */
export const WikiLink = Node.create({
  name: "wikiLink",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      targetId: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-target-id"),
        renderHTML: (attributes) =>
          attributes.targetId
            ? { "data-target-id": attributes.targetId as string }
            : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-wikilink]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["span", mergeAttributes(HTMLAttributes, { "data-wikilink": "" })];
  },

  addNodeView() {
    return ReactNodeViewRenderer(WikiLinkView);
  },

  // These three are top-level config fields, not nested under a `markdown`
  // key. The doc comment on createInlineMarkdownSpec suggests `markdown: spec`,
  // but MarkdownManager.registerExtension reads them with
  // getExtensionField(extension, "parseMarkdown") — i.e. straight off the
  // config. Nesting them means the tokenizer is silently never registered and
  // `[[id]]` ends up escaped as literal text.
  markdownTokenizer: {
    name: "wikiLink",
    level: "inline" as const,
    // Tells the lexer where a match could begin so it does not run the regex
    // against every character.
    start: (src: string) => src.indexOf("[["),
    tokenize: (src: string) => {
      const match = WIKILINK.exec(src);
      if (!match) return;
      return { type: "wikiLink", raw: match[0], targetId: match[1] };
    },
  },

  // The tokenizer above attaches `targetId`, which is ours and so not part of
  // the base MarkdownToken shape.
  parseMarkdown: (token: MarkdownToken) => ({
    type: "wikiLink",
    attrs: {
      targetId: (token as MarkdownToken & { targetId?: string }).targetId ?? null,
    },
  }),

  renderMarkdown: (node: { attrs?: { targetId?: string | null } }) =>
    node.attrs?.targetId ? `[[${node.attrs.targetId}]]` : "",
});
