import { mathjax } from "mathjax-full/js/mathjax.js";
import { TeX } from "mathjax-full/js/input/tex.js";
import { SVG } from "mathjax-full/js/output/svg.js";
import { browserAdaptor } from "mathjax-full/js/adaptors/browserAdaptor.js";
import { RegisterHTMLHandler } from "mathjax-full/js/handlers/html.js";
import { AllPackages } from "mathjax-full/js/input/tex/AllPackages.js";
import { rasterise } from "./rasterise";

/**
 * Rendering formulas for export.
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
 *
 * Two forms come back, because the two export routes want different things.
 * The SVG is the real output — it stays sharp at any zoom and prints at the
 * printer's resolution, which is what a formula in a paper needs. The PNG is
 * the compatibility copy: Word's SVG support is an extension on a picture that
 * must still name a raster image, so a .docx carries both.
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

/** Pixels per `ex`. Sets the raster copy's resolution, and the size Word lays
 *  the picture out at. Larger than screen size so the fallback is still crisp
 *  where it is the one being shown. */
const PIXELS_PER_EX = 16;

export type RenderedMath = {
  /** SVG markup. What Word and any modern reader actually draws. */
  svg: string;
  /** PNG data URL, for readers that do not understand the SVG extension. */
  png: string;
  width: number;
  height: number;
};

/**
 * Render LaTeX to an SVG, and to a PNG of the same thing.
 *
 * Returns null rather than throwing on bad input: one malformed formula should
 * degrade to its source text in the export, not abort the whole document.
 */
export async function mathToImage(
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

    const svg = standalone(new XMLSerializer().serializeToString(svgElement));
    const png = await rasterise(svg, width, height);
    // No PNG means no picture at all: the .docx cannot hold a lone SVG.
    return png ? { svg, png, width, height } : null;
  } catch {
    return null;
  }
}

/**
 * Make a serialised `<svg>` well-formed on its own.
 *
 * MathJax sets `xmlns` as an ordinary attribute, and the serialiser adds the
 * namespace declaration too — so the root element comes out with `xmlns`
 * written twice. A duplicate attribute is not well-formed XML, and a .docx
 * carries the SVG as its own part, which Word parses strictly and would
 * reject. So: strip every namespace declaration off the root, put back exactly
 * one of each.
 *
 * Only the root tag is touched. No attribute value on it can contain a literal
 * `>` — the serialiser escapes those — so finding the tag's end is safe.
 */
function standalone(svg: string): string {
  const end = svg.indexOf(">");
  if (end === -1) return svg;
  const root = svg.slice(0, end).replace(/\s+xmlns(:xlink)?="[^"]*"/g, "");
  return `${root} xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"${svg.slice(end)}`;
}
