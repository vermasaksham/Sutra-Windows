import { describe, expect, it } from "vitest";
import vaultRs from "../../../src-tauri/src/vault.rs?raw";
import { LITERATURE_TEMPLATE, voiceOf } from "./voiceRules";

/** The sections `literature_body` in vault.rs writes, in its order. */
const TEMPLATE_HEADINGS = [
  "Summary",
  "Key Evidence",
  "Important Quotes",
  "My Interpretation",
  "Research Questions",
  "Limitations",
  "Related Notes",
];

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

  it("classifies the literature-note template Rust writes", () => {
    // The template lives in Rust (vault.rs) because that is where the note is
    // created, and the renderer that colours the three voices lives here. Two
    // languages, one convention — so this reads the actual Rust source rather
    // than a copy of it. Change the headings there without changing the rules
    // here and a note the app generated is the one place the source's words
    // and the reader's stop being told apart, which is the single thing
    // section 11 calls mandatory.
    const headings = [...vaultRs.matchAll(/^\s*"([A-Z][A-Za-z ]+)",$/gm)]
      .map((m) => m[1] ?? "")
      .filter((h) => TEMPLATE_HEADINGS.includes(h));

    expect(headings).toEqual(TEMPLATE_HEADINGS);
    expect(voiceOf("Key Evidence")).toBe("source");
    expect(voiceOf("Important Quotes")).toBe("source");
    expect(voiceOf("My Interpretation")).toBe("interpretation");
    expect(voiceOf("Research Questions")).toBe("question");
    // These are the reader's own scaffolding, and claiming a voice for them
    // would assert something about their text that is not true.
    expect(voiceOf("Summary")).toBeNull();
    expect(voiceOf("Limitations")).toBeNull();
    expect(voiceOf("Related Notes")).toBeNull();
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
