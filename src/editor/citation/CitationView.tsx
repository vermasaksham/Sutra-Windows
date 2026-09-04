import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { useCitation } from "./citationStore";

/**
 * How a citation looks inline.
 *
 * Three states, each meaning something different to the author: still
 * resolving, resolved, and asked-for-but-not-found. The last is shown rather
 * than hidden — the reference is in the file either way, and a citation that
 * quietly disappears is worse than one that admits it is unresolved.
 *
 * A citation still pointing at a Zotero key is marked, because it is the one
 * kind that stops working when Zotero does. That mark is what makes the
 * migration something you can see the need for rather than be told about.
 */
export default function CitationView({ node, selected }: NodeViewProps) {
  const ref = (node.attrs.ref as string) ?? "";
  const state = useCitation(ref);

  const text =
    state.status === "found"
      ? `(${state.cited.label})`
      : state.status === "missing"
        ? state.legacy
          ? `(not in Zotero: ${ref})`
          : "(source not in this vault)"
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
            : `Reference ${ref}`
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
