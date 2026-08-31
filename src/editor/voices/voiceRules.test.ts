import { describe, expect, it } from "vitest";
import { LITERATURE_TEMPLATE, voiceOf } from "./voiceRules";

describe("voiceOf", () => {
  it("recognises the three headings the template writes", () => {
    expect(voiceOf("Source says")).toBe("source");
    expect(voiceOf("My interpretation")).toBe("interpretation");
    expect(voiceOf("My question")).toBe("question");
  });

  it("allows what someone would plausibly type instead", () => {
    expect(voiceOf("source")).toBe("source");
    expect(voiceOf("The author says")).toBe("source");
    expect(voiceOf("Paper says")).toBe("source");
    expect(voiceOf("Interpretation")).toBe("interpretation");
    expect(voiceOf("My questions")).toBe("question");
  });

  it("keeps a page reference on the heading", () => {
    // "Source says · p. 6" is still the source's voice.
    expect(voiceOf("Source says · p. 6")).toBe("source");
    expect(voiceOf("  My interpretation  ")).toBe("interpretation");
  });

  it("does not claim headings that are about something else", () => {
    for (const heading of [
      "Method",
      "Results",
      "Sources",
      "Resourcing",
      "Interpreting the phase diagram",
      "",
    ]) {
      expect(voiceOf(heading)).toBeNull();
    }
  });

  it("writes a template whose headings it recognises", () => {
    // The two must not drift: a template the renderer does not recognise would
    // silently produce an unstyled note.
    const headings = LITERATURE_TEMPLATE.split("\n")
      .filter((line) => line.startsWith("## "))
      .map((line) => line.slice(3));
    expect(headings).toHaveLength(3);
    expect(headings.map(voiceOf)).toEqual([
      "source",
      "interpretation",
      "question",
    ]);
  });
});
