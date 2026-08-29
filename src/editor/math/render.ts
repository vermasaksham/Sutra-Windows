import katex from "katex";
// Loading this module registers \ce, \pu and friends on the KaTeX instance
// above. It is a side-effect import with no exports, and it must come after
// katex itself or there is nothing to register onto.
//
// mhchem ships inside KaTeX, so chemistry costs no additional dependency.
import "katex/contrib/mhchem";

/**
 * Render LaTeX to HTML.
 *
 * `throwOnError: false` is deliberate. This runs on every keystroke while
 * someone is typing a formula, and a half-typed `\frac{` is the normal state of
 * affairs, not an exception. KaTeX renders the offending source in red instead,
 * which shows the author where they are rather than blanking the block.
 */
export function renderMath(latex: string, displayMode: boolean): string {
  return katex.renderToString(latex, {
    displayMode,
    throwOnError: false,
    // Warnings for unsupported commands would otherwise fill the console on
    // every render of a formula KaTeX cannot fully handle.
    strict: false,
    trust: false,
  });
}
