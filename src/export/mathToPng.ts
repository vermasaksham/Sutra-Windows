import { mathjax } from "mathjax-full/js/mathjax.js";
import { TeX } from "mathjax-full/js/input/tex.js";
import { SVG } from "mathjax-full/js/output/svg.js";
import { browserAdaptor } from "mathjax-full/js/adaptors/browserAdaptor.js";
import { RegisterHTMLHandler } from "mathjax-full/js/handlers/html.js";
import { AllPackages } from "mathjax-full/js/input/tex/AllPackages.js";

/**
 * Rendering formulas to images, for export.
 *
 * A note on why this is not KaTeX. The editor renders maths with KaTeX, and
 * the intent was to reuse it here — but KaTeX only emits HTML or MathML, never
 * an image, so there is nothing to embed in a .docx. MathJax renders the same
 * LaTeX, including mhchem's `\ce{}`, straight to SVG whose glyphs are `<path>`
 * elements rather than text in a font. That last part is what makes this work:
 * a path-only SVG rasterises on a canvas with no font loading and no tainting.
 *
 * The cost, stated plainly: exported formulas are rendered by a different
 * engine from the on-screen ones. Both are correct TeX; the metrics and glyph
 * shapes differ slightly, so an exported equation will not be pixel-identical
 * to the editor.
 *
 * MathJax is loaded only when an export runs.
 */

let converter: ReturnType<typeof buildConverter> | null = null;

function buildConverter() {
  RegisterHTMLHandler(browserAdaptor());
  return mathjax.document("", {
    // AllPackages is what brings in mhchem; without it `\ce{}` renders as the
    // letters c and e, which is exactly the failure this project has already
    // hit once.
    InputJax: new TeX({ packages: AllPackages }),
    OutputJax: new SVG({ fontCache: "local" }),
  });
}

/** Pixels per `ex` when rasterising. Larger than screen size so the image is
 *  still crisp when Word or a printer scales it. */
const PIXELS_PER_EX = 16;

export type RenderedMath = { png: string; width: number; height: number };

/**
 * Render LaTeX to a PNG data URL.
 *
 * Returns null rather than throwing on bad input: one malformed formula should
 * degrade to its source text in the export, not abort the whole document.
 */
export async function mathToPng(
  latex: string,
  display: boolean,
): Promise<RenderedMath | null> {
  try {
    converter ??= buildConverter();
    const node = converter.convert(latex, { display });
    const svgElement = (node as unknown as HTMLElement).querySelector("svg");
    if (!svgElement) return null;

    // MathJax sizes its SVG in `ex`, which a canvas cannot use. Convert to
    // pixels and pin them on the element, or the drawn image comes out 0×0 in
    // some browsers.
    const exWidth = parseFloat(svgElement.getAttribute("width") ?? "0");
    const exHeight = parseFloat(svgElement.getAttribute("height") ?? "0");
    const width = Math.max(Math.ceil(exWidth * PIXELS_PER_EX), 1);
    const height = Math.max(Math.ceil(exHeight * PIXELS_PER_EX), 1);
    svgElement.setAttribute("width", `${width}`);
    svgElement.setAttribute("height", `${height}`);
    // Word renders a transparent PNG on white, but a dark theme's colours would
    // be baked in otherwise; force black so the export is print-ready.
    svgElement.setAttribute("color", "#000000");

    const svg = new XMLSerializer().serializeToString(svgElement);
    const png = await rasterise(svg, width, height);
    return png ? { png, width, height } : null;
  } catch {
    return null;
  }
}

function rasterise(
  svg: string,
  width: number,
  height: number,
): Promise<string | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d");
      if (!context) return resolve(null);
      // White rather than transparent: a formula dropped into a Word document
      // with a coloured background should still be legible.
      context.fillStyle = "#ffffff";
      context.fillRect(0, 0, width, height);
      context.drawImage(image, 0, 0, width, height);
      try {
        resolve(canvas.toDataURL("image/png"));
      } catch {
        resolve(null);
      }
    };
    image.onerror = () => resolve(null);
    // Base64 rather than a blob URL: no object to revoke, and no chance of the
    // URL outliving the export.
    image.src = `data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(svg)))}`;
  });
}
