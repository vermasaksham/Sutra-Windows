import { Node, mergeAttributes } from "@tiptap/core";
import type { MarkdownToken } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import WikiLinkView from "./WikiLinkView";
import { titleOf } from "./titleStore";

/** `[[` followed by a 26-character ULID and `]]`, anchored at the cursor. */
/**
 * `[[ULID]]`, and `[[ULID|Any Title]]`.
 *
 * The second shape is read but never written, in v0.2.1. Writing it is a
 * vault-wide rewrite of every note, which needs a preview and a way back, so
 * it waits for v0.3 — but a vault edited by a newer build, or by hand in
 * Obsidian (where `[[x|y]]` is the alias syntax), must not read as broken
 * text here in the meantime. Reading first, writing later, is what makes that
 * migration safe to do at all.
 *
 * The id is what resolves. Everything after the pipe is display text with no
 * authority: a title that has gone stale shows the note's real current name,
 * because the title is looked up from the id exactly as it always was.
 */
const WIKILINK = /^\[\[([0-9A-Z]{26})(?:\|[^\]]*)?\]\]/;

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
      targetId:
        (token as MarkdownToken & { targetId?: string }).targetId ?? null,
    },
  }),

  // Written as `[[id|Title]]` from v0.3, and read either way since v0.2.1.
  //
  // The id stays first and stays the only thing that resolves; the title is
  // display text, refreshed from the current note every time the file is
  // written, so renaming a note updates the words in every link to it without
  // any link changing what it points at. Obsidian reads the same shape as its
  // alias syntax, which is what makes a Sutra vault legible in another editor
  // rather than a page of 26-character strings.
  //
  // A target that cannot be resolved is written back as a bare `[[id]]`. No
  // title is invented for a note that is not there — the id is what the file
  // actually knows, and a plausible-looking name for a missing note is worse
  // than an honest identifier.
  renderMarkdown: (node: { attrs?: { targetId?: string | null } }) => {
    const targetId = node.attrs?.targetId;
    if (!targetId) return "";
    const title = titleOf(targetId)?.trim();
    // A title containing `]` or `|` would break the shape it is written into,
    // so such a note keeps the plain form rather than producing markdown that
    // reads back as something else.
    const safe = title && !/[[\]|]/.test(title) ? title : null;
    return safe ? `[[${targetId}|${safe}]]` : `[[${targetId}]]`;
  },
});
