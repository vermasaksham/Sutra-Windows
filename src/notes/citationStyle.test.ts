import { describe, expect, it } from "vitest";
import { bibliography, emphasisRuns, styledFor } from "./citationStyle";
import type { NoteSummary, SourceMeta } from "../vault/api";

const ko: SourceMeta = {
  authors: "Ko, J.; Smith, A.",
  year: "2024",
  container: "Nature Energy",
  doi: "10.1000/abc",
  zotero: "KO2024",
  styled: {
    "american-chemical-society": {
      citation: "(1)",
      bib: "Ko, J.; Smith, A. Thermal conductivity. *Nature Energy* 2024, 14, 221–230.",
    },
    apa: { citation: "(Ko & Smith, 2024)", bib: "Ko, J., & Smith, A. (2024)." },
  },
};

function source(id: string, title: string, meta?: SourceMeta): NoteSummary {
  return {
    id,
    type: "source",
    title,
    folder: "Library",
    position: 0,
    tags: [],
    icon: null,
    cover: null,
    sources: [],
    source: meta,
    excerpt: "",
    updated: "2026-09-02T00:00:00Z",
  } as unknown as NoteSummary;
}

describe("styledFor", () => {
  it("returns the rendering for the style asked for", () => {
    expect(styledFor(ko, "apa")?.citation).toBe("(Ko & Smith, 2024)");
    expect(styledFor(ko, "american-chemical-society")?.citation).toBe("(1)");
  });

  it("is absent rather than wrong for a style never fetched", () => {
    // The caller falls back to a plain label. Returning another style's
    // rendering would silently put an APA citation in an ACS manuscript,
    // which is worse than an unstyled one because it looks finished.
    expect(styledFor(ko, "ieee")).toBeNull();
    expect(styledFor(ko, "")).toBeNull();
    expect(styledFor(undefined, "apa")).toBeNull();
    expect(styledFor({}, "apa")).toBeNull();
  });

  it("treats an entry with neither half as no answer", () => {
    const empty: SourceMeta = { styled: { ieee: {} } };
    expect(styledFor(empty, "ieee")).toBeNull();
  });
});

describe("bibliography", () => {
  const sources = [
    source("S1", "Thermal conductivity", ko),
    source("S2", "Structure of Sb2Se3", {
      authors: "Smith, B.",
      year: "2023",
      container: "J. Mater. Chem.",
    }),
  ];

  it("renders in the style when the library has done so", () => {
    const [first] = bibliography(["S1"], sources, "american-chemical-society");
    expect(first?.styled).toBe(true);
    expect(first?.text).toContain("Nature Energy");
  });

  it("falls back to a plain description, and says it is one", () => {
    // S2 was never rendered. It still belongs in the reference list — a
    // missing entry is a missing citation, which is worse than an unstyled
    // one — but the UI must be able to tell the reader it is not the style.
    const [entry] = bibliography(["S2"], sources, "american-chemical-society");
    expect(entry?.styled).toBe(false);
    expect(entry?.text).toBe(
      "Smith, B. (2023) Structure of Sb2Se3 J. Mater. Chem.",
    );
  });

  it("keeps citation order rather than sorting", () => {
    // ACS and Nature number by first appearance. Sorting alphabetically here
    // would make the list disagree with the numbers in the prose.
    const order = bibliography(["S2", "S1"], sources, "apa").map((e) => e.id);
    expect(order).toEqual(["S2", "S1"]);
  });

  it("lists one paper once however often it is cited", () => {
    // Three pages of one paper is three pieces of evidence and one reference.
    const entries = bibliography(["S1", "S1", "S1"], sources, "apa");
    expect(entries).toHaveLength(1);
  });

  it("skips a source that is not in this vault", () => {
    expect(bibliography(["GONE"], sources, "apa")).toEqual([]);
  });
});

describe("emphasisRuns", () => {
  it("turns markdown emphasis into runs the panel can render", () => {
    // The string keeps its markdown, because pasting into a note should carry
    // the italics; the panel renders it so a reference list does not display
    // its own asterisks.
    expect(emphasisRuns("Ko, J. *Nature Energy* 2024.")).toEqual([
      { text: "Ko, J. ", emphasis: false },
      { text: "Nature Energy", emphasis: true },
      { text: " 2024.", emphasis: false },
    ]);
  });

  it("leaves a lone asterisk alone", () => {
    // Footnote markers and corrected-proof asterisks are real; swallowing the
    // rest of the line after one would mangle the entry.
    expect(emphasisRuns("Smith, A.* 2019")).toEqual([
      { text: "Smith, A.* 2019", emphasis: false },
    ]);
  });

  it("handles plain text and empty strings", () => {
    expect(emphasisRuns("Plain")).toEqual([{ text: "Plain", emphasis: false }]);
    expect(emphasisRuns("")).toEqual([]);
    expect(emphasisRuns("**")).toEqual([]);
  });
});
