/**
 * What the theme is, with none of the machinery that applies it.
 *
 * Separate from theme.ts on purpose. That module reads `localStorage`, writes
 * to `document.documentElement` and subscribes to a media query at import
 * time, so importing it outside a browser fails on the first line that touches
 * `window`. Everything here is a pure function over values, which is what
 * makes the decisions below testable without standing up a DOM for them — the
 * same split as `choose()` on the Rust side.
 */

export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

/**
 * The palettes on offer.
 *
 * The order is the order Settings shows them in, and `note` is what appears
 * under the name there — a row of four swatches shows what the colours are,
 * but not what they are for.
 *
 * Adding one means adding it here, adding its two blocks to styles/tokens.css,
 * and adding its id to the pre-paint script in index.html. A test pins the
 * third of those to this list, because it is the one that is easy to forget
 * and produces a bug nobody reports.
 */
export const PALETTES = [
  {
    id: "sutra",
    label: "Sutra",
    note: "Persimmon and teal on warm paper. The default.",
  },
  {
    id: "indigo",
    label: "Indigo",
    note: "The original brief — indigo links, saffron highlights.",
  },
  {
    id: "slate",
    label: "Slate",
    note: "Cool neutrals and one quiet blue. Nearly monochrome.",
  },
  {
    id: "contrast",
    label: "Contrast",
    note: "Pure grounds and heavy edges, for a lit room or a projector.",
  },
] as const;

export type PaletteId = (typeof PALETTES)[number]["id"];

export const DEFAULT_PALETTE: PaletteId = "sutra";

export const THEME_KEY = "sutra.theme";
export const PALETTE_KEY = "sutra.palette";

/**
 * Narrow whatever came out of storage to a palette we can actually render.
 *
 * A value written by a newer build, a half-finished write, or somebody poking
 * at localStorage must all land on the default rather than on an attribute
 * that matches no rule in tokens.css — which would leave the app painted in
 * the bare `:root` fallback with no way back except clearing storage.
 */
export function readPalette(stored: string | null): PaletteId {
  const known = PALETTES.find((palette) => palette.id === stored);
  return known ? known.id : DEFAULT_PALETTE;
}

/** The same narrowing for the light/dark preference. */
export function readPreference(stored: string | null): ThemePreference {
  return stored === "light" || stored === "dark" || stored === "system"
    ? stored
    : "system";
}

/** Which of the two concrete themes a preference means right now. */
export function resolveTheme(
  preference: ThemePreference,
  prefersDark: boolean,
): ResolvedTheme {
  if (preference !== "system") return preference;
  return prefersDark ? "dark" : "light";
}
