import { useState } from "react";
import { attachmentUrl } from "../editor/image/attachmentUrl";
import IconPicker from "./IconPicker";
import TagEditor from "./TagEditor";
import type { NoteDoc } from "../vault/api";

/**
 * Everything above the note body: cover, icon, title, tags.
 *
 * All of it is page-level metadata, so all of it lives in frontmatter and none
 * of it touches the markdown body.
 */
export default function NoteHeader({
  doc,
  onTitle,
  onIcon,
  onCover,
  onTags,
  onSelectTag,
}: {
  doc: NoteDoc;
  onTitle: (title: string) => void;
  onIcon: (icon: string | null) => void;
  onCover: () => void;
  onTags: (tags: string[]) => void;
  onSelectTag: (tag: string) => void;
}) {
  const [picking, setPicking] = useState(false);
  const [coverFailed, setCoverFailed] = useState(false);

  return (
    <>
      {doc.cover && !coverFailed && (
        <div className="sutra-cover">
          <img
            src={attachmentUrl(doc.cover)}
            alt=""
            onError={() => setCoverFailed(true)}
            draggable={false}
          />
          <button
            type="button"
            onClick={onCover}
            className="sutra-cover-change"
          >
            Change cover
          </button>
        </div>
      )}

      <div className="relative mb-1 flex items-center gap-2">
        <button
          type="button"
          onClick={() => setPicking((open) => !open)}
          aria-label={doc.icon ? "Change icon" : "Add icon"}
          className={[
            "rounded-md transition-colors duration-150 ease-out",
            doc.icon
              ? "text-4xl leading-none"
              : "px-2 py-0.5 text-xs text-ink-muted opacity-0 hover:text-ink focus-visible:opacity-100 group-hover/page:opacity-100",
          ].join(" ")}
        >
          {doc.icon ?? "＋ icon"}
        </button>

        {!doc.cover && (
          <button
            type="button"
            onClick={onCover}
            className="rounded-md px-2 py-0.5 text-xs text-ink-muted opacity-0 transition-opacity duration-150 ease-out hover:text-ink group-hover/page:opacity-100 focus-visible:opacity-100"
          >
            ＋ cover
          </button>
        )}

        {picking && (
          <IconPicker
            icon={doc.icon}
            onPick={(icon) => {
              onIcon(icon);
              setPicking(false);
            }}
            onClose={() => setPicking(false)}
          />
        )}
      </div>

      <input
        value={doc.title}
        onChange={(e) => onTitle(e.target.value)}
        placeholder="Untitled"
        aria-label="Note title"
        className="mb-2 w-full bg-transparent text-4xl font-semibold tracking-tight text-ink outline-none placeholder:text-ink-muted"
      />

      <TagEditor tags={doc.tags} onChange={onTags} onSelect={onSelectTag} />
    </>
  );
}
