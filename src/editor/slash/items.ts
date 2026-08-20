import type { Editor, Range } from "@tiptap/core";
import type { ComponentType, SVGProps } from "react";
import {
  BulletListIcon,
  CodeIcon,
  DividerIcon,
  H1Icon,
  H2Icon,
  H3Icon,
  ImageIcon,
  OrderedListIcon,
  QuoteIcon,
  TableIcon,
  TaskListIcon,
  TextIcon,
} from "../icons";

export type SlashItem = {
  id: string;
  title: string;
  hint: string;
  /** Extra search terms that should match this item but need not be shown. */
  keywords: string[];
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  group: "Basic" | "Lists" | "Blocks";
  /**
   * Items either run immediately, or ask for one piece of text first. Images
   * are the only thing that needs asking in this phase — once Phase 3 owns the
   * filesystem, this becomes a file picker and the prompt goes away.
   */
  prompt?: { label: string; placeholder: string };
  run: (editor: Editor, range: Range, value?: string) => void;
};

export const SLASH_ITEMS: SlashItem[] = [
  {
    id: "text",
    title: "Text",
    hint: "Plain paragraph",
    keywords: ["paragraph", "body", "p"],
    icon: TextIcon,
    group: "Basic",
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).setParagraph().run(),
  },
  {
    id: "h1",
    title: "Heading 1",
    hint: "Section title",
    keywords: ["title", "h1", "#"],
    icon: H1Icon,
    group: "Basic",
    run: (editor, range) =>
      editor
        .chain()
        .focus()
        .deleteRange(range)
        .setNode("heading", { level: 1 })
        .run(),
  },
  {
    id: "h2",
    title: "Heading 2",
    hint: "Subsection",
    keywords: ["subtitle", "h2", "##"],
    icon: H2Icon,
    group: "Basic",
    run: (editor, range) =>
      editor
        .chain()
        .focus()
        .deleteRange(range)
        .setNode("heading", { level: 2 })
        .run(),
  },
  {
    id: "h3",
    title: "Heading 3",
    hint: "Minor heading",
    keywords: ["h3", "###"],
    icon: H3Icon,
    group: "Basic",
    run: (editor, range) =>
      editor
        .chain()
        .focus()
        .deleteRange(range)
        .setNode("heading", { level: 3 })
        .run(),
  },
  {
    id: "bulletList",
    title: "Bulleted list",
    hint: "An unordered list",
    keywords: ["ul", "unordered", "point"],
    icon: BulletListIcon,
    group: "Lists",
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).toggleBulletList().run(),
  },
  {
    id: "orderedList",
    title: "Numbered list",
    hint: "An ordered list",
    keywords: ["ol", "ordered", "number"],
    icon: OrderedListIcon,
    group: "Lists",
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).toggleOrderedList().run(),
  },
  {
    id: "taskList",
    title: "To-do list",
    hint: "Checkboxes",
    keywords: ["todo", "task", "check", "box"],
    icon: TaskListIcon,
    group: "Lists",
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).toggleTaskList().run(),
  },
  {
    id: "codeBlock",
    title: "Code",
    hint: "Monospaced block",
    keywords: ["pre", "snippet", "```"],
    icon: CodeIcon,
    group: "Blocks",
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).setCodeBlock().run(),
  },
  {
    id: "blockquote",
    title: "Quote",
    hint: "Set-off passage",
    keywords: ["citation", "blockquote", ">"],
    icon: QuoteIcon,
    group: "Blocks",
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).toggleBlockquote().run(),
  },
  {
    id: "divider",
    title: "Divider",
    hint: "Horizontal rule",
    keywords: ["hr", "rule", "line", "---"],
    icon: DividerIcon,
    group: "Blocks",
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).setHorizontalRule().run(),
  },
  {
    id: "table",
    title: "Table",
    hint: "3 x 3 with a header row",
    keywords: ["grid", "rows", "columns"],
    icon: TableIcon,
    group: "Blocks",
    run: (editor, range) =>
      editor
        .chain()
        .focus()
        .deleteRange(range)
        .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
        .run(),
  },
  {
    id: "image",
    title: "Image",
    hint: "Embed by URL",
    keywords: ["picture", "photo", "figure"],
    icon: ImageIcon,
    group: "Blocks",
    prompt: { label: "Image URL", placeholder: "https://… or data:image/…" },
    run: (editor, range, value) => {
      const src = value?.trim();
      if (!src) return;
      editor.chain().focus().deleteRange(range).setImage({ src }).run();
    },
  },
];

/** Case-insensitive match over title, hint, and keywords. */
export function filterItems(query: string): SlashItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return SLASH_ITEMS;
  return SLASH_ITEMS.filter((item) =>
    [item.title, item.hint, ...item.keywords].some((field) =>
      field.toLowerCase().includes(q),
    ),
  );
}
