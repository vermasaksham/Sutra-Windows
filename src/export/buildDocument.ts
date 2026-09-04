import type { JSONContent } from "@tiptap/core";
import { positionOf, resolved } from "../editor/citation/citationStore";
import { emphasisRuns, marker } from "../notes/citationStyle";
import { mathToImage } from "./mathToImage";
import { encodeSvg, rasterise } from "./rasterise";
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
  | {
      kind: "listItem";
      ordered: boolean;
      depth: number;
      checked?: boolean;
      runs: Run[];
    }
  | { kind: "divider" }
  | { kind: "table"; rows: string[][]; headerRow: boolean }
  /**
   * A picture. `data` is always a PNG data URL — Rust embeds that, and Word
   * requires it even when a vector copy is supplied. `svg` is that vector copy,
   * present for formulas and for attachments that were SVG to begin with.
   */
  | {
      kind: "image";
      data: string;
      svg?: string;
      width: number;
      height: number;
      alt: string;
    };

export type ExportDocument = {
  title: string;
  blocks: Block[];
  /** Formatted bibliography lines, if the note cites anything. */
  references: Run[][];
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
      const ref = String(node.attrs?.ref ?? "");
      const [cited] = resolved([ref]);
      // An unresolved citation exports as its ref rather than vanishing: the
      // reference is in the file, and a silently dropped one is worse.
      // Same marker as on screen, from the same function, so an exported
      // document numbers its citations exactly as the editor showed them.
      runs.push({
        text: cited
          ? marker(cited.styled, cited.label, positionOf(ref))
          : `(${ref})`,
      });
    } else if (node.type === "hardBreak") {
      runs.push({ text: "\n" });
    }
  }
  return runs;
}

/** Inline maths becomes its own image block, since a run cannot hold a picture. */
async function inlineMathBlocks(
  nodes: JSONContent[] | undefined,
): Promise<Block[]> {
  const blocks: Block[] = [];
  for (const node of nodes ?? []) {
    if (node.type === "mathInline") {
      const latex = String(node.attrs?.latex ?? "");
      const rendered = await mathToImage(latex, false);
      if (rendered) blocks.push(imageBlock(rendered, latex));
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
        checked:
          node.type === "taskItem" ? Boolean(node.attrs?.checked) : undefined,
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
      const rendered = await mathToImage(latex, true);
      if (rendered) {
        blocks.push(imageBlock(rendered, latex));
      } else {
        // A formula MathJax cannot render still has to appear, or the export
        // silently loses content. Its source is better than nothing.
        blocks.push({ kind: "code", text: latex });
      }
      break;
    }

    case "image": {
      const source = String(node.attrs?.src ?? "");
      const picture = await attachmentForExport(attachmentUrl(source));
      if (picture) {
        blocks.push({
          kind: "image",
          ...picture,
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
              .map((p) =>
                runsFrom(p.content)
                  .map((r) => r.text)
                  .join(""),
              )
              .join(" "),
          );
        }
        rows.push(cells);
      }
      blocks.push({ kind: "table", rows, headerRow });
      break;
    }

    default:
      for (const child of node.content ?? [])
        await walk(child, blocks, depth, ordered);
  }
}

function imageBlock(
  rendered: { svg: string; png: string; width: number; height: number },
  alt: string,
): Block {
  return {
    kind: "image",
    data: rendered.png,
    svg: `data:image/svg+xml;base64,${encodeSvg(rendered.svg)}`,
    width: rendered.width,
    height: rendered.height,
    alt,
  };
}

/**
 * Read an attachment through the sutra:// scheme and put it in the shape Rust
 * expects: a PNG, plus the original vector if it was one.
 *
 * The conversion to PNG is not optional. Rust hands the bytes to docx-rs, which
 * panics on anything it cannot decode as an image, and the app is built to
 * abort on panic — so a stray file format would take the window down rather
 * than fail one export. Normalising here means Rust only ever sees a PNG.
 */
async function attachmentForExport(
  url: string,
): Promise<{ data: string; svg?: string } | null> {
  try {
    const response = await fetch(url);
    if (!response.ok) return null;
    const blob = await response.blob();

    if (blob.type === "image/svg+xml") {
      const svg = await blob.text();
      const { width, height } = await intrinsicSize(url);
      const png = await rasterise(svg, width, height);
      return png
        ? { data: png, svg: `data:image/svg+xml;base64,${encodeSvg(svg)}` }
        : null;
    }

    const png = await toPng(url);
    return png ? { data: png } : null;
  } catch {
    return null;
  }
}

/** Draw an image through a canvas so whatever came in leaves as a PNG. */
function toPng(url: string): Promise<string | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = image.naturalWidth || FALLBACK_SIZE.width;
      canvas.height = image.naturalHeight || FALLBACK_SIZE.height;
      const context = canvas.getContext("2d");
      if (!context) return resolve(null);
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      try {
        resolve(canvas.toDataURL("image/png"));
      } catch {
        resolve(null);
      }
    };
    image.onerror = () => resolve(null);
    image.src = url;
  });
}

/** An SVG with no width and height reports nothing; pick something printable. */
const FALLBACK_SIZE = { width: 600, height: 400 };

function intrinsicSize(
  url: string,
): Promise<{ width: number; height: number }> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () =>
      resolve({
        width: image.naturalWidth || FALLBACK_SIZE.width,
        height: image.naturalHeight || FALLBACK_SIZE.height,
      });
    image.onerror = () => resolve(FALLBACK_SIZE);
    image.src = url;
  });
}

export async function buildDocument(
  title: string,
  doc: JSONContent,
  citedRefs: string[],
): Promise<ExportDocument> {
  const blocks: Block[] = [];
  await walk(doc, blocks);

  // Split each entry into runs so italics survive into Word.
  //
  // A styled bibliography comes back from Zotero as HTML and is flattened to
  // markdown, which is how a journal name reaches here as *Nature Energy*. Sent
  // as one flat string, Word showed the asterisks — the one place in the app
  // where markup leaked into a finished document. `emphasisRuns` is the same
  // function the bibliography panel uses, so the two cannot drift.
  const references = resolved(citedRefs).map((cited) =>
    emphasisRuns(cited.detail).map((run) => ({
      text: run.text,
      italic: run.emphasis,
    })),
  );

  return { title, blocks, references };
}
