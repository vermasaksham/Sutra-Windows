import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * How the app is set in type.
 *
 * Rust owns the values; this is the mirror the components read, and the one
 * place that turns them into CSS. Applied as custom properties on the root
 * element so the whole app repaints from one write — the same mechanism as the
 * theme, for the same reason.
 */

export type CustomFont = { family: string; file: string };

export type Typography = {
  /** CSS family for the note body. Empty means the app's own font. */
  reading: string;
  /** CSS family for the surrounding interface. */
  interface: string;
  size: number;
  leading: number;
  width: number;
  fonts: CustomFont[];
};

export const DEFAULT_TYPOGRAPHY: Typography = {
  reading: "",
  interface: "",
  size: 16,
  leading: 1.65,
  width: 700,
  fonts: [],
};

/**
 * Families worth offering by name.
 *
 * Everything here either ships with the app or ships with the operating
 * system, so the list is what a person can pick and actually get. Anything
 * else can still be typed in, and anything at all can be imported — this is a
 * shortcut, not a limit.
 */
export const FONT_CHOICES: ReadonlyArray<{
  id: string;
  label: string;
  note: string;
}> = [
  {
    id: "",
    label: "Sutra's own (Source Sans 3)",
    note: "Bundled — always available",
  },
  { id: "Segoe UI", label: "Segoe UI", note: "Windows" },
  { id: "Calibri", label: "Calibri", note: "Windows" },
  { id: "Cambria", label: "Cambria", note: "Windows · serif" },
  { id: "Constantia", label: "Constantia", note: "Windows · serif" },
  { id: "Georgia", label: "Georgia", note: "Serif, everywhere" },
  {
    id: "Times New Roman",
    label: "Times New Roman",
    note: "Serif, everywhere",
  },
  { id: "Arial", label: "Arial", note: "Everywhere" },
  { id: "Verdana", label: "Verdana", note: "Wide, very legible" },
  { id: "Consolas", label: "Consolas", note: "Monospaced" },
];

/**
 * The stack a chosen family sits at the front of.
 *
 * A family that is not installed has to fall through to something rather than
 * to nothing, which is also what makes it safe to accept a name typed by hand
 * or one that came from a machine where the font existed.
 */
function stack(family: string, fallback: string): string {
  const clean = family.trim();
  if (clean === "") return fallback;
  return `"${clean}", ${fallback}`;
}

const BODY_FALLBACK =
  '"Source Sans 3", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif';

let current: Typography = DEFAULT_TYPOGRAPHY;
const listeners = new Set<() => void>();

/** The <style> element holding @font-face rules for imported fonts. */
let sheet: HTMLStyleElement | null = null;

function applyFontFaces(fonts: CustomFont[]) {
  if (typeof document === "undefined") return;
  if (!sheet) {
    sheet = document.createElement("style");
    sheet.dataset.sutra = "fonts";
    document.head.append(sheet);
  }
  // The file is served by Rust from the app's own directory over the sutra://
  // scheme, so no filesystem path appears here — the same rule as attachments.
  sheet.textContent = fonts
    .map(
      (font) =>
        `@font-face{font-family:"${font.family}";` +
        `src:url("sutra://localhost/fonts/${encodeURIComponent(font.file)}");` +
        `font-display:swap;}`,
    )
    .join("\n");
}

function apply() {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  applyFontFaces(current.fonts);
  root.style.setProperty(
    "--font-body",
    stack(current.interface, BODY_FALLBACK),
  );
  root.style.setProperty(
    "--font-reading",
    stack(current.reading, "var(--font-body)"),
  );
  root.style.setProperty("--text-size-body", `${current.size}px`);
  root.style.setProperty("--leading-body", `${current.leading}`);
  root.style.setProperty("--content-width", `${current.width}px`);
  for (const listener of listeners) listener();
}

export function setTypography(next: Typography) {
  current = next;
  apply();
}

export function currentTypography(): Typography {
  return current;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useTypography(): Typography {
  return useSyncExternalStore(
    subscribe,
    currentTypography,
    () => DEFAULT_TYPOGRAPHY,
  );
}

export const typographyApi = {
  get: () => invoke<Typography>("typography"),
  set: (typography: Typography) =>
    invoke<Typography>("set_typography", { typography }),
  /** Opens a native picker. Resolves null if it was cancelled. */
  importFont: (family: string) =>
    invoke<Typography | null>("import_font", { family }),
  removeFont: (family: string) => invoke<Typography>("remove_font", { family }),
};
