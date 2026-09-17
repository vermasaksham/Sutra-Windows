import { Node } from "@tiptap/core";
import type { JSONContent, MarkdownToken } from "@tiptap/core";
import { createInterpretationBlock, readInterpretationBlock } from "./format";
import type { InterpretationBlock, InterpretationMeta } from "./format";

type InterpretationToken = MarkdownToken & { block: InterpretationBlock };

/**
 * Experimental: deliberately absent from extensions.ts until the compatibility
 * gates in v0.5-interpretation-format.md pass. No command mints an identity.
 */
export const Interpretation = Node.create({
  name: "interpretation",
  group: "block",
  content: "block+",
  defining: true,
  isolating: true,

  addAttributes() {
    return {
      meta: { default: null, rendered: false },
      original: { default: null, rendered: false },
      originalContent: { default: null, rendered: false },
    };
  },

  // HTML paste must not create a second copy of an interpretation identity.
  // Clipboard identity semantics need a deliberate policy before registration.
  parseHTML() {
    return [];
  },

  renderHTML() {
    return ["section", { "data-interpretation": "" }, 0];
  },

  markdownTokenizer: {
    name: "interpretation",
    level: "block",
    start: (source) =>
      source.search(/^(?:~{3,}|`{3,})sutra-interpretation-v1/m),
    tokenize: (source, _tokens, lexer) => {
      const block = readInterpretationBlock(source);
      if (!block) return;
      return {
        type: "interpretation",
        raw: block.raw,
        block,
        tokens: lexer.blockTokens(block.body),
      };
    },
  },

  parseMarkdown: (token, helpers) => {
    const { block } = token as InterpretationToken;
    const parse = helpers.parseBlockChildren ?? helpers.parseChildren;
    const children = parse(token.tokens ?? []);
    const content: JSONContent[] = children.length
      ? children
      : [{ type: "paragraph" }];
    return helpers.createNode(
      "interpretation",
      {
        meta: block.meta,
        original: block.raw,
        originalContent: JSON.stringify(content),
      },
      content,
    );
  },

  renderMarkdown: (node, helpers) => {
    const meta = node.attrs?.meta as InterpretationMeta;
    const original = node.attrs?.original as string | null;
    const parsed = original ? readInterpretationBlock(original) : undefined;
    // An unrelated edit must not normalize this block's author's prose. The
    // snapshot lives only in editor memory; Markdown remains the durable copy.
    if (
      parsed &&
      JSON.stringify(parsed.meta) === JSON.stringify(meta) &&
      node.attrs?.originalContent === JSON.stringify(node.content)
    ) {
      return original!;
    }
    return createInterpretationBlock(
      meta,
      helpers.renderChildren(node.content ?? [], "\n\n"),
    );
  },
});
