import { InputRule, Node, mergeAttributes } from "@tiptap/core";
import type { MarkdownToken } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import MathBlockView from "./MathBlockView";

/**
 * Display maths: a `$$` fence on its own lines.
 *
 * ```
 * $$
 * \ce{Sb2Se3 + 3I2 <=> 2SbI3 + 3Se}
 * $$
 * ```
 *
 * A block-level tokenizer, so it is matched before the inline one ever sees the
 * dollars — otherwise `$$` would parse as an empty inline formula.
 */
const BLOCK_MATH = /^\$\$[ \t]*\r?\n([\s\S]*?)\r?\n?\$\$[ \t]*(?:\r?\n|$)/;

export const MathBlock = Node.create({
  name: "mathBlock",
  group: "block",
  atom: true,
  selectable: true,
  // No `content`: the LaTeX lives in an attribute, not as child text nodes.
  // ProseMirror would otherwise try to apply text marks inside a formula.

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
    return [{ tag: "div[data-math-block]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["div", mergeAttributes(HTMLAttributes, { "data-math-block": "" })];
  },

  addNodeView() {
    return ReactNodeViewRenderer(MathBlockView);
  },

  /**
   * `$$` at the start of a line opens an empty display formula.
   *
   * Typed rather than pasted: the block tokenizer above only runs when
   * markdown is read from disk, so without this the fence had to be typed,
   * saved and reopened before it became a formula. An empty node is right —
   * the editor puts the caret in it, which is where the LaTeX goes.
   */
  addInputRules() {
    return [
      new InputRule({
        find: /^\$\$$/,
        handler: ({ chain, range }) => {
          chain()
            .deleteRange(range)
            .insertContent({ type: this.name, attrs: { latex: "" } })
            .run();
        },
      }),
    ];
  },

  markdownTokenizer: {
    name: "mathBlock",
    level: "block" as const,
    start: (src: string) => src.indexOf("$$"),
    tokenize: (src: string) => {
      const match = BLOCK_MATH.exec(src);
      if (!match) return;
      return { type: "mathBlock", raw: match[0], latex: match[1] };
    },
  },

  parseMarkdown: (token: MarkdownToken) => ({
    type: "mathBlock",
    attrs: { latex: (token as MarkdownToken & { latex?: string }).latex ?? "" },
  }),

  /** Verbatim, unescaped — see the note on MathInline's renderMarkdown. */
  renderMarkdown: (node: { attrs?: { latex?: string } }) =>
    `$$\n${node.attrs?.latex ?? ""}\n$$`,
});
