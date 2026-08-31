import { describe, expect, it } from "vitest";
import { buildTagTree, flattenTags, suggestTags, taggedWith } from "./tags";
import type { NoteSummary } from "../vault/api";

function note(id: string, tags: string[]): NoteSummary {
  return {
    id,
    type: "standard",
    title: id,
    folder: "",
    position: 0,
    tags,
    icon: null,
    cover: null,
    excerpt: "",
    updated: "2026-08-31T00:00:00Z",
  };
}

describe("buildTagTree", () => {
  it("nests tags on their slashes", () => {
    const roots = buildTagTree([note("a", ["research/materials/sb2se3"])]);
    expect(roots.map((r) => r.path)).toEqual(["research"]);
    expect(roots[0]!.children[0]!.path).toBe("research/materials");
    expect(roots[0]!.children[0]!.children[0]!.name).toBe("sb2se3");
  });

  it("rolls counts up to every level", () => {
    const roots = buildTagTree([
      note("a", ["research/materials/sb2se3"]),
      note("b", ["research/thermodynamics"]),
      note("c", ["cvt"]),
    ]);
    const research = roots.find((r) => r.path === "research")!;
    expect(research.total).toBe(2);
    expect(research.own).toBe(0);
    expect(research.children.map((c) => c.name)).toEqual([
      "materials",
      "thermodynamics",
    ]);
  });

  it("counts a note once per level even when two of its tags share one", () => {
    // The bug this guards: `research/a` and `research/b` on one note must not
    // make `research` claim two notes.
    const roots = buildTagTree([note("a", ["research/a", "research/b"])]);
    expect(roots[0]!.total).toBe(1);
  });

  it("separates notes carrying a parent from those carrying a child", () => {
    const roots = buildTagTree([
      note("a", ["research"]),
      note("b", ["research/materials"]),
    ]);
    expect(roots[0]!.own).toBe(1);
    expect(roots[0]!.total).toBe(2);
  });

  it("sorts siblings by name, insensitively", () => {
    const roots = buildTagTree([note("a", ["Zeta", "alpha", "Beta"])]);
    expect(roots.map((r) => r.name)).toEqual(["alpha", "Beta", "Zeta"]);
  });

  it("is empty for notes with no tags", () => {
    expect(buildTagTree([note("a", [])])).toEqual([]);
  });
});

describe("flattenTags", () => {
  it("hides the children of a collapsed tag", () => {
    const roots = buildTagTree([note("a", ["research/materials/sb2se3"])]);
    expect(flattenTags(roots, new Set()).map((n) => n.path)).toEqual([
      "research",
      "research/materials",
      "research/materials/sb2se3",
    ]);
    expect(
      flattenTags(roots, new Set(["research"])).map((n) => n.path),
    ).toEqual(["research"]);
  });
});

describe("taggedWith", () => {
  it("matches the tag and everything beneath it", () => {
    const n = note("a", ["research/materials/sb2se3"]);
    expect(taggedWith(n, "research")).toBe(true);
    expect(taggedWith(n, "research/materials")).toBe(true);
    expect(taggedWith(n, "research/materials/sb2se3")).toBe(true);
  });

  it("does not match a prefix that is not a tag boundary", () => {
    // `res` must not match `research`.
    expect(taggedWith(note("a", ["research"]), "res")).toBe(false);
  });
});

describe("suggestTags", () => {
  const all = [
    "thermal-conductivity",
    "thermodynamics",
    "research/thermodynamics",
    "cvt",
  ];

  it("ignores punctuation, so a duplicate spelling never gets created", () => {
    expect(suggestTags("thermalcond", all, [])).toEqual([
      "thermal-conductivity",
    ]);
  });

  it("puts what starts with the input before what merely contains it", () => {
    expect(suggestTags("thermo", all, [])).toEqual([
      "thermodynamics",
      "research/thermodynamics",
    ]);
  });

  it("offers the more-used spelling first when a vault has both", () => {
    // The whole point of the feature is to steer towards the tag people
    // already use. Offering the rarer spelling first would spread the
    // duplicate it exists to prevent. `all` arrives most-used first.
    const both = ["thermal-conductivity", "thermalconductivity"];
    expect(suggestTags("thermalcond", both, [])).toEqual([
      "thermal-conductivity",
      "thermalconductivity",
    ]);
    expect(suggestTags("thermalcond", [...both].reverse(), [])).toEqual([
      "thermalconductivity",
      "thermal-conductivity",
    ]);
  });

  it("does not offer a tag the note already carries", () => {
    expect(suggestTags("cvt", all, ["cvt"])).toEqual([]);
  });

  it("offers nothing for an empty draft", () => {
    expect(suggestTags("  ", all, [])).toEqual([]);
  });
});
