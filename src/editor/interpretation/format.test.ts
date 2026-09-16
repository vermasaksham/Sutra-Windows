import { describe, expect, it } from "vitest";
import {
  createInterpretationBlock,
  readInterpretationBlock,
  writeInterpretationBlock,
} from "./format";

const iid = "01HQ3M8K2P00000000000000A1";
const eid = "01HQ3M8K2P00000000000000E7";
const meta = { iid, evidence: [eid] };

describe("interpretation body format", () => {
  it("keeps identity and evidence beside the researcher's prose", () => {
    const raw = createInterpretationBlock(
      meta,
      "Only two samples support this.",
    );
    const block = readInterpretationBlock(raw)!;
    expect(block.meta).toEqual(meta);
    expect(block.body).toBe("Only two samples support this.\n");
    expect(writeInterpretationBlock(block)).toBe(raw);
  });

  it("preserves CRLF, header spacing, unknown metadata and closing whitespace", () => {
    const raw =
      `~~~~sutra-interpretation-v1  { "iid": "${iid}", "evidence": ["${eid}"], "future": {"confidence":"unassigned"} }\r\n` +
      "## My reading\r\n\r\n  Exact prose.  \r\n\r\n~~~~~ \t\r\n";
    const block = readInterpretationBlock(raw)!;
    expect(block.meta.future).toEqual({ confidence: "unassigned" });
    expect(writeInterpretationBlock(block)).toBe(raw);
  });

  it("does not consume the next paragraph or interpretation", () => {
    const first = createInterpretationBlock(meta, "First reading.");
    const second = createInterpretationBlock(
      { iid: "01HQ3M8K2P00000000000000A2", evidence: [] },
      "A separate reading.",
    );
    const input = first + "\nAfter.\n\n" + second;
    const parsed = readInterpretationBlock(input)!;
    expect(parsed.raw).toBe(first);
    expect(input.slice(parsed.raw.length)).toBe("\nAfter.\n\n" + second);
  });

  it("retains unknown and repeated evidence IDs without choosing a winner", () => {
    const raw = createInterpretationBlock({ iid, evidence: [eid, eid] }, "");
    expect(readInterpretationBlock(raw)?.meta.evidence).toEqual([eid, eid]);
  });

  it("allows an explicit interpretation with no supporting evidence yet", () => {
    const raw = createInterpretationBlock(
      { iid, evidence: [] },
      "A hypothesis.",
    );
    expect(readInterpretationBlock(raw)?.meta.evidence).toEqual([]);
  });

  it("chooses framing that cannot be closed by code examples in the body", () => {
    const body =
      "## Reading\n\n```rust\nlet x = 1;\n```\n\n~~~~\nexample\n~~~~\n";
    const raw = createInterpretationBlock(meta, body);
    expect(raw.startsWith("~~~~~sutra-interpretation-v1 ")).toBe(true);
    expect(readInterpretationBlock(raw)?.body).toBe(body);
  });

  it("preserves chemistry, tables, citations and links without reparsing them", () => {
    const body =
      `The $\\ce{Sb2Se3}$ result in [@${eid}] and [[${iid}|Draft]].\n\n` +
      "| x | y |\n|---|---|\n| 1 | 2 |\n";
    const raw = createInterpretationBlock(meta, body);
    expect(readInterpretationBlock(raw)?.body).toBe(body);
    expect(writeInterpretationBlock(readInterpretationBlock(raw)!)).toBe(raw);
  });

  it("accepts backtick framing and a closing fence at EOF", () => {
    const raw =
      "```sutra-interpretation-v1 " + JSON.stringify(meta) + "\nText\n```";
    expect(writeInterpretationBlock(readInterpretationBlock(raw)!)).toBe(raw);
  });

  it.each([
    [
      "future version",
      "~~~sutra-interpretation-v2 " + JSON.stringify(meta) + "\nText\n~~~\n",
    ],
    ["missing id", '~~~sutra-interpretation-v1 {"evidence":[]}\nText\n~~~\n'],
    ["invalid JSON", "~~~sutra-interpretation-v1 {broken}\nText\n~~~\n"],
    [
      "invalid ULID",
      '~~~sutra-interpretation-v1 {"iid":"made-up","evidence":[]}\nText\n~~~\n',
    ],
    [
      "missing evidence list",
      `~~~sutra-interpretation-v1 {"iid":"${iid}"}\nText\n~~~\n`,
    ],
    [
      "unclosed fence",
      "~~~sutra-interpretation-v1 " + JSON.stringify(meta) + "\nText\n",
    ],
    [
      "mismatched closing fence",
      "~~~sutra-interpretation-v1 " + JSON.stringify(meta) + "\nText\n```\n",
    ],
    [
      "short closing fence",
      "~~~~sutra-interpretation-v1 " + JSON.stringify(meta) + "\nText\n~~~\n",
    ],
    ["ordinary heading", "## My interpretation\n\nProse without an id.\n"],
    [
      "quoted example",
      "> ~~~sutra-interpretation-v1 " +
        JSON.stringify(meta) +
        "\n> Text\n> ~~~\n",
    ],
  ])("leaves %s for the ordinary Markdown parser", (_name, source) => {
    expect(readInterpretationBlock(source)).toBeUndefined();
  });

  it("never creates a block from invalid identity or evidence", () => {
    expect(() =>
      createInterpretationBlock({ iid: "", evidence: [] }, ""),
    ).toThrow();
    expect(() =>
      createInterpretationBlock({ iid, evidence: ["paper title"] }, ""),
    ).toThrow();
  });
});
