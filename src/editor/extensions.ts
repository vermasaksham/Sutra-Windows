import StarterKit from "@tiptap/starter-kit";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { TableKit } from "@tiptap/extension-table";
import Image from "@tiptap/extension-image";
import { ReactNodeViewRenderer } from "@tiptap/react";
import ImageView from "./image/ImageView";
import { Placeholder } from "@tiptap/extensions";
import { Markdown } from "@tiptap/markdown";
import { SlashCommand } from "./slash/SlashCommand";
import { Citation } from "./citation/Citation";
import { CitationSuggestion } from "./citation/CitationSuggestion";
import { MathBlock } from "./math/MathBlock";
import { MathInline } from "./math/MathInline";
import { WikiLink } from "./wikilink/WikiLink";
import { WikiLinkSuggestion } from "./wikilink/WikiLinkSuggestion";

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

  // A node view, so the stored `src` stays a vault-relative reference while
  // the displayed URL goes through the sutra:// scheme. Nothing about
  // serialisation changes: what is on disk is what was written.
  Image.extend({
    addNodeView() {
      return ReactNodeViewRenderer(ImageView);
    },
  }).configure({
    inline: false,
    allowBase64: true,
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

  // Markdown is the on-disk format, so the editor has to speak it directly.
  // This registers the parse/render specs for every node above; see
  // ./markdown.ts for why the conversion lives here and not in Rust.
  Markdown,

  SlashCommand,

  // Links between notes. Stored as [[id]] and rendered as the target's current
  // title, so renaming a note cannot break a link.
  WikiLink,
  WikiLinkSuggestion,

  // Maths and chemistry. MathBlock is registered before MathInline so the
  // block tokenizer sees `$$` first — otherwise the inline rule would match
  // the opening `$$` as an empty formula and the fence would never form.
  MathBlock,
  MathInline,

  // Citations. Stored as [@KEY] and rendered from the live Zotero entry, so
  // editing a reference there updates every citation of it.
  Citation,
  CitationSuggestion,
];
