import type { JSONContent } from "@tiptap/core";
import { label, resolved } from "../editor/citation/citationStore";
import { mathToPng } from "./mathToPng";
import { attachmentUrl } from "../editor/image/attachmentUrl";

/**
 * A note flattened into something a document writer can consume.
 *
 * Deliberately dumb: a list of blocks, each a list of runs. Rust does not need
 * to know about ProseMirror, and the frontend does not need to know about
 * OOXML. Anything that requires a browser to produce — rasterised formulas,
 * fetched image bytes — is resolved here, because Rust has neither a canvas nor
 * a TeX engine.
 */

export type Run = {
  text: string;
  bold?: boolean;
  italic?: boolean;
  code?: boolean;
  strike?: boolean;
  link?: string;
};

export type Block =
  | { kind: "heading"; level: number; runs: Run[] }
  | { kind: "paragraph"; runs: Run[] }
  | { kind: "quote"; runs: Run[] }
  | { kind: "code"; text: string }
  | { kind: "listItem"; ordered: boolean; depth: number; checked?: boolean; runs: Run[] }
  | { kind: "divider" }
  | { kind: "table"; rows: string[][]; headerRow: boolean }
  /** PNG bytes, base64. Both maths and attachments arrive as this. */
  | { kind: "image"; data: string; width: number; height: number; alt: string };

export type ExportDocument = {
  title: string;
  blocks: Block[];
  /** Formatted bibliography lines, if the note cites anything. */
  references: string[];
};

const MARKS: Record<string, keyof Run> = {
  bold: "bold",
  italic: "italic",
  code: "code",
  strike: "strike",
};

function runsFrom(nodes: JSONContent[] | undefined): Run[] {
  const runs: Run[] = [];
  for (const node of nodes ?? []) {
    if (node.type === "text") {
      const run: Run = { text: node.text ?? "" };
      for (const mark of node.marks ?? []) {
        const key = MARKS[mark.type ?? ""];
        if (key) (run as Record<string, unknown>)[key] = true;
        if (mark.type === "link") run.link = String(mark.attrs?.href ?? "");
      }
      runs.push(run);
    } else if (node.type === "wikiLink") {
      // A link between notes has no meaning outside the vault, so it exports
      // as the target's title — the same thing it shows on screen.
      runs.push({ text: String(node.attrs?.targetId ?? ""), italic: true });
    } else if (node.type === "citation") {
      const key = String(node.attrs?.itemKey ?? "");
      const [reference] = resolved([key]);
      runs.push({ text: reference ? `(${label(reference)})` : `(${key})` });
    } else if (node.type === "hardBreak") {
      runs.push({ text: "\n" });
    }
  }
  return runs;
}

/** Inline maths becomes its own image block, since a run cannot hold a picture. */
async function inlineMathBlocks(nodes: JSONContent[] | undefined): Promise<Block[]> {
  const blocks: Block[] = [];
  for (const node of nodes ?? []) {
    if (node.type === "mathInline") {
      const latex = String(node.attrs?.latex ?? "");
      const rendered = await mathToPng(latex, false);
      if (rendered) {
        blocks.push({
          kind: "image",
          data: rendered.png,
          width: rendered.width,
          height: rendered.height,
          alt: latex,
        });
      }
    }
  }
  return blocks;
}

async function walk(
  node: JSONContent,
  blocks: Block[],
  depth = 0,
  ordered = false,
): Promise<void> {
  switch (node.type) {
    case "heading":
      blocks.push({
        kind: "heading",
        level: Number(node.attrs?.level ?? 1),
        runs: runsFrom(node.content),
      });
      blocks.push(...(await inlineMathBlocks(node.content)));
      break;

    case "paragraph":
      blocks.push({ kind: "paragraph", runs: runsFrom(node.content) });
      blocks.push(...(await inlineMathBlocks(node.content)));
      break;

    case "blockquote":
      for (const child of node.content ?? []) {
        blocks.push({ kind: "quote", runs: runsFrom(child.content) });
      }
      break;

    case "codeBlock":
      blocks.push({
        kind: "code",
        text: (node.content ?? []).map((c) => c.text ?? "").join(""),
      });
      break;

    case "bulletList":
    case "orderedList":
    case "taskList":
      for (const item of node.content ?? []) {
        await walk(item, blocks, depth, node.type === "orderedList");
      }
      break;

    case "listItem":
    case "taskItem": {
      const [first, ...rest] = node.content ?? [];
      blocks.push({
        kind: "listItem",
        ordered,
        depth,
        checked: node.type === "taskItem" ? Boolean(node.attrs?.checked) : undefined,
        runs: runsFrom(first?.content),
      });
      blocks.push(...(await inlineMathBlocks(first?.content)));
      // Nested lists live inside the item, one level deeper.
      for (const child of rest) await walk(child, blocks, depth + 1, ordered);
      break;
    }

    case "horizontalRule":
      blocks.push({ kind: "divider" });
      break;

    case "mathBlock": {
      const latex = String(node.attrs?.latex ?? "");
      const rendered = await mathToPng(latex, true);
      if (rendered) {
        blocks.push({
          kind: "image",
          data: rendered.png,
          width: rendered.width,
          height: rendered.height,
          alt: latex,
        });
      } else {
        // A formula MathJax cannot render still has to appear, or the export
        // silently loses content. Its source is better than nothing.
        blocks.push({ kind: "code", text: latex });
      }
      break;
    }

    case "image": {
      const source = String(node.attrs?.src ?? "");
      const data = await fetchAsDataUrl(attachmentUrl(source));
      if (data) {
        blocks.push({
          kind: "image",
          data,
          width: 0,
          height: 0,
          alt: String(node.attrs?.alt ?? ""),
        });
      }
      break;
    }

    case "table": {
      const rows: string[][] = [];
      let headerRow = false;
      for (const row of node.content ?? []) {
        const cells: string[] = [];
        for (const cell of row.content ?? []) {
          if (cell.type === "tableHeader") headerRow = true;
          cells.push(
            (cell.content ?? [])
              .map((p) => runsFrom(p.content).map((r) => r.text).join(""))
              .join(" "),
          );
        }
        rows.push(cells);
      }
      blocks.push({ kind: "table", rows, headerRow });
      break;
    }

    default:
      for (const child of node.content ?? []) await walk(child, blocks, depth, ordered);
  }
}

/** Read an attachment through the sutra:// scheme and base64 it for Rust. */
async function fetchAsDataUrl(url: string): Promise<string | null> {
  try {
    const response = await fetch(url);
    if (!response.ok) return null;
    const blob = await response.blob();
    return await new Promise((resolve) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => resolve(null);
      reader.readAsDataURL(blob);
    });
  } catch {
    return null;
  }
}

export async function buildDocument(
  title: string,
  doc: JSONContent,
  citationKeys: string[],
): Promise<ExportDocument> {
  const blocks: Block[] = [];
  await walk(doc, blocks);

  const references = resolved(citationKeys).map((r) =>
    [r.creators, r.year && `(${r.year})`, r.title, r.doi && `doi:${r.doi}`]
      .filter(Boolean)
      .join(" "),
  );

  return { title, blocks, references };
}
