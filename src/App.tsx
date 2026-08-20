import Editor from "./editor/Editor";
import ThemeToggle from "./components/ThemeToggle";

/**
 * Phase 2 shell: a top bar and the writing surface.
 *
 * There is no sidebar, no breadcrumb, and no page tree — those arrive in
 * Phase 4 with the SQLite index behind them. Nothing here persists.
 */
export default function App() {
  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-10 border-b border-border bg-canvas">
        <div className="mx-auto flex h-12 max-w-content items-center justify-between px-6">
          <span className="text-sm font-semibold tracking-tight text-ink-soft">
            Sutra
          </span>
          <ThemeToggle />
        </div>
      </header>

      <main className="mx-auto max-w-content px-6 py-12">
        <Editor />
      </main>
    </div>
  );
}
