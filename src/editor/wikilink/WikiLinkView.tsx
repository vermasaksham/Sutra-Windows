import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { followWikiLink, useNoteTitle } from "./titleStore";

/**
 * How a `[[id]]` looks in the editor: the target's title, not the id.
 *
 * A note that no longer exists renders as dangling rather than vanishing. The
 * text is still in the file, so hiding it would misrepresent the note.
 */
export default function WikiLinkView({ node }: NodeViewProps) {
  const targetId = (node.attrs.targetId as string | null) ?? "";
  const title = useNoteTitle(targetId);
  const missing = title === undefined;

  return (
    <NodeViewWrapper as="span" className="sutra-wikilink-wrapper">
      <span
        role="link"
        tabIndex={0}
        // contentEditable={false} keeps the caret from entering the node; the
        // atom flag governs the document model, this governs the DOM.
        contentEditable={false}
        title={missing ? `No note with id ${targetId}` : title}
        onClick={() => !missing && followWikiLink(targetId)}
        onKeyDown={(e) => {
          if ((e.key === "Enter" || e.key === " ") && !missing) {
            e.preventDefault();
            followWikiLink(targetId);
          }
        }}
        className={missing ? "sutra-wikilink is-missing" : "sutra-wikilink"}
      >
        {missing ? "Missing note" : title}
      </span>
    </NodeViewWrapper>
  );
}
