import { describe, expect, it } from "vitest";
import { sourceLabel } from "./sourceLabel";
import type { NoteSummary, SourceMeta } from "../vault/api";

function source(title: string, meta?: SourceMeta): NoteSummary {
  return {
    id: "S1",
    type: "source",
    title,
    folder: "Library",
    position: 0,
    tags: [],
    icon: null,
    cover: null,
    source: meta,
    excerpt: "",
    updated: "2026-08-31T00:00:00Z",
  };
}

describe("sourceLabel", () => {
  it("names one author by surname", () => {
    expect(
      sourceLabel(source("Ribbons", { authors: "Zhou, Y.", year: "2019" })),
    ).toBe("Zhou, 2019");
  });

  it("says et al. for several", () => {
    expect(
      sourceLabel(
        source("Ribbons", { authors: "Zhou, Y.; Wang, L.", year: "2019" }),
      ),
    ).toBe("Zhou et al., 2019");
  });

  it("copes with a name written without initials", () => {
    expect(sourceLabel(source("R", { authors: "Zhou", year: "2019" }))).toBe(
      "Zhou, 2019",
    );
  });

  it("uses whichever half it has", () => {
    expect(sourceLabel(source("R", { authors: "Zhou, Y." }))).toBe("Zhou");
    expect(sourceLabel(source("R", { year: "2019" }))).toBe("2019");
  });

  it("falls back to the title rather than empty brackets", () => {
    // A source captured from a scribbled reference has only a title, and that
    // still has to read as something in the middle of a sentence.
    expect(sourceLabel(source("Neumann-Kopp rule"))).toBe("Neumann-Kopp rule");
    expect(sourceLabel(source("R", { authors: "  ", year: " " }))).toBe("R");
    expect(sourceLabel(source(""))).toBe("Untitled source");
  });
});
