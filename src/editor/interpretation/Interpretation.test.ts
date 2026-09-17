import { describe, expect, it } from "vitest";
import { getSchema } from "@tiptap/core";
import { extensions } from "../extensions";
import { Interpretation } from "./Interpretation";
import {
  InterpretationLiteral,
  InterpretationMarkdown,
} from "./InterpretationMarkdown";
import { createInterpretationBlock, readInterpretationBlock } from "./format";

const meta = {
  iid: "01HQ3M8K2P00000000000000A1",
  evidence: ["01HQ3M8K2P00000000000000E7"],
  future: { confidence: "unassigned" },
};
const experimental = [...extensions, Interpretation, InterpretationLiteral];
const manager = new InterpretationMarkdown({ extensions: experimental });
const schema = getSchema(experimental);

describe("experimental interpretation container", () => {
  it("is not enabled in the app or the shared export parser", () => {
    expect(
      extensions.some((extension) => extension.name === "interpretation"),
    ).toBe(false);
  });

  it("parses editable prose and keeps identity through a ProseMirror schema round trip", () => {
    const raw = createInterpretationBlock(
      meta,
      "## My interpretation\n\nOnly **two** samples.\n",
    );
    const doc = schema.nodeFromJSON(manager.parse(raw));
    doc.check();
    const block = doc.firstChild!;
    expect(block.type.name).toBe("interpretation");
    expect(block.child(0).type.name).toBe("heading");
    expect(block.child(1).textContent).toBe("Only two samples.");
    expect(block.attrs.meta).toEqual(meta);
    expect(
      manager.parse(manager.serialize(doc.toJSON())).content?.[0]?.attrs?.meta,
    ).toEqual(meta);
  });

  it("keeps an unedited block byte-identical when a different paragraph changes", () => {
    const raw = `~~~~sutra-interpretation-v1  ${JSON.stringify(meta)}\n## My interpretation\n\nUnusual  spacing.\n\n~~~~~ \t\n`;
    const json = manager.parse(raw + "\nOutside.\n");
    json.content![1]!.content![0]!.text = "Outside, edited.";
    expect(manager.serialize(json)).toContain(raw);
  });

  it("retains identity, evidence references and unknown metadata after a prose edit", () => {
    const json = manager.parse(
      createInterpretationBlock(meta, "Old conclusion."),
    );
    json.content![0]!.content![0]!.content![0]!.text = "Revised conclusion.";
    const saved = manager.serialize(json);
    const reopened = manager.parse(saved);
    expect(reopened.content?.[0]?.attrs?.meta).toEqual(meta);
    expect(readInterpretationBlock(saved)?.body).toBe("Revised conclusion.\n");
    expect(manager.serialize(reopened)).toBe(saved);
  });

  it("keeps references when the block is moved within a document", () => {
    const json = manager.parse(
      createInterpretationBlock(meta, "Conclusion.") + "\nOther paragraph.",
    );
    json.content!.reverse();
    const reopened = manager.parse(manager.serialize(json));
    expect(reopened.content?.[1]?.attrs?.meta).toEqual(meta);
  });

  it("keeps an empty interpretation valid in the schema", () => {
    const doc = schema.nodeFromJSON(
      manager.parse(createInterpretationBlock({ ...meta, evidence: [] }, "")),
    );
    doc.check();
    expect(doc.firstChild?.attrs.meta.evidence).toEqual([]);
  });

  it("does not turn ordinary headings or outer fenced examples into interpretations", () => {
    expect(
      manager.parse("## My interpretation\n\nProse.").content?.[0]?.type,
    ).toBe("heading");
    const example =
      "````markdown\n" + createInterpretationBlock(meta, "Example.") + "````\n";
    expect(manager.parse(example).content?.[0]?.type).toBe("codeBlock");
  });

  it("does not interpret unsupported versions or malformed metadata", () => {
    for (const raw of [
      createInterpretationBlock(meta, "Prose.").replace("v1", "v2"),
      "~~~sutra-interpretation-v1 {broken}\nProse.\n~~~\n",
    ]) {
      expect(manager.parse(raw).content?.[0]?.type).toBe(
        "interpretationLiteral",
      );
    }
  });

  it("preserves original bytes through the ProseMirror schema", () => {
    const raw = createInterpretationBlock(
      meta,
      "## My interpretation\n\nOdd  spacing.\n",
    );
    const json = schema.nodeFromJSON(manager.parse(raw)).toJSON();
    expect(manager.serialize(json)).toBe(raw);
  });

  it("keeps CRLF through the Markdown lexer", () => {
    const raw = createInterpretationBlock(meta, "Conclusion.").replaceAll(
      "\n",
      "\r\n",
    );
    expect(manager.serialize(manager.parse(raw))).toBe(raw);
  });

  it("quoted examples stay inert rather than becoming live identities", () => {
    const quoted = createInterpretationBlock(meta, "Example.")
      .trimEnd()
      .split("\n")
      .map((line) => `> ${line}`)
      .join("\n");
    expect(manager.parse(quoted).content?.[0]?.content?.[0]?.type).toBe(
      "codeBlock",
    );
  });

  it("unsupported syntax keeps its original fence on save", () => {
    const raw =
      "~~~~sutra-interpretation-v2 {future}\n```js\nexample()\n```\n~~~~\n";
    expect(manager.serialize(manager.parse(raw))).toBe(raw);
  });

  it("maps repeated blocks to their own raw bytes across mixed line endings", () => {
    const first = createInterpretationBlock(meta, "First.").replaceAll(
      "\n",
      "\r\n",
    );
    const second = createInterpretationBlock(
      { ...meta, iid: "01HQ3M8K2P00000000000000A2" },
      "Second.",
    );
    const raw =
      "```js\nordinary()\n```\n\n" + first + "\nBetween.\n\n" + second;
    const saved = manager.serialize(
      schema.nodeFromJSON(manager.parse(raw)).toJSON(),
    );
    expect(saved).toContain(first);
    expect(saved).toContain(second);
  });

  it("keeps list examples and nested interpretation examples inert", () => {
    const inner = createInterpretationBlock(meta, "Example.");
    const list =
      "- Example:\n\n" +
      inner
        .trimEnd()
        .split("\n")
        .map((line) => `  ${line}`)
        .join("\n");
    expect(JSON.stringify(manager.parse(list))).not.toContain(
      '"type":"interpretation"',
    );
    const outer = createInterpretationBlock(meta, inner);
    expect(manager.parse(outer).content?.[0]?.content?.[0]?.type).toBe(
      "codeBlock",
    );
  });

  it("keeps malformed and unclosed root blocks byte-identical through the schema", () => {
    for (const raw of [
      "~~~sutra-interpretation-v1 {broken}\r\nExact.\r\n~~~\r\n",
      "~~~sutra-interpretation-v2 {}\r\nUnclosed.",
    ]) {
      const json = schema.nodeFromJSON(manager.parse(raw)).toJSON();
      expect(manager.serialize(json)).toBe(raw);
    }
  });

  it("preserves untouched rich prose after schema defaults are applied", () => {
    const body =
      "## Reading\n\n[Link](https://example.org) and $E_g$.\n\n- [ ] Verify\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
    const raw = createInterpretationBlock(meta, body);
    expect(
      manager.serialize(schema.nodeFromJSON(manager.parse(raw)).toJSON()),
    ).toBe(raw);
  });
});
