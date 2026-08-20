import { useSyncExternalStore } from "react";

/**
 * Theme handling.
 *
 * The whole mechanism is one attribute — <html data-theme="light|dark"> — and
 * the CSS in styles/tokens.css does the rest. This module only decides which
 * of the two values to write, and remembers the user's preference.
 *
 * "system" is a *preference*, not a theme: it resolves to light or dark via
 * the OS setting, so `data-theme` is always a concrete value and the CSS never
 * needs a media query.
 */

export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "sutra.theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";

function readPreference(): ThemePreference {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark" || stored === "system") {
      return stored;
    }
  } catch {
    // Private mode / storage disabled — fall through to the default.
  }
  return "system";
}

function resolve(preference: ThemePreference): ResolvedTheme {
  if (preference !== "system") return preference;
  return window.matchMedia(DARK_QUERY).matches ? "dark" : "light";
}

let preference: ThemePreference = readPreference();
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

function apply() {
  document.documentElement.dataset.theme = resolve(preference);
  emit();
}

export function setThemePreference(next: ThemePreference) {
  preference = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Not fatal: the theme still applies for this session.
  }
  apply();
}

// When the preference is "system", follow the OS if it changes mid-session.
window.matchMedia(DARK_QUERY).addEventListener("change", () => {
  if (preference === "system") apply();
});

// The inline script in index.html has already set data-theme before first
// paint; this re-runs it so the module and the DOM start out agreeing.
apply();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useTheme(): {
  preference: ThemePreference;
  resolved: ResolvedTheme;
  setPreference: (next: ThemePreference) => void;
} {
  const current = useSyncExternalStore(
    subscribe,
    () => preference,
    () => "system" as ThemePreference,
  );
  return {
    preference: current,
    resolved: resolve(current),
    setPreference: setThemePreference,
  };
}
