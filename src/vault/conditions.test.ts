import { describe, expect, it } from "vitest";
import {
  CONDITION_KINDS,
  condition,
  conditionKind,
  conditionValue,
  type Condition,
} from "./api";

describe("view conditions", () => {
  it("round-trips every kind the editor offers", () => {
    // Three lists have to agree: the `Condition` union, `CONDITION_KINDS`, and
    // the keys Rust writes. A kind the editor offers but the union has no
    // member for would build a condition Rust ignores — silently widening the
    // view rather than failing.
    for (const { kind } of CONDITION_KINDS) {
      const built = condition(kind, "value");
      expect(conditionKind(built)).toBe(kind);
      expect(conditionValue(built)).toBe("value");
      expect(Object.keys(built)).toEqual([kind]);
    }
  });

  it("offers exactly the kinds Rust can compile", () => {
    // Pinned by hand against src-tauri/src/views.rs. Adding one there and not
    // here just means the editor cannot build it; adding one here and not
    // there produces a view whose results are wider than it says.
    expect(CONDITION_KINDS.map((c) => c.kind).sort()).toEqual([
      "cites",
      "in",
      "links-to",
      "tag",
      "text",
      "type",
      "under",
      "updated-after",
      "updated-before",
    ]);
  });

  it("reads a condition that came back from Rust", () => {
    const fromDisk: Condition = { under: "Research/Sb2Se3" };
    expect(conditionKind(fromDisk)).toBe("under");
    expect(conditionValue(fromDisk)).toBe("Research/Sb2Se3");
  });

  it("treats an empty value as a value, not as an absent one", () => {
    // `{ in: "" }` is the top level of the vault — a real condition, and the
    // one a blank folder box means. Reading it as missing would turn a view
    // of the root into a view of everything.
    const root = condition("in", "");
    expect(conditionValue(root)).toBe("");
    expect(Object.keys(root)).toEqual(["in"]);
  });
});
