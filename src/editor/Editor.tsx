import { useEffect } from "react";
import { EditorContent, useEditor } from "@tiptap/react";
import type { Editor as TiptapEditor } from "@tiptap/core";
import { DragHandle } from "@tiptap/extension-drag-handle-react";
import { extensions } from "./extensions";
import { GripIcon } from "./icons";
import { getMarkdown, setMarkdown } from "./markdown";

type Props = {
  /** The note body, as markdown. */
  body: string;
  /** Called with markdown on every edit. */
  onChange: (markdown: string) => void;
  /** Hands the live editor up, so export can read the document as JSON rather
   *  than re-parsing the markdown it was built from. */
  onReady?: (editor: TiptapEditor | null) => void;
};

export default function Editor({ body, onChange, onReady }: Props) {
  const editor = useEditor({
    extensions,
    // Content is loaded in onCreate instead of here: parsing markdown needs the
    // Markdown extension's manager, which does not exist until the editor does.
    content: "",
    onCreate: ({ editor }) => setMarkdown(editor, body),
    onUpdate: ({ editor }) => onChange(getMarkdown(editor)),
    editorProps: {
      attributes: {
        class: "sutra-prose selectable",
        role: "textbox",
        "aria-multiline": "true",
        "aria-label": "Note body",
      },
    },
  });

  useEffect(() => {
    onReady?.(editor);
    return () => onReady?.(null);
  }, [editor, onReady]);

  if (!editor) return null;

  return (
    <>
      {/*
        The handle is portalled to the body and follows whichever block the
        pointer is over.

        `nested` is passed as a bare `true` deliberately. It is not just an
        on switch: `true` also turns on left-edge detection, which is what
        makes the handle grab the *container* when the pointer is near a
        block's left margin — the path a pointer takes on its way to the
        gutter handle. Hovering the middle of a quote targets the paragraph
        inside it; approaching from the left targets the quote itself.

        Passing an options object instead silently resets edge detection to
        'none', and top-level blocks then stop being grabbable at all.
      */}
      <DragHandle editor={editor} nested className="sutra-drag-handle-wrapper">
        {/*
          A span, not a button. The extension puts `draggable` and the
          dragstart listener on the wrapper it owns; a nested button
          intercepts the press and the drag never starts. It is also hidden
          from assistive tech on purpose — it is a pointer-only affordance
          that does nothing on click, so announcing it as a button would be
          a lie. Keyboard block movement is a Phase 6 shortcut.
        */}
        <span aria-hidden="true" className="sutra-drag-handle">
          <GripIcon className="size-4" />
        </span>
      </DragHandle>

      <EditorContent editor={editor} />
    </>
  );
}
