/**
 * Turning SVG markup into a PNG data URL, on a canvas.
 *
 * Export needs this even though the vector is what we would rather ship: Word's
 * SVG support is an extension hanging off a picture that names a PNG, so every
 * vector picture has to be accompanied by a raster one. Readers that do not
 * know the extension — LibreOffice, Word 2013, Google Docs — show the PNG.
 */
export function rasterise(
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
      // White rather than transparent: a picture dropped into a Word document
      // with a coloured background should still be legible.
      context.fillStyle = "#ffffff";
      context.fillRect(0, 0, width, height);
      context.drawImage(image, 0, 0, width, height);
      try {
        resolve(canvas.toDataURL("image/png"));
      } catch {
        // A tainted canvas. Only reachable if the SVG pulled in something
        // cross-origin, which ours never do — but throwing here would abort a
        // whole export over one picture.
        resolve(null);
      }
    };
    image.onerror = () => resolve(null);
    // Base64 rather than a blob URL: no object to revoke, and no chance of the
    // URL outliving the export.
    image.src = `data:image/svg+xml;base64,${encodeSvg(svg)}`;
  });
}

/**
 * base64 of the UTF-8 bytes. `btoa` alone throws on anything non-Latin-1, and
 * spreading the whole byte array into `fromCharCode` overflows the argument
 * stack on a long formula — hence the chunking.
 */
export function encodeSvg(svg: string): string {
  const bytes = new TextEncoder().encode(svg);
  const CHUNK = 0x8000;
  let binary = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}
