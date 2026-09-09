import { describe, expect, it } from "vitest";
import { buildDocument } from "./buildDocument";

/**
 * The export path, entered where a chapter would enter it: markdown, in order,
 * with no editor anywhere.
 *
 * These tests deliberately stay away from formulas and images, which need a
 * browser to rasterise. What they check is the part that had been impossible to
 * check before — that a document can be assembled from note bodies at all, and
 * that several notes come out as one continuous document.
 */
describe("building a document from note bodies", () => {
  it("turns one note's markdown into blocks", async () => {
    const doc = await buildDocument(
      [
        {
          title: "Growth log",
          body: "## Conditions\n\nThe source sat at **560 °C**.\n",
        },
      ],
      [],
    );

    expect(doc.title).toBe("Growth log");
    expect(doc.blocks).toEqual([
      { kind: "heading", level: 2, runs: [{ text: "Conditions" }] },
      {
        kind: "paragraph",
        runs: [
          { text: "The source sat at " },
          { text: "560 °C", bold: true },
          { text: "." },
        ],
      },
    ]);
  });

  it("does not repeat the title of a single note", async () => {
    const doc = await buildDocument([{ title: "Only", body: "Prose.\n" }], []);
    expect(doc.blocks.some((b) => b.kind === "heading")).toBe(false);
  });

  it("reads several notes into one continuous document", async () => {
    const doc = await buildDocument(
      [
        { title: "Chapter 3", body: "The opening argument.\n" },
        { title: "Growth", body: "How the films were made.\n", heading: true },
        { title: "Optics", body: "What they absorbed.\n", heading: true },
      ],
      [],
    );

    // The first title is the document's; the rest become headings in place.
    expect(doc.title).toBe("Chapter 3");
    expect(doc.blocks.map((b) => b.kind)).toEqual([
      "paragraph",
      "heading",
      "paragraph",
      "heading",
      "paragraph",
    ]);
    expect(doc.blocks[1]).toEqual({
      kind: "heading",
      level: 1,
      runs: [{ text: "Growth" }],
    });
  });

  it("keeps the order it was given", async () => {
    const doc = await buildDocument(
      [
        { title: "First", body: "alpha\n" },
        { title: "Second", body: "beta\n", heading: true },
      ],
      [],
    );
    const text = doc.blocks
      .flatMap((b) => ("runs" in b ? b.runs.map((r) => r.text) : []))
      .join(" ");
    expect(text.indexOf("alpha")).toBeLessThan(text.indexOf("beta"));
  });

  it("carries lists and tables through", async () => {
    const doc = await buildDocument(
      [
        {
          title: "Runs",
          body:
            "- first\n- second\n\n" +
            "| Run | Gap |\n| --- | --- |\n| 14 | 1.2 |\n",
        },
      ],
      [],
    );

    const kinds = doc.blocks.map((b) => b.kind);
    expect(kinds).toContain("listItem");
    expect(kinds).toContain("table");
  });

  it("returns an empty document for an empty list rather than throwing", async () => {
    const doc = await buildDocument([], []);
    expect(doc).toEqual({ title: "", blocks: [], references: [] });
  });
});
