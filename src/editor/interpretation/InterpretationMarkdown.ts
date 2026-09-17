import { getSchema, Node } from "@tiptap/core";
import type { JSONContent } from "@tiptap/core";
import { MarkdownManager } from "@tiptap/markdown";
import { readInterpretationBlock } from "./format";

/** Unsupported syntax is opaque, not a code block the serializer may rewrite. */
export const InterpretationLiteral = Node.create({
  name: "interpretationLiteral",
  group: "block",
  atom: true,
  addAttributes: () => ({ raw: { default: "", rendered: false } }),
  parseHTML: () => [],
  renderHTML: ({ node }) => ["pre", {}, ["code", {}, node.attrs.raw]],
  renderMarkdown: (node) => node.attrs?.raw ?? "",
});

/**
 * Interpret only root-level code tokens, after normal Markdown context is
 * resolved. A recursive tokenizer cannot distinguish prose from quoted samples.
 * Map normalized token boundaries back to the original source before parsing
 * metadata: Marked normalizes CRLF, but those bytes belong to the researcher.
 * Deliberately not wired into the application until browser integration passes.
 */
export class InterpretationMarkdown extends MarkdownManager {
  private readonly interpretationSchema: ReturnType<typeof getSchema>;

  constructor(
    options: NonNullable<ConstructorParameters<typeof MarkdownManager>[0]>,
  ) {
    super(options);
    this.interpretationSchema = getSchema(options.extensions);
  }

  override parse(source: string): JSONContent {
    const normalized = source.replace(/\r\n?/g, "\n");
    const offsets = [0];
    for (let index = 0; index < source.length; index++) {
      if (source[index] === "\r" && source[index + 1] === "\n") index++;
      offsets.push(index + 1);
    }
    const lexer = new this.instance.Lexer(this.instance.defaults);
    const tokens = lexer.lex(source);
    const code: string[] = [];
    let position = 0;
    for (const token of tokens) {
      // Fail closed if a dependency changes token accounting. Guessing a raw
      // span here could silently assign one interpretation another's identity.
      if (
        normalized.slice(position, position + token.raw.length) !== token.raw
      ) {
        throw new Error("Markdown token boundaries do not match the source");
      }
      const end = position + token.raw.length;
      if (token.type === "code") {
        // Marked can leave the closing line ending in a following space token.
        // It still belongs to the preserved fence, including when it is CRLF.
        const framedEnd =
          !token.raw.endsWith("\n") && normalized[end] === "\n" ? end + 1 : end;
        code.push(source.slice(offsets[position], offsets[framedEnd]));
      }
      position = end;
    }
    const doc = super.parse(source);
    if (
      (doc.content ?? []).filter((node) => node.type === "codeBlock").length !==
      code.length
    ) {
      throw new Error("Markdown code blocks do not match their source tokens");
    }
    let codeIndex = 0;
    doc.content = doc.content?.map((node) => {
      if (node.type !== "codeBlock") return node;
      const raw = code[codeIndex++];
      if (!raw || !/^(?:~{3,}|`{3,})sutra-interpretation-/.test(raw))
        return node;
      const block = readInterpretationBlock(raw);
      if (!block) return { type: "interpretationLiteral", attrs: { raw } };
      // No recursive interpretation: nested blocks and quoted examples remain
      // ordinary Markdown. Identity only exists at the document root.
      const children = super.parse(block.body).content ?? [];
      // Snapshot the schema-normalized children. Otherwise defaults added by
      // ProseMirror (e.g. table cell attrs) look like edits and rewrite prose.
      const content = this.interpretationSchema
        .nodeFromJSON({
          type: "doc",
          content: children.length ? children : [{ type: "paragraph" }],
        })
        .toJSON().content!;
      return {
        type: "interpretation",
        attrs: {
          meta: block.meta,
          original: raw,
          originalContent: JSON.stringify(content),
        },
        content,
      };
    });
    return doc;
  }
}
