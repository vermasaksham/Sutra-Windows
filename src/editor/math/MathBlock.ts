import { Node, mergeAttributes } from "@tiptap/core";
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
