import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ThemeToggle from "./components/ThemeToggle";

/**
 * Phase 1 shell.
 *
 * This screen exists to prove four things and nothing more: the window opens,
 * Source Sans 3 loads, both themes switch off a single attribute, and the
 * React <-> Rust bridge is wired. It gets replaced by the editor in Phase 2.
 */

const SWATCHES = [
  { token: "--canvas", label: "canvas" },
  { token: "--surface", label: "surface" },
  { token: "--border", label: "border" },
  { token: "--text-primary", label: "text-primary" },
  { token: "--text-secondary", label: "text-secondary" },
  { token: "--text-muted", label: "text-muted" },
  { token: "--accent", label: "accent" },
  { token: "--accent-bg", label: "accent-bg" },
  { token: "--highlight", label: "highlight" },
  { token: "--highlight-bg", label: "highlight-bg" },
] as const;

export default function App() {
  const [backend, setBackend] = useState<string | null>(null);
  const [backendError, setBackendError] = useState<string | null>(null);

  useEffect(() => {
    // `invoke` is the whole frontend/backend boundary. It returns a Promise
    // that resolves with whatever the Rust command returned, or rejects with
    // the Err side of its Result. In a plain `vite dev` browser tab there is
    // no Tauri host, so this rejects — that is expected, not a bug.
    invoke<string>("app_version")
      .then(setBackend)
      .catch(() => setBackendError("not running inside Tauri"));
  }, []);

  return (
    <main className="min-h-screen px-6 py-16">
      <div className="mx-auto flex max-w-content flex-col gap-10">
        <header className="flex flex-col gap-3">
          <p className="text-sm tracking-wide text-ink-muted">सूत्र · thread</p>
          <h1 className="text-4xl font-semibold tracking-tight text-ink">
            Sutra
          </h1>
          <p className="selectable text-ink-soft">
            Local-first notes for materials chemistry. Markdown files on disk
            are the source of truth. Everything else is derived.
          </p>
        </header>

        <section className="flex flex-col items-start gap-3">
          <h2 className="text-sm font-semibold text-ink-soft">Theme</h2>
          <ThemeToggle />
        </section>

        <section className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-ink-soft">Typeface</h2>
          <p className="selectable">
            Source Sans 3 draws real subscripts, which is why formulas like
            Sb<sub>2</sub>Se<sub>3</sub> and Cu<sub>2</sub>ZnSnS<sub>4</sub>{" "}
            stay legible at body size. Links render in{" "}
            <a href="#" className="text-accent underline underline-offset-2">
              indigo
            </a>
            , and{" "}
            <mark className="rounded-sm bg-highlight-bg px-1 text-highlight">
              saffron is reserved for highlights
            </mark>{"."}
          </p>
        </section>

        <section className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-ink-soft">Palette</h2>
          <ul className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {SWATCHES.map((swatch) => (
              <li
                key={swatch.token}
                className="flex items-center gap-3 rounded-lg border border-border bg-surface p-2"
              >
                <span
                  aria-hidden
                  className="size-8 shrink-0 rounded-md border border-border"
                  style={{ backgroundColor: `var(${swatch.token})` }}
                />
                <code className="font-mono text-xs text-ink-soft">
                  {swatch.label}
                </code>
              </li>
            ))}
          </ul>
        </section>

        <footer className="border-t border-border pt-4 font-mono text-xs text-ink-muted">
          backend: {backend ?? backendError ?? "…"}
        </footer>
      </div>
    </main>
  );
}
