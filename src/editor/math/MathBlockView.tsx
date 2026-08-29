import { useEffect, useMemo, useRef, useState } from "react";
import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { renderMath } from "./render";

/**
 * Display maths: rendered and centred, editable as raw LaTeX on click.
 *
 * A newly inserted block opens straight into editing, since an empty rendered
 * formula is nothing to look at and the user's next act is always to type.
 */
export default function MathBlockView({
  node,
  updateAttributes,
  selected,
}: NodeViewProps) {
  const latex = (node.attrs.latex as string) ?? "";
  const [editing, setEditing] = useState(latex === "");
  const [draft, setDraft] = useState(latex);
  const areaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => setDraft(latex), [latex]);

  useEffect(() => {
    if (!editing) return;
    const area = areaRef.current;
    if (!area) return;
    area.focus();
    area.setSelectionRange(area.value.length, area.value.length);
  }, [editing]);

  const html = useMemo(() => renderMath(latex, true), [latex]);

  function commit() {
    setEditing(false);
    if (draft !== latex) updateAttributes({ latex: draft });
  }

  return (
    <NodeViewWrapper
      className={["sutra-math-block", selected ? "is-selected" : ""].join(" ")}
    >
      {editing ? (
        <textarea
          ref={areaRef}
          value={draft}
          aria-label="Display formula"
          spellCheck={false}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            e.stopPropagation();
            // Escape finishes and keeps what was typed; undo is the way back,
            // as it is everywhere else in the editor. Enter inserts a newline
            // instead, because formulas are routinely multi-line.
            if (e.key === "Escape") {
              e.preventDefault();
              commit();
            }
          }}
          rows={Math.max(draft.split("\n").length, 2)}
          className="sutra-math-textarea"
          placeholder="\ce{Sb2Se3 + 3I2 <=> 2SbI3 + 3Se}"
        />
      ) : (
        <div
          role="button"
          tabIndex={0}
          contentEditable={false}
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
          className="sutra-math-rendered"
          // See the note in MathInlineView: KaTeX produces this with
          // trust:false, so no markup from the note reaches the DOM directly.
          dangerouslySetInnerHTML={{ __html: html }}
        />
      )}
    </NodeViewWrapper>
  );
}
