import StarterKit from "@tiptap/starter-kit";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { TableKit } from "@tiptap/extension-table";
import Image from "@tiptap/extension-image";
import { Placeholder } from "@tiptap/extensions";
import { SlashCommand } from "./slash/SlashCommand";

/**
 * The block vocabulary of a Sutra note.
 *
 * StarterKit already brings paragraphs, headings, both list kinds, code blocks,
 * quotes, dividers, hard breaks, links, the inline marks, undo/redo, the drop
 * and gap cursors, and a trailing node so there is always somewhere to click
 * below the last block. Everything below is what it does not cover.
 *
 * Deliberately absent: anything that belongs to a later phase. No maths nodes
 * (Phase 5), no wikilinks (Phase 4), no persistence of any kind.
 */
export const extensions = [
  StarterKit.configure({
    heading: { levels: [1, 2, 3] },
    // The drop cursor is the line that shows where a dragged block will land,
    // so it should read as an interactive accent like everything else.
    dropcursor: { color: "var(--accent)", width: 2 },
    link: {
      openOnClick: false,
      HTMLAttributes: { class: "sutra-link" },
    },
  }),

  // Checkboxes. `nested` lets a task item contain its own sub-list.
  TaskList,
  TaskItem.configure({ nested: true }),

  TableKit.configure({
    table: { resizable: true, allowTableNodeSelection: true },
  }),

  Image.configure({
    inline: false,
    allowBase64: true,
    HTMLAttributes: { class: "sutra-image" },
  }),

  Placeholder.configure({
    // Only the block the cursor is in gets prompted, and only when the whole
    // document is otherwise untouched — a placeholder on every empty line is
    // noise.
    showOnlyCurrent: true,
    placeholder: ({ node, editor }) => {
      if (node.type.name === "heading") return "Heading";
      if (editor.isEmpty) return "Write something, or press / for blocks";
      return "";
    },
  }),

  SlashCommand,
];
