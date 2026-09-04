import { useSyncExternalStore } from "react";
import {
  DEFAULT_PALETTE,
  PALETTE_KEY,
  THEME_KEY,
  readPalette,
  readPreference,
  resolveTheme,
  type PaletteId,
  type ResolvedTheme,
  type ThemePreference,
} from "./palettes";

/**
 * Applying the theme, and remembering it.
 *
 * Two independent choices, two attributes:
 *
 *   <html data-palette="sutra|indigo|slate|contrast" data-theme="light|dark">
 *
 * The *palette* is which colours; the *theme* is light or dark. tokens.css
 * defines every palette in both, so neither choice constrains the other. This
 * module only decides which two values to write, and remembers them; the
 * decisions themselves live in palettes.ts, where they can be tested.
 *
 * "system" is a *preference*, not a theme: it resolves to light or dark via
 * the OS setting, so `data-theme` is always a concrete value and the CSS never
 * needs a media query.
 *
 * Both preferences live in localStorage rather than in sutra.json with the
 * other settings, deliberately. The inline script in index.html has to resolve
 * them *before the first paint* or the window flashes the wrong colours on
 * every launch, and reading sutra.json means an async call over the Tauri
 * bridge, which by definition cannot happen before the first paint. They are
 * also per-machine display preferences rather than anything about the vault,
 * so nothing about them belongs beside the notes.
 */

export {
  PALETTES,
  DEFAULT_PALETTE,
  type PaletteId,
  type ResolvedTheme,
  type ThemePreference,
} from "./palettes";

const DARK_QUERY = "(prefers-color-scheme: dark)";

function load(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    // Private mode / storage disabled — the caller's default applies.
    return null;
  }
}

function save(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not fatal: the choice still applies for this session.
  }
}

function prefersDark(): boolean {
  return window.matchMedia(DARK_QUERY).matches;
}

let preference: ThemePreference = readPreference(load(THEME_KEY));
let palette: PaletteId = readPalette(load(PALETTE_KEY));

const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

function apply() {
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(preference, prefersDark());
  root.dataset.palette = palette;
  emit();
}

export function setThemePreference(next: ThemePreference) {
  preference = next;
  save(THEME_KEY, next);
  apply();
}

export function setPalette(next: PaletteId) {
  palette = next;
  save(PALETTE_KEY, next);
  apply();
}

// When the preference is "system", follow the OS if it changes mid-session.
window.matchMedia(DARK_QUERY).addEventListener("change", () => {
  if (preference === "system") apply();
});

// The inline script in index.html has already set both attributes before first
// paint; this re-runs it so the module and the DOM start out agreeing.
apply();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * One snapshot object carrying both choices.
 *
 * `useSyncExternalStore` compares snapshots by identity, so this has to be a
 * cached object rather than a fresh literal per call: a new object every time
 * would be a new value every time, and the component would re-render forever.
 */
type Snapshot = { preference: ThemePreference; palette: PaletteId };

let snapshot: Snapshot = { preference, palette };

function getSnapshot(): Snapshot {
  if (snapshot.preference !== preference || snapshot.palette !== palette) {
    snapshot = { preference, palette };
  }
  return snapshot;
}

const SERVER_SNAPSHOT: Snapshot = {
  preference: "system",
  palette: DEFAULT_PALETTE,
};

export function useTheme(): {
  preference: ThemePreference;
  resolved: ResolvedTheme;
  palette: PaletteId;
  setPreference: (next: ThemePreference) => void;
  setPalette: (next: PaletteId) => void;
} {
  const current = useSyncExternalStore(
    subscribe,
    getSnapshot,
    () => SERVER_SNAPSHOT,
  );
  return {
    preference: current.preference,
    resolved: resolveTheme(current.preference, prefersDark()),
    palette: current.palette,
    setPreference: setThemePreference,
    setPalette,
  };
}
