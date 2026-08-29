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
              commit();
            }
            // Escape finishes and keeps the edit, same as the block view.
            if (e.key === "Escape") {
              e.preventDefault();
              commit();
            }
          }}
          className="sutra-math-input"
          size={Math.max(draft.length, 6)}
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
