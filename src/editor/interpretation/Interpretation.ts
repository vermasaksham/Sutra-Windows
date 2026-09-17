import { Node } from "@tiptap/core";
import { createInterpretationBlock, readInterpretationBlock } from "./format";
import type { InterpretationMeta } from "./format";

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

  // Recognize the format's closing whitespace, but emit ordinary code tokens.
  // Only the document-level adapter may promote them to live identities.
  markdownTokenizer: {
    name: "interpretationFence",
    level: "block",
    start: (source) =>
      source.search(/^(?:~{3,}|`{3,})sutra-interpretation-v1/m),
    tokenize: (source) => {
      const block = readInterpretationBlock(source);
      if (!block) return;
      return {
        type: "code",
        raw: block.raw,
        lang: block.opening.replace(/^(?:~+|`+)/, "").trim(),
        text: block.body.replace(/\r?\n$/, ""),
      };
    },
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
