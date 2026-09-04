import { useEffect, useMemo, useRef, useState } from "react";
import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { renderMath } from "./render";

/**
 * Inline maths: rendered by default, raw LaTeX when you click it.
 *
 * An empty formula still shows something clickable — a freshly inserted node
 * would otherwise be a zero-width gap the user cannot find again.
 */
export default function MathInlineView({
  node,
  updateAttributes,
  selected,
  editor,
  getPos,
}: NodeViewProps) {
  const latex = (node.attrs.latex as string) ?? "";
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(latex);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => setDraft(latex), [latex]);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  // Re-rendering KaTeX on every keystroke of the surrounding document would be
  // wasteful; the output only depends on the formula.
  const html = useMemo(() => renderMath(latex, false), [latex]);

  function commit() {
    setEditing(false);
    if (draft !== latex) updateAttributes({ latex: draft });
  }

  /**
   * Finish, and put the caret back in the sentence, just after the formula.
   *
   * Same reason as the block view: closing the input unmounts the focused
   * element, focus falls to the body, and the rest of the sentence typed
   * after a formula was going nowhere. Here it matters more, because the
   * words after an inline formula are usually the point of the sentence.
   *
   * Not on blur — that means the author clicked elsewhere on purpose.
   */
  function finish() {
    commit();
    const pos = typeof getPos === "function" ? getPos() : null;
    editor.commands.focus(pos == null ? null : pos + node.nodeSize);
  }

  if (editing) {
    return (
      <NodeViewWrapper as="span" className="sutra-math-inline-wrapper">
        <input
          ref={inputRef}
          value={draft}
          aria-label="Inline formula"
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            // Stop the editor's own handlers seeing these keys; inside this
            // input they mean "finish editing", not "insert a paragraph".
            e.stopPropagation();
            if (e.key === "Enter") {
              e.preventDefault();
              finish();
            }
            // Escape finishes and keeps the edit, same as the block view.
            if (e.key === "Escape") {
              e.preventDefault();
              finish();
            }
          }}
          className="sutra-math-input"
          // A floor, not a width: `field-sizing: content` sizes this to the
          // formula, and this is only what browsers without it fall back to.
          // Small, so an empty formula is a caret to type into rather than a
          // gap in the sentence.
          size={Math.max(draft.length, 2)}
        />
      </NodeViewWrapper>
    );
  }

  return (
    <NodeViewWrapper as="span" className="sutra-math-inline-wrapper">
      <span
        role="button"
        tabIndex={0}
        contentEditable={false}
        title={latex || "Empty formula"}
        onMouseDown={(e) => {
          // ProseMirror claims mousedown on an atom node view to make a node
          // selection, and the click event never reaches React. Taking it here
          // and stopping propagation is what makes the formula clickable.
          e.preventDefault();
          e.stopPropagation();
          setEditing(true);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            setEditing(true);
          }
        }}
        className={[
          "sutra-math-inline",
          selected ? "is-selected" : "",
          latex ? "" : "is-empty",
        ].join(" ")}
        // KaTeX generates this markup from the LaTeX itself, with trust:false
        // so \href, \url and \includegraphics are rejected rather than emitted.
        // Nothing from the note reaches the DOM except through KaTeX.
        dangerouslySetInnerHTML={{ __html: latex ? html : "" }}
      />
    </NodeViewWrapper>
  );
}
