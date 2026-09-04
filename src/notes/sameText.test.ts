import { describe, expect, it } from "vitest";
import { sameText } from "./useNote";

describe("recognising our own save coming back", () => {
  it("forgives the trailing newline the file gains", () => {
    // The bug this exists for. `written` holds the buffer the editor produced;
    // the watcher event makes us re-read, and the parsed file carries a
    // trailing newline the buffer never had. Comparing exactly meant every
    // autosave looked like an outside edit — remounting the editor mid-word
    // on a clean buffer, and raising the conflict prompt on a dirty one.
    expect(
      sameText("Cp fitted over 300-800 K.\n", "Cp fitted over 300-800 K."),
    ).toBe(true);
    expect(
      sameText("Two paragraphs.\n\nSecond.\n\n", "Two paragraphs.\n\nSecond."),
    ).toBe(true);
    expect(sameText("", "\n")).toBe(true);
  });

  it("still calls a real edit a real edit", () => {
    expect(
      sameText("Cp fitted over 300-800 K.\n", "Cp fitted over 300-900 K."),
    ).toBe(false);
    expect(
      sameText("Ramped under argon.\n", "Ramped under argon. Twice."),
    ).toBe(false);
    // A trailing space someone typed mid-sentence is not trailing.
    expect(sameText("a b\n", "a  b")).toBe(false);
    // Leading whitespace is content — an indented code block starts with it.
    expect(sameText("  indented\n", "indented")).toBe(false);
  });

  it("is symmetric, since either side may be the one with the newline", () => {
    for (const [a, b] of [
      ["x\n", "x"],
      ["x", "x\n"],
      ["x", "y"],
    ] as const) {
      expect(sameText(a, b)).toBe(sameText(b, a));
    }
  });
});
