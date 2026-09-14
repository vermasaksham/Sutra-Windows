import { describe, expect, it } from "vitest";
import { displayTitle } from "./titleText";

describe("displayTitle", () => {
  it("lowers a subscript instead of flattening it", () => {
    expect(
      displayTitle("Sb<sub>2</sub>Se<sub>3</sub> Nanosheet Film-Based Devices"),
    ).toBe("Sb₂Se₃ Nanosheet Film-Based Devices");
  });

  it("keeps a Greek prefix beside the formula it qualifies", () => {
    expect(displayTitle("α-Sb<sub>2</sub>O<sub>3</sub> polymorph")).toBe(
      "α-Sb₂O₃ polymorph",
    );
  });

  it("raises a superscript, so a charge is not read as a digit", () => {
    expect(displayTitle("Sb<sup>3+</sup> centres")).toBe("Sb³⁺ centres");
    expect(displayTitle("10<sup>-4</sup> S cm<sup>-1</sup>")).toBe(
      "10⁻⁴ S cm⁻¹",
    );
  });

  it("drops the tags it has no meaning for", () => {
    expect(displayTitle("Growth of <i>Escherichia coli</i> on <b>Sb</b>")).toBe(
      "Growth of Escherichia coli on Sb",
    );
    // And leaves no double space where the markup used to be.
    expect(displayTitle("A <span>B</span> C")).toBe("A B C");
  });

  it("leaves a character Unicode has no lowered form for on the line", () => {
    expect(displayTitle("X<sub>Q</sub>Y")).toBe("XQY");
    // The ones that do have a form are still lowered in the same title.
    expect(displayTitle("X<sub>Q2</sub>Y")).toBe("XQ₂Y");
  });

  it("decodes entities, in both spellings", () => {
    expect(displayTitle("Cu&#x2013;O bonds &amp; strain")).toBe(
      "Cu–O bonds & strain",
    );
    expect(displayTitle("221&#8211;230")).toBe("221–230");
  });

  it("returns an ordinary title exactly as it came in", () => {
    // Including the whitespace, which the tag-stripping path collapses.
    expect(displayTitle("Thermal transport  in nanowires")).toBe(
      "Thermal transport  in nanowires",
    );
    expect(displayTitle("")).toBe("");
  });

  it("treats a lone angle bracket or ampersand as text, not markup", () => {
    expect(displayTitle("Grain size < 50 nm")).toBe("Grain size < 50 nm");
    expect(displayTitle("Sn & Se precursors")).toBe("Sn & Se precursors");
  });
});
