import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { useCitation, useCitationPosition } from "./citationStore";
import { marker } from "../../notes/citationStyle";

/**
 * How a citation looks inline.
 *
 * Three states, each meaning something different to the author: still
 * resolving, resolved, and asked-for-but-not-found. The last is shown rather
 * than hidden — the reference is in the file either way, and a citation that
 * quietly disappears is worse than one that admits it is unresolved.
 *
 * How a resolved one reads is `marker`'s decision: a number in a numbering
 * style, the library's own text in an author-date one. The number is this
 * note's, because Zotero renders one item at a time and cannot know what else
 * the note cites — every citation would otherwise be "(1)".
 *
 * A citation still pointing at a Zotero key is marked, because it is the one
 * kind that stops working when Zotero does. That mark is what makes the
 * migration something you can see the need for rather than be told about.
 */
export default function CitationView({ node, selected }: NodeViewProps) {
  const ref = (node.attrs.ref as string) ?? "";
  const state = useCitation(ref);
  const position = useCitationPosition(ref);

  const text =
    state.status === "found"
      ? marker(state.cited.styled, state.cited.label, position)
      : state.status === "missing"
        ? state.legacy
          ? `(not in Zotero: ${ref})`
          : "(source note missing)"
        : "(…)";

  const legacy =
    state.status === "found"
      ? state.cited.legacy
      : state.status === "missing" && state.legacy;

  return (
    <NodeViewWrapper as="span" className="sutra-citation-wrapper">
      <span
        contentEditable={false}
        title={
          state.status === "found"
            ? legacy
              ? `${state.cited.title} — still a Zotero reference, not yet a source note`
              : state.cited.title
            : state.status === "loading"
              ? "Looking this reference up…"
              : // The id, and what it means that nothing answers to it. Shown
                // here, in the tooltip, rather than in the sentence: it is what
                // finds the file, and it is not what the paper is called.
                `No note in this vault has the id ${ref}. It may be in .sutra/trash, or on a machine that has not synced yet.`
        }
        className={[
          "sutra-citation",
          selected ? "is-selected" : "",
          state.status === "missing" ? "is-missing" : "",
          state.status === "loading" ? "is-loading" : "",
          legacy ? "is-legacy" : "",
        ].join(" ")}
      >
        {text}
      </span>
    </NodeViewWrapper>
  );
}
