import { describe, expect, it } from "vitest";
import { citationOrder, isNumeric, marker } from "./citationStyle";

describe("citationOrder", () => {
  it("numbers by first appearance in the prose", () => {
    expect(citationOrder(["b", "a", "b", "c"], [])).toEqual(["b", "a", "c"]);
  });

  it("keeps a source the prose no longer names", () => {
    // Its page and quote are still evidence; dropping it from the list would
    // discard provenance because a marker was deleted.
    expect(citationOrder(["a"], ["a", "z"])).toEqual(["a", "z"]);
  });

  it("never lists one paper twice", () => {
    // Citing a paper on three pages is three pieces of evidence, one number.
    expect(citationOrder(["a", "a", "a"], ["a"])).toEqual(["a"]);
  });
});

describe("isNumeric", () => {
  it("recognises what a numbering style renders", () => {
    for (const rendered of ["1", "(1)", "[1]", " (12) "]) {
      expect(isNumeric(rendered), rendered).toBe(true);
    }
  });

  it("does not mistake an author-date citation for a number", () => {
    for (const rendered of ["(Zhou et al., 2019)", "Zhou 2019", "", null]) {
      expect(isNumeric(rendered), String(rendered)).toBe(false);
    }
  });

  it("reads a bare year as a year, not as reference two thousand", () => {
    // A source with a year and no author renders as "(2019)". Treating that as
    // a citation number would renumber it to "(1)" and lose the only thing the
    // marker said.
    expect(isNumeric("(2019)")).toBe(false);
    expect(marker("(2019)", "Untitled source", 1)).toBe("(2019)");
    // Real citation numbers stay numbers, including implausibly high ones.
    expect(isNumeric("(999)")).toBe(true);
  });
});

describe("marker", () => {
  it("numbers by position, keeping the library's brackets", () => {
    expect(marker("(1)", "Zhou et al., 2019", 3)).toBe("(3)");
    expect(marker("[1]", "Zhou et al., 2019", 3)).toBe("[3]");
  });

  it("brackets a bare number, which would otherwise read as prose", () => {
    expect(marker("1", "Zhou et al., 2019", 2)).toBe("[2]");
  });

  it("leaves an author-date citation exactly as rendered", () => {
    // Not re-wrapped: doing so produced "((Zhou et al., 2019))".
    expect(marker("(Zhou et al., 2019)", "Zhou et al., 2019", 1)).toBe(
      "(Zhou et al., 2019)",
    );
  });

  it("falls back to the vault's own label when nothing is rendered", () => {
    expect(marker(null, "Zhou et al., 2019", 1)).toBe("(Zhou et al., 2019)");
    expect(marker("", "Binnewies, 2012", 2)).toBe("(Binnewies, 2012)");
  });

  it("prefers the label to a number it cannot trust", () => {
    // Zotero renders each item alone, so its "1" means nothing until the note's
    // order is known. Showing it would number every citation [1].
    expect(marker("(1)", "Zhou et al., 2019", null)).toBe(
      "(Zhou et al., 2019)",
    );
  });
});
