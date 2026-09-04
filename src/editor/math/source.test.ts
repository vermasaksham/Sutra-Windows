import { describe, expect, it } from "vitest";
import { caretAt, isBlank } from "./source";

describe("isBlank", () => {
  it("treats an empty wrapper as nothing written yet", () => {
    // The bug this exists for: Chemical equation inserts `\ce{}`, which is not
    // the empty string, so the block opened rendered — a blank strip with no
    // caret, and no hint that clicking it was the way in.
    expect(isBlank("\\ce{}")).toBe(true);
    expect(isBlank("")).toBe(true);
    expect(isBlank("   ")).toBe(true);
    expect(isBlank("\\ce{ }")).toBe(true);
  });

  it("leaves a formula with anything in it alone", () => {
    expect(isBlank("\\ce{Sb2Se3 + 3I2 <=> 2SbI3 + 3Se}")).toBe(false);
    expect(isBlank("E = mc^2")).toBe(false);
    // A wrapper that is empty but not alone is still a formula: the author
    // wrote the rest of it.
    expect(isBlank("\\ce{} + heat")).toBe(false);
  });
});

describe("caretAt", () => {
  it("puts the caret where the author is about to type", () => {
    // Between the braces of `\ce{}` — the reaction is the part they came to
    // write, and the wrapper is the part that was filled in for them.
    expect(caretAt("\\ce{}")).toBe(4);
    expect("\\ce{}".slice(0, caretAt("\\ce{}"))).toBe("\\ce{");
  });

  it("resumes at the end of a formula that already says something", () => {
    const latex = "\\ce{Sb2Se3}";
    expect(caretAt(latex)).toBe(latex.length);
  });
});
