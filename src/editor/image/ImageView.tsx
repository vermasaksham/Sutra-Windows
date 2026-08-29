import { useState } from "react";
import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";
import { attachmentUrl } from "./attachmentUrl";

/**
 * An image in a note.
 *
 * The `src` attribute stays exactly as it is in the markdown — a vault-relative
 * reference — and only the displayed URL is resolved. Serialisation therefore
 * needs no special handling: what is stored is what was written.
 */
export default function ImageView({ node, selected }: NodeViewProps) {
  const src = (node.attrs.src as string) ?? "";
  const alt = (node.attrs.alt as string) ?? "";
  const [failed, setFailed] = useState(false);

  return (
    <NodeViewWrapper className="sutra-image-wrapper">
      {failed ? (
        // A broken image is worth saying out loud: the file may have been moved
        // out of the vault, and silently showing nothing would hide that.
        <p className="sutra-image-missing">
          Missing attachment: <code>{src}</code>
        </p>
      ) : (
        <img
          src={attachmentUrl(src)}
          alt={alt}
          draggable={false}
          onError={() => setFailed(true)}
          className={["sutra-image", selected ? "is-selected" : ""].join(" ")}
        />
      )}
    </NodeViewWrapper>
  );
}
