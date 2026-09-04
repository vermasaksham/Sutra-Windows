/**
 * What to call the modifier key.
 *
 * Windows is the primary target, so the default is Ctrl and macOS is the
 * special case — the opposite of how most web code is written, and the reason
 * this exists at all rather than a hardcoded ⌘ in every tooltip.
 *
 * `navigator.platform` is deprecated and lies in some webviews; the user-agent
 * string is what Tauri's WebView2 and WKWebView both report honestly.
 */
export const isMac = /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);

/** "Ctrl" on Windows and Linux, "⌘" on macOS. */
export const MOD = isMac ? "⌘" : "Ctrl";

/** "Shift" everywhere, but "⇧" reads better beside a ⌘. */
export const SHIFT = isMac ? "⇧" : "Shift";

/**
 * A shortcut written the way the platform writes them: `⌘⇧F` on macOS, where
 * modifiers are glyphs run together, and `Ctrl+Shift+F` on Windows, where they
 * are words joined by plus signs.
 */
export function shortcut(...parts: string[]): string {
  return isMac ? parts.join("") : parts.join("+");
}
