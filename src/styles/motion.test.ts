import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/** Read a stylesheet beside this test as text. */
const read = (file: string) =>
  readFileSync(new URL(file, import.meta.url).pathname, "utf8");

const index = read("./index.css");
const tokens = read("./tokens.css");

/**
 * Motion has to have exactly one dial.
 *
 * It briefly had two. The tokens below governed the ten transitions written in
 * these stylesheets, while a hardcoded `duration-150 ease-out` on a hundred
 * components governed everything else — so retuning the tokens moved almost
 * nothing, and, worse, only those ten answered to prefers-reduced-motion. The
 * components kept animating for someone who had asked the OS for less
 * movement.
 *
 * These read the CSS as text because that coupling is between two files and
 * has no runtime surface to assert on: Tailwind's defaults are declared in one
 * and the tokens they point at in the other. Read from disk rather than
 * imported, because Vitest hands a `.css` import no usable export — and an
 * empty string would let every assertion below pass by asserting over nothing.
 */

/** The names of every duration token declared in `css`. */
function durationNames(css: string): string[] {
  const names = [...css.matchAll(/(--duration-[a-z-]+):/g)].map((m) => m[1]);
  return [...new Set(names.filter((name) => name !== undefined))];
}

const REDUCED = "@media (prefers-reduced-motion: reduce)";
const split = tokens.indexOf(REDUCED);

// Split rather than scan the whole file, so a token that appears *only* in the
// override cannot satisfy its own check.
const declared = split === -1 ? "" : tokens.slice(0, split);
const reducedMotion = split === -1 ? "" : tokens.slice(split);

describe("motion", () => {
  it("declares durations and a curve", () => {
    // Guards the guards: if these were renamed, every assertion below would
    // pass over an empty set and prove nothing.
    expect(split).not.toBe(-1);
    expect(durationNames(declared).length).toBeGreaterThan(0);
    expect(declared).toMatch(/--ease-out:\s*cubic-bezier/);
  });

  it("hands Tailwind's transition utilities the tokens, not literals", () => {
    // A literal here is how the two dials happened. It reads as harmless —
    // the same 150ms either way — but it takes every `transition-*` utility
    // in the app back out of the reduced-motion override's reach.
    expect(index).toMatch(
      /--default-transition-duration:\s*var\(--duration-[a-z-]+\)/,
    );
    expect(index).toMatch(
      /--default-transition-timing-function:\s*var\(--ease-[a-z-]+\)/,
    );
  });

  it("stops every duration under prefers-reduced-motion", () => {
    // Every one, not just the ones that existed when the override was
    // written: a new token that nothing zeroes keeps moving for someone who
    // asked the OS for less movement.
    for (const name of durationNames(declared)) {
      expect(reducedMotion).toContain(`${name}: 0ms`);
    }
  });
});
