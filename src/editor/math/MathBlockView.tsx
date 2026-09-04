import { useEffect, useMemo, useRef, useState } from "react";
import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { renderMath } from "./render";
import { caretAt, isBlank } from "./source";

/**
 * Display maths: rendered and centred, editable as raw LaTeX on click.
 *
 * A newly inserted block opens straight into editing, since a formula with
 * nothing in it is nothing to look at and the user's next act is always to
 * type — and the caret lands inside the `\ce{}` that Chemical equation fills
 * in, which is where the reaction goes.
 */
export default function MathBlockView({
  node,
  updateAttributes,
  selected,
  editor,
  getPos,
}: NodeViewProps) {
  const latex = (node.attrs.latex as string) ?? "";
  // Blank rather than empty: Chemical equation pre-fills `\ce{}`, which has
  // nothing in it to look at, so it needs the caret just as much as "" does.
  const [editing, setEditing] = useState(isBlank(latex));
  const [draft, setDraft] = useState(latex);
  const areaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => setDraft(latex), [latex]);

  useEffect(() => {
    if (!editing) return;
    const area = areaRef.current;
    if (!area) return;
    area.focus();
    const caret = caretAt(area.value);
    area.setSelectionRange(caret, caret);
  }, [editing]);

  const html = useMemo(() => renderMath(latex, true), [latex]);

  function commit() {
    setEditing(false);
    if (draft !== latex) updateAttributes({ latex: draft });
  }

  /**
   * Finish, and put the caret back in the document after the formula.
   *
   * Closing the textarea unmounts the only focused element on the page, and
   * focus fell to the body: you finished an equation, carried on typing, and
   * the whole next sentence went nowhere. A formula is also frequently the
   * last thing in a note, and there was nothing after it to carry on *in*, so
   * one is added when it is missing.
   *
   * Only on Escape. Blur means the author clicked somewhere else, and taking
   * focus back from wherever they went would be worse than losing it.
   */
  function finish() {
    commit();
    const pos = typeof getPos === "function" ? getPos() : null;
    if (pos == null) return;
    const after = pos + node.nodeSize;
    const chain = editor.chain();
    if (after >= editor.state.doc.content.size) {
      chain.insertContentAt(after, { type: "paragraph" });
    }
    chain.focus(after).run();
  }

  return (
    <NodeViewWrapper
      className={[
        "sutra-math-block",
        // The selection outline is for a formula selected *as a node* — picked
        // up by the keyboard, about to be moved or deleted. While the source
        // is open it says nothing the caret has not already said, and a
        // freshly inserted equation is selected, so it drew a box around
        // exactly the moment the author is trying to write in.
        selected && !editing ? "is-selected" : "",
      ].join(" ")}
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
              finish();
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
