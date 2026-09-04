/**
 * Two questions about a formula's source, kept apart from the views so they
 * can be answered without a DOM.
 *
 * Both exist because of one bug. Inserting a chemical equation pre-fills the
 * source with `\ce{}` — the wrapper nobody remembers, so that what you type is
 * the reaction itself. But the block opened in *rendered* mode, since the test
 * for "new, start editing" was `latex === ""` and `\ce{}` is not empty. KaTeX
 * renders an empty `\ce{}` as very nearly nothing, so choosing Chemical
 * equation produced a blank strip with no caret in it, and the only way
 * forward was to guess that it could be clicked.
 */

/** Is there anything here for KaTeX to draw? */
export function isBlank(latex: string): boolean {
  // An empty wrapper is scaffolding, not content: `\ce{}` and `\text{}` alike
  // render to nothing, so a formula made only of them is a formula with
  // nothing in it.
  return latex.replace(/\\[a-zA-Z]+\{\s*\}/g, "").trim() === "";
}

/**
 * Where the caret belongs when the source opens for editing.
 *
 * Inside the first empty pair of braces, which is where the author's next
 * keystroke is meant to go, and otherwise at the end — the normal place to
 * resume typing something you are coming back to.
 */
export function caretAt(latex: string): number {
  const empty = latex.indexOf("{}");
  return empty === -1 ? latex.length : empty + 1;
}
