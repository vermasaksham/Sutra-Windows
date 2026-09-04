import { InputRule, Node, mergeAttributes } from "@tiptap/core";
import type { MarkdownToken } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import MathInlineView from "./MathInlineView";

/**
 * Inline maths: `$E = mc^2$`.
 *
 * The guards in this pattern are what stop prose being eaten. Currency is the
 * case that matters — in "costs $5 and $7" a naive `\$(.+?)\$` matches
 * "5 and ", turning a sentence into a formula. Requiring a non-space directly
 * after the opening `$` and directly before the closing one rejects it, because
 * the character before the second `$` is a space.
 *
 * Newlines are excluded too: inline maths that spans a line break is almost
 * always two unrelated dollar signs.
 */
const INLINE_MATH = /^\$(?![\s$])((?:[^$\n\\]|\\.)+?)(?<![\s\\])\$/;

/**
 * The same pattern, anchored at the caret, for converting as you type.
 *
 * Without this a formula only became a formula when the file was read back
 * from disk, so `$\ce{Sb2Se3}$` sat there as literal dollars for the whole
 * writing session — which is to say inline maths did not work at all for the
 * person typing it, only for the person reopening the note.
 *
 * `\$\$` is excluded at the start so typing the second dollar of a display
 * fence does not produce an empty inline formula.
 */
const INLINE_MATH_TYPED = /\$(?![\s$])((?:[^$\n\\]|\\.)+?)(?<![\s\\])\$$/;

export const MathInline = Node.create({
  name: "mathInline",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      latex: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-latex") ?? "",
        renderHTML: (attributes) => ({
          "data-latex": (attributes.latex as string) ?? "",
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-math-inline]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { "data-math-inline": "" }),
    ];
  },

  addNodeView() {
    return ReactNodeViewRenderer(MathInlineView);
  },

  addInputRules() {
    return [
      // Written out rather than using `nodeInputRule`, which leaves the
      // opening `$` behind when the pattern has a capture group — the formula
      // rendered but the prose kept a stray dollar in front of it.
      new InputRule({
        find: INLINE_MATH_TYPED,
        handler: ({ chain, range, match }) => {
          const latex = match[1] ?? "";
          if (latex.trim() === "") return;
          chain()
            .deleteRange(range)
            .insertContent({ type: this.name, attrs: { latex } })
            .run();
        },
      }),
    ];
  },

  // Top-level config keys, not nested under `markdown` — see the note in
  // wikilink/WikiLink.ts for why nesting them silently does nothing.
  markdownTokenizer: {
    name: "mathInline",
    level: "inline" as const,
    start: (src: string) => src.indexOf("$"),
    tokenize: (src: string) => {
      const match = INLINE_MATH.exec(src);
      if (!match) return;
      return { type: "mathInline", raw: match[0], latex: match[1] };
    },
  },

  parseMarkdown: (token: MarkdownToken) => ({
    type: "mathInline",
    attrs: { latex: (token as MarkdownToken & { latex?: string }).latex ?? "" },
  }),

  /**
   * The LaTeX goes back out verbatim, with no escaping.
   *
   * This is the whole reason markdown conversion lives in the editor. A
   * formula is full of characters the prose serialiser escapes — `_`, `*`, `\`,
   * `{` — and `\ce{Sb2Se3 + 3I2 <=> 2SbI3 + 3Se}` would come back as
   * `\\ce{Sb2Se3 + 3I2 &lt;=&gt; 2SbI3 + 3Se}` if it were treated as prose.
   * Because this node owns its own serialisation, the text never reaches the
   * escaper at all.
   */
  renderMarkdown: (node: { attrs?: { latex?: string } }) =>
    `$${node.attrs?.latex ?? ""}$`,
});
