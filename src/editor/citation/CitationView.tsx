import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { label, useCitation } from "./citationStore";

/**
 * How a citation looks inline.
 *
 * Three states, all of them meaning something different to the author:
 * still fetching, resolved, and asked-for-but-not-in-the-library. The last is
 * shown in the highlight colour rather than hidden — the key is in the file either way, and
 * a citation that quietly disappears is worse than one that admits it is
 * unresolved.
 */
export default function CitationView({ node, selected }: NodeViewProps) {
  const itemKey = (node.attrs.itemKey as string) ?? "";
  const state = useCitation(itemKey);

  const text =
    state.status === "found"
      ? `(${label(state.reference)})`
      : state.status === "missing"
        ? `(not in Zotero: ${itemKey})`
        : "(…)";

  return (
    <NodeViewWrapper as="span" className="sutra-citation-wrapper">
      <span
        contentEditable={false}
        title={
          state.status === "found"
            ? state.reference.title
            : `Zotero item ${itemKey}`
        }
        className={[
          "sutra-citation",
          selected ? "is-selected" : "",
          state.status === "missing" ? "is-missing" : "",
          state.status === "loading" ? "is-loading" : "",
        ].join(" ")}
      >
        {text}
      </span>
    </NodeViewWrapper>
  );
}
