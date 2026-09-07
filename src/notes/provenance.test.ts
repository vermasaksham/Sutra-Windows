import { describe, expect, it } from "vitest";
import { divergence, isConsistent, summarise } from "./provenance";

const cite = (id: string) => ({ id });

describe("divergence", () => {
  it("says nothing when the two records agree", () => {
    const found = divergence([cite("A"), cite("B")], ["A", "B"]);
    expect(found).toEqual({ uncited: [], unrecorded: [] });
    expect(isConsistent(found)).toBe(true);
  });

  it("does not care about the order they appear in", () => {
    expect(isConsistent(divergence([cite("B"), cite("A")], ["A", "B"]))).toBe(
      true,
    );
  });

  it("finds a source recorded but never cited", () => {
    // The case that used to be invisible: cut the sentence, keep the
    // bibliography entry, and nothing said so.
    const found = divergence([cite("A"), cite("B")], ["A"]);
    expect(found.uncited).toEqual(["B"]);
    expect(found.unrecorded).toEqual([]);
    expect(isConsistent(found)).toBe(false);
  });

  it("finds a citation with no provenance record", () => {
    const found = divergence([cite("A")], ["A", "B"]);
    expect(found.unrecorded).toEqual(["B"]);
    expect(found.uncited).toEqual([]);
  });

  it("finds both directions at once", () => {
    const found = divergence([cite("A"), cite("C")], ["A", "B"]);
    expect(found.uncited).toEqual(["C"]);
    expect(found.unrecorded).toEqual(["B"]);
  });

  it("treats two records for one source as one source", () => {
    // Citing the same paper at two pages is ordinary, and must not read as a
    // problem — nor be reported twice.
    const found = divergence([cite("A"), cite("A")], ["A"]);
    expect(isConsistent(found)).toBe(true);
  });

  it("reports a repeated inline citation once", () => {
    const found = divergence([], ["B", "B", "B"]);
    expect(found.unrecorded).toEqual(["B"]);
  });

  it("says nothing about an empty note", () => {
    expect(isConsistent(divergence([], []))).toBe(true);
  });
});

describe("summarise", () => {
  it("counts in the singular", () => {
    expect(summarise(divergence([cite("A")], []))).toBe(
      "1 recorded source is not cited in this note.",
    );
    expect(summarise(divergence([], ["A"]))).toBe(
      "1 citation in the text has no provenance record.",
    );
  });

  it("counts in the plural", () => {
    expect(summarise(divergence([cite("A"), cite("B")], []))).toBe(
      "2 recorded sources are not cited in this note.",
    );
  });

  it("joins both directions into one sentence", () => {
    expect(summarise(divergence([cite("A")], ["B"]))).toBe(
      "1 recorded source is not cited in this note. 1 citation in the text has no provenance record.",
    );
  });
});
