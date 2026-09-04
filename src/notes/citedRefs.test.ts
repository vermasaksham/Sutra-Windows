import { describe, expect, it } from "vitest";
import { citedRefs } from "./citedRefs";

describe("citedRefs", () => {
  it("finds both shapes, once each, in order", () => {
    const body =
      "As [@ABCD1234] and [@01HQ3M8K2P00000000000000A1] show, and [@ABCD1234] again.";
    expect(citedRefs(body)).toEqual(["ABCD1234", "01HQ3M8K2P00000000000000A1"]);
  });

  it("ignores things that only look like citations", () => {
    for (const body of [
      "an email like a@b.com",
      "[@lowercase]",
      "[@SHORT]",
      "[@NINECHARS9]",
      "[@ABCD1234 unclosed",
    ]) {
      expect(citedRefs(body), body).toEqual([]);
    }
  });

  it("is empty for a note that cites nothing", () => {
    expect(citedRefs("Just prose.")).toEqual([]);
  });
});
