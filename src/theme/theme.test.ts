// `?raw` rather than node:fs, so the test needs no Node types and reads the
// file exactly as the bundler sees it.
import html from "../../index.html?raw";
import { describe, expect, it } from "vitest";
import {
  DEFAULT_PALETTE,
  PALETTES,
  readPalette,
  readPreference,
  resolveTheme,
  type PaletteId,
} from "./palettes";

describe("readPalette", () => {
  it("keeps a palette it knows", () => {
    for (const palette of PALETTES) {
      expect(readPalette(palette.id)).toBe(palette.id);
    }
  });

  it("falls back to the default for anything else", () => {
    // Nothing stored yet, a palette from a build that has since been removed,
    // a half-finished write, and somebody editing localStorage by hand. All
    // four have to land somewhere renderable: an unknown value would be
    // written to data-palette, match no rule in tokens.css, and leave the app
    // painted in the bare :root fallback with no way back except clearing
    // storage.
    expect(readPalette(null)).toBe(DEFAULT_PALETTE);
    expect(readPalette("")).toBe(DEFAULT_PALETTE);
    expect(readPalette("sepia")).toBe(DEFAULT_PALETTE);
    expect(readPalette("SUTRA")).toBe(DEFAULT_PALETTE);
  });
});

describe("readPreference", () => {
  it("keeps the three it knows", () => {
    expect(readPreference("light")).toBe("light");
    expect(readPreference("dark")).toBe("dark");
    expect(readPreference("system")).toBe("system");
  });

  it("defaults to following the system", () => {
    expect(readPreference(null)).toBe("system");
    expect(readPreference("midnight")).toBe("system");
  });
});

describe("resolveTheme", () => {
  it("takes an explicit choice over the OS", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("asks the OS only when told to", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });
});

describe("the pre-paint script", () => {
  // The inline script in index.html duplicates this module's logic, because it
  // has to run before any module can load — a palette resolved after the first
  // paint is a palette the user watches change. Duplicated logic drifts, so
  // these two tests are the thing that stops it: add a palette to PALETTES and
  // forget the script, and the app renders the new palette correctly from the
  // second paint onwards while flashing Sutra on every launch. That is the
  // kind of bug nobody reports and everybody notices.
  it("knows every palette this module offers", () => {
    for (const palette of PALETTES) {
      expect(html).toContain(`"${palette.id}"`);
    }
  });

  it("agrees with the module about the storage keys and the default", () => {
    expect(html).toContain('localStorage.getItem("sutra.palette")');
    expect(html).toContain('localStorage.getItem("sutra.theme")');
    expect(html).toContain(`dataset.palette = "${DEFAULT_PALETTE}"`);
  });
});

describe("the palette list", () => {
  it("has no duplicate ids", () => {
    const ids = PALETTES.map((palette) => palette.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("contains the default", () => {
    const ids: PaletteId[] = PALETTES.map((palette) => palette.id);
    expect(ids).toContain(DEFAULT_PALETTE);
  });

  it("describes every palette, since Settings shows the description", () => {
    for (const palette of PALETTES) {
      expect(palette.label.length).toBeGreaterThan(0);
      expect(palette.note.length).toBeGreaterThan(0);
    }
  });
});
