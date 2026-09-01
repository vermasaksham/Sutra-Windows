import type { Editor, Range } from "@tiptap/core";
import { notesApi } from "../../vault/api";
import { currentFolder } from "../../notes/folderStore";
import type { ComponentType, SVGProps } from "react";
import {
  BulletListIcon,
  ChemistryIcon,
  CodeIcon,
  DividerIcon,
  H1Icon,
  H2Icon,
  H3Icon,
  ImageIcon,
  MathIcon,
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
  group: "Basic" | "Lists" | "Blocks" | "Table";
  /**
   * Whether this item applies where the caret is.
   *
   * Absent means always. Used by the table commands, which are meaningless
   * outside a table and would be noise in every other menu.
   */
  when?: (editor: Editor) => boolean;
  /**
   * The result is ignored, so this is typed `unknown` rather than `void`: the
   * chained editor commands return a boolean, and the image picker returns a
   * promise because it waits on a native dialog. The menu closes as soon as
   * the item runs, whichever it is.
   */
  run: (editor: Editor, range: Range) => unknown;
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
    id: "math",
    title: "Equation",
    hint: "Display maths block",
    keywords: ["latex", "katex", "formula", "$$", "maths", "math"],
    icon: MathIcon,
    group: "Blocks",
    run: (editor, range) =>
      editor
        .chain()
        .focus()
        .deleteRange(range)
        // Empty on purpose: the block opens straight into its editor, so the
        // next keystroke goes into the formula.
        .insertContent({ type: "mathBlock", attrs: { latex: "" } })
        .run(),
  },
  {
    id: "chemistry",
    title: "Chemical equation",
    hint: "Display block, pre-filled with \\ce{}",
    keywords: ["mhchem", "reaction", "ce", "chem"],
    icon: ChemistryIcon,
    group: "Blocks",
    run: (editor, range) =>
      editor
        .chain()
        .focus()
        .deleteRange(range)
        // Pre-filled, because \ce{} is the part nobody remembers and the
        // formula inside it is the part they came to write.
        .insertContent({ type: "mathBlock", attrs: { latex: "\\ce{}" } })
        .run(),
  },
  {
    id: "image",
    title: "Image",
    hint: "Copy a file into the vault",
    keywords: ["picture", "photo", "figure"],
    icon: ImageIcon,
    group: "Blocks",
    // Opens the native file picker on the Rust side, which copies the chosen
    // file into the vault's attachments folder and returns the relative
    // reference to store. No path reaches this side, and the vault stays
    // self-contained rather than pointing at a file elsewhere on the disk that
    // could later move.
    run: async (editor, range) => {
      const reference = await notesApi.attach(currentFolder());
      if (!reference) return; // Cancelled.
      editor
        .chain()
        .focus()
        .deleteRange(range)
        .setImage({ src: reference })
        .run();
    },
  },
  // ---- inside a table ------------------------------------------------------
  //
  // A table is the one block here that cannot be removed by putting the caret
  // near it and pressing Backspace — ProseMirror deletes a cell's contents
  // instead, so the grid stays and only empties. These give it the way out,
  // and the rest of the shape while the menu is open anyway.
  {
    id: "table-delete",
    title: "Delete this table",
    hint: "Removes the whole grid",
    keywords: ["remove", "drop", "table", "grid"],
    icon: TableIcon,
    group: "Table",
    when: (editor) => editor.isActive("table"),
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).deleteTable().run(),
  },
  {
    id: "table-row-after",
    title: "Add a row below",
    hint: "",
    keywords: ["row", "insert", "table"],
    icon: TableIcon,
    group: "Table",
    when: (editor) => editor.isActive("table"),
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).addRowAfter().run(),
  },
  {
    id: "table-row-delete",
    title: "Delete this row",
    hint: "",
    keywords: ["row", "remove", "table"],
    icon: TableIcon,
    group: "Table",
    when: (editor) => editor.isActive("table"),
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).deleteRow().run(),
  },
  {
    id: "table-column-after",
    title: "Add a column to the right",
    hint: "",
    keywords: ["column", "insert", "table"],
    icon: TableIcon,
    group: "Table",
    when: (editor) => editor.isActive("table"),
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).addColumnAfter().run(),
  },
  {
    id: "table-column-delete",
    title: "Delete this column",
    hint: "",
    keywords: ["column", "remove", "table"],
    icon: TableIcon,
    group: "Table",
    when: (editor) => editor.isActive("table"),
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).deleteColumn().run(),
  },
];

/**
 * Case-insensitive match over title, hint, and keywords.
 *
 * The editor is needed as well as the query because some items only apply
 * where the caret is — the table commands are meaningless anywhere else, and
 * offering "Delete this table" in an ordinary paragraph would be a menu item
 * that does nothing.
 */
export function filterItems(query: string, editor?: Editor): SlashItem[] {
  const q = query.trim().toLowerCase();
  const here = SLASH_ITEMS.filter(
    (item) => !item.when || (editor != null && item.when(editor)),
  );
  if (!q) return here;
  return here.filter((item) =>
    [item.title, item.hint, ...item.keywords].some((field) =>
      field.toLowerCase().includes(q),
    ),
  );
}
