import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * Turn a vault-relative attachment reference into something the webview can
 * load.
 *
 * The note stores `attachments/01H…_diagram.png` and nothing else — that is
 * what keeps a vault portable, since moving the folder cannot break it. The
 * webview cannot open a relative path, so it asks our own `sutra://` scheme
 * for it and Rust resolves the reference against the open vault.
 *
 * `convertFileSrc` only assembles the URL for the platform's scheme rules
 * (`http://sutra.localhost/…` on Windows, `sutra://localhost/…` elsewhere). It
 * is not being used for its usual purpose of converting a filesystem path,
 * because we never have one here.
 *
 * An absolute URL is passed through untouched: a note may legitimately point
 * at something remote, and rewriting it would break the link.
 */
export function attachmentUrl(reference: string): string {
  if (/^[a-z][a-z0-9+.-]*:/i.test(reference) || reference.startsWith("//")) {
    return reference;
  }
  try {
    return convertFileSrc(reference, "sutra");
  } catch {
    // No Tauri host — `npm run dev` in a plain browser. The image will not
    // load, which is the honest outcome there.
    return reference;
  }
}
