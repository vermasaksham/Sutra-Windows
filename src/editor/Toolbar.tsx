import { Fragment, useEffect, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";
import { isVertical, nearestDock, setDock, useDock } from "./toolbarDock";

/**
 * The editing toolbar.
 *
 * Every control is *probed* rather than assumed: an item appears only if the
 * editor reports it can run the command. The extension set is assembled in
 * extensions.ts and changes between TipTap versions, so a hardcoded list would
 * eventually show a button that does nothing — which is worse than not showing
 * it, because a dead button looks like a bug in the document rather than in
 * the toolbar.
 *
 * It docks to one of four edges, dragged by the handle. Not free positioning:
 * a toolbar that can go anywhere ends up half off-screen, and then it has to
 * be rescued.
 */

type Item = {
  id: string;
  label: string;
  /** One or two characters. The app ships no icon font, and drawing thirty
   *  glyphs would be a worse use of the space than the letters themselves. */
  glyph: string;
  run: (editor: Editor) => boolean;
  can: (editor: Editor) => boolean;
  active?: (editor: Editor) => boolean;
  group: number;
};

const ITEMS: Item[] = [
  {
    id: "undo",
    label: "Undo",
    glyph: "↶",
    group: 0,
    run: (e) => e.chain().focus().undo().run(),
    can: (e) => e.can().chain().undo().run(),
  },
  {
    id: "redo",
    label: "Redo",
    glyph: "↷",
    group: 0,
    run: (e) => e.chain().focus().redo().run(),
    can: (e) => e.can().chain().redo().run(),
  },

  {
    id: "bold",
    label: "Bold",
    glyph: "B",
    group: 1,
    run: (e) => e.chain().focus().toggleBold().run(),
    can: (e) => e.can().chain().toggleBold().run(),
    active: (e) => e.isActive("bold"),
  },
  {
    id: "italic",
    label: "Italic",
    glyph: "I",
    group: 1,
    run: (e) => e.chain().focus().toggleItalic().run(),
    can: (e) => e.can().chain().toggleItalic().run(),
    active: (e) => e.isActive("italic"),
  },
  {
    id: "underline",
    label: "Underline",
    glyph: "U",
    group: 1,
    // Present in some TipTap builds and not others, which is exactly why
    // every item is probed rather than listed.
    run: (e) => e.chain().focus().toggleUnderline().run(),
    can: (e) => e.can().chain().toggleUnderline().run(),
    active: (e) => e.isActive("underline"),
  },
  {
    id: "strike",
    label: "Strikethrough",
    glyph: "S",
    group: 1,
    run: (e) => e.chain().focus().toggleStrike().run(),
    can: (e) => e.can().chain().toggleStrike().run(),
    active: (e) => e.isActive("strike"),
  },
  {
    id: "code",
    label: "Inline code",
    glyph: "‹›",
    group: 1,
    run: (e) => e.chain().focus().toggleCode().run(),
    can: (e) => e.can().chain().toggleCode().run(),
    active: (e) => e.isActive("code"),
  },

  {
    id: "p",
    label: "Body text",
    glyph: "¶",
    group: 2,
    run: (e) => e.chain().focus().setParagraph().run(),
    can: (e) => e.can().chain().setParagraph().run(),
    active: (e) => e.isActive("paragraph"),
  },
  ...([1, 2, 3] as const).map((level) => ({
    id: `h${level}`,
    label: `Heading ${level}`,
    glyph: `H${level}`,
    group: 2,
    run: (e: Editor) => e.chain().focus().toggleHeading({ level }).run(),
    can: (e: Editor) => e.can().chain().toggleHeading({ level }).run(),
    active: (e: Editor) => e.isActive("heading", { level }),
  })),

  {
    id: "ul",
    label: "Bulleted list",
    glyph: "•",
    group: 3,
    run: (e) => e.chain().focus().toggleBulletList().run(),
    can: (e) => e.can().chain().toggleBulletList().run(),
    active: (e) => e.isActive("bulletList"),
  },
  {
    id: "ol",
    label: "Numbered list",
    glyph: "1.",
    group: 3,
    run: (e) => e.chain().focus().toggleOrderedList().run(),
    can: (e) => e.can().chain().toggleOrderedList().run(),
    active: (e) => e.isActive("orderedList"),
  },
  {
    id: "task",
    label: "To-do list",
    glyph: "☑",
    group: 3,
    run: (e) => e.chain().focus().toggleTaskList().run(),
    can: (e) => e.can().chain().toggleTaskList().run(),
    active: (e) => e.isActive("taskList"),
  },

  {
    id: "quote",
    label: "Quote",
    glyph: "❝",
    group: 4,
    run: (e) => e.chain().focus().toggleBlockquote().run(),
    can: (e) => e.can().chain().toggleBlockquote().run(),
    active: (e) => e.isActive("blockquote"),
  },
  {
    id: "codeblock",
    label: "Code block",
    glyph: "{ }",
    group: 4,
    run: (e) => e.chain().focus().toggleCodeBlock().run(),
    can: (e) => e.can().chain().toggleCodeBlock().run(),
    active: (e) => e.isActive("codeBlock"),
  },
  {
    id: "rule",
    label: "Divider",
    glyph: "—",
    group: 4,
    run: (e) => e.chain().focus().setHorizontalRule().run(),
    can: (e) => e.can().chain().setHorizontalRule().run(),
  },
  {
    id: "math",
    label: "Equation",
    glyph: "∑",
    group: 4,
    run: (e) =>
      e
        .chain()
        .focus()
        .insertContent({ type: "mathBlock", attrs: { latex: "" } })
        .run(),
    can: (e) =>
      e
        .can()
        .chain()
        .insertContent({ type: "mathBlock", attrs: { latex: "" } })
        .run(),
  },
  {
    id: "chem",
    label: "Chemical equation",
    glyph: "⚗",
    group: 4,
    run: (e) =>
      e
        .chain()
        .focus()
        .insertContent({ type: "mathBlock", attrs: { latex: "\\ce{}" } })
        .run(),
    can: (e) =>
      e
        .can()
        .chain()
        .insertContent({ type: "mathBlock", attrs: { latex: "\\ce{}" } })
        .run(),
  },
  {
    id: "table",
    label: "Table",
    glyph: "▦",
    group: 4,
    run: (e) =>
      e
        .chain()
        .focus()
        .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
        .run(),
    can: (e) =>
      e
        .can()
        .chain()
        .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
        .run(),
  },

  {
    id: "clear",
    label: "Clear formatting",
    glyph: "✕",
    group: 5,
    run: (e) => e.chain().focus().unsetAllMarks().clearNodes().run(),
    can: (e) => e.can().chain().unsetAllMarks().run(),
  },
];

export default function Toolbar({ editor }: { editor: Editor | null }) {
  const dock = useDock();
  const [, bump] = useState(0);
  const [dragging, setDragging] = useState(false);
  const [hint, setHint] = useState<null | ReturnType<typeof nearestDock>>(null);
  const bar = useRef<HTMLDivElement>(null);

  // Re-render on every selection or document change, so the pressed states and
  // the enabled states are about where the caret actually is.
  useEffect(() => {
    if (!editor) return;
    const refresh = () => bump((n) => n + 1);
    editor.on("selectionUpdate", refresh);
    editor.on("transaction", refresh);
    return () => {
      editor.off("selectionUpdate", refresh);
      editor.off("transaction", refresh);
    };
  }, [editor]);

  if (!editor) return null;

  const vertical = isVertical(dock);
  const available = ITEMS.filter((item) => {
    try {
      return item.can(editor);
    } catch {
      // `can()` throws when the command does not exist at all, which is the
      // main thing being probed for.
      return false;
    }
  });

  function onPointerDown(event: React.PointerEvent) {
    event.preventDefault();
    setDragging(true);
    const move = (e: PointerEvent) =>
      setHint(
        nearestDock(
          e.clientX,
          e.clientY,
          window.innerWidth,
          window.innerHeight,
        ),
      );
    const up = (e: PointerEvent) => {
      setDock(
        nearestDock(
          e.clientX,
          e.clientY,
          window.innerWidth,
          window.innerHeight,
        ),
      );
      setDragging(false);
      setHint(null);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  return (
    <>
      {/* While dragging, show where it would land. Otherwise the toolbar
          appears to jump on release with no warning. */}
      {dragging && hint && (
        <div
          aria-hidden
          className={[
            "sutra-no-print pointer-events-none fixed z-40 bg-accent/25",
            hint === "top" && "inset-x-0 top-0 h-12",
            hint === "bottom" && "inset-x-0 bottom-0 h-12",
            hint === "left" && "inset-y-0 left-0 w-12",
            hint === "right" && "inset-y-0 right-0 w-12",
          ]
            .filter(Boolean)
            .join(" ")}
        />
      )}

      <div
        ref={bar}
        role="toolbar"
        aria-label="Formatting"
        aria-orientation={vertical ? "vertical" : "horizontal"}
        className={[
          "sutra-no-print z-30 flex items-center gap-0.5 rounded-xl border border-border bg-surface shadow-pane",
          // Absolute inside <main>, which is `relative` — so a side dock sits
          // beside the note rather than on top of the sidebar it would
          // otherwise cover. `sticky` would not do: the note column scrolls,
          // and the toolbar should not scroll away from the text it formats.
          vertical
            ? "absolute top-1/2 max-h-[80vh] -translate-y-1/2 flex-col overflow-y-auto px-1 py-1.5"
            : "flex-row flex-wrap px-1.5 py-1",
          dock === "left" && "left-3",
          dock === "right" && "right-3",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        <button
          type="button"
          onPointerDown={onPointerDown}
          aria-label="Move the toolbar — drag to any edge"
          title="Drag to any edge"
          className={[
            "shrink-0 cursor-grab rounded text-ink-muted transition-colors duration-150 ease-out hover:text-accent active:cursor-grabbing",
            vertical ? "px-1 py-0.5" : "px-0.5 py-1",
          ].join(" ")}
        >
          <span aria-hidden className="block text-xs leading-none select-none">
            {vertical ? "⋯" : "⋮"}
          </span>
        </button>

        {available.map((item, i) => {
          const previous = available[i - 1];
          const newGroup = previous && previous.group !== item.group;
          const active = item.active?.(editor) ?? false;
          return (
            // A fragment, not a wrapper: the divider has to be a sibling of the
            // button inside the toolbar's own flex container, or a column
            // layout lays it out beside the button rather than above it.
            <Fragment key={item.id}>
              {newGroup && (
                <span
                  aria-hidden
                  className={
                    vertical
                      ? "my-1 h-px w-5 shrink-0 bg-border"
                      : "mx-1 h-4 w-px shrink-0 bg-border"
                  }
                />
              )}
              <button
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => item.run(editor)}
                aria-label={item.label}
                aria-pressed={item.active ? active : undefined}
                title={item.label}
                className={[
                  "grid size-7 shrink-0 place-items-center rounded text-xs transition-colors duration-150 ease-out",
                  active
                    ? "bg-accent-bg font-semibold text-accent"
                    : "text-ink-soft hover:bg-row-hover hover:text-ink",
                  item.id === "bold" && "font-bold",
                  item.id === "italic" && "italic",
                  item.id === "underline" && "underline",
                  item.id === "strike" && "line-through",
                ]
                  .filter(Boolean)
                  .join(" ")}
              >
                {item.glyph}
              </button>
            </Fragment>
          );
        })}
      </div>
    </>
  );
}
