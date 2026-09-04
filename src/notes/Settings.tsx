import ReferenceSettingsPanel from "./ReferenceSettings";
import TypographySettings from "./TypographySettings";
import {
  PALETTES,
  useTheme,
  type PaletteId,
  type ResolvedTheme,
  type ThemePreference,
} from "../theme/theme";

const MODES: ReadonlyArray<{ value: ThemePreference; label: string }> = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "System" },
];

/**
 * Settings.
 *
 * Appearance lives here, and assistance is a door rather than a section — it
 * is a page of its own about what leaves the machine, and folding that into a
 * list of display preferences would bury the one setting that deserves
 * reading before it is changed.
 */
export default function SettingsDialog({
  onClose,
  onOpenAiSettings,
  aiEnabled,
  onReport,
}: {
  onClose: () => void;
  onOpenAiSettings: () => void;
  aiEnabled: boolean;
  onReport: (message: string, cause: unknown) => void;
}) {
  const { preference, resolved, palette, setPreference, setPalette } =
    useTheme();

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      className="sutra-no-print fixed inset-0 z-50 grid place-items-center bg-canvas/70 px-6 backdrop-blur-sm"
      onMouseDown={(e) => {
        // Only a press that both starts and ends on the backdrop closes it, so
        // a selection dragged out of the dialog does not dismiss it.
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[86vh] w-full max-w-xl flex-col gap-5 overflow-y-auto rounded-xl border border-border bg-surface p-5 shadow-pane">
        <div className="flex items-start justify-between gap-4">
          <h2 className="text-lg font-semibold text-ink">Settings</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close settings"
            className="rounded-lg px-2 py-0.5 text-ink-muted transition-colors hover:text-accent"
          >
            ✕
          </button>
        </div>

        <section className="flex flex-col gap-3">
          <h3 className="text-xs font-semibold tracking-wide text-ink-soft uppercase">
            Appearance
          </h3>

          <div className="flex flex-wrap items-center gap-3">
            <span className="text-sm text-ink-soft">Mode</span>
            <div
              role="radiogroup"
              aria-label="Light or dark"
              className="inline-flex rounded-md border border-border bg-canvas p-px"
            >
              {MODES.map((mode) => {
                const active = preference === mode.value;
                return (
                  <button
                    key={mode.value}
                    type="button"
                    role="radio"
                    aria-checked={active}
                    onClick={() => setPreference(mode.value)}
                    className={[
                      "rounded-[5px] px-2.5 py-1 text-xs transition-colors",
                      active
                        ? "bg-accent-bg font-medium text-accent"
                        : "text-ink-soft hover:text-ink",
                    ].join(" ")}
                  >
                    {mode.label}
                  </button>
                );
              })}
            </div>
            {preference === "system" && (
              <span className="text-xs text-ink-muted">
                Following the system — {resolved} right now.
              </span>
            )}
          </div>

          <div
            role="radiogroup"
            aria-label="Palette"
            className="grid gap-2 sm:grid-cols-2"
          >
            {PALETTES.map((option) => (
              <PaletteCard
                key={option.id}
                id={option.id}
                label={option.label}
                note={option.note}
                mode={resolved}
                chosen={palette === option.id}
                onChoose={() => setPalette(option.id)}
              />
            ))}
          </div>

          <p className="text-xs text-ink-muted">
            The palette and the mode are separate: every palette is drawn for
            both light and dark, so switching one never undoes the other.
          </p>
        </section>

        <section className="flex flex-col gap-2 border-t border-border pt-4">
          <h3 className="text-xs font-semibold tracking-wide text-ink-soft uppercase">
            Typography
          </h3>
          <TypographySettings onReport={onReport} />
        </section>

        <section className="flex flex-col gap-2 border-t border-border pt-4">
          <h3 className="text-xs font-semibold tracking-wide text-ink-soft uppercase">
            References
          </h3>
          <ReferenceSettingsPanel onReport={onReport} />
        </section>

        <section className="flex flex-col gap-2 border-t border-border pt-4">
          <h3 className="text-xs font-semibold tracking-wide text-ink-soft uppercase">
            Assistance
          </h3>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-sm text-ink-soft">
              {aiEnabled
                ? "On. One note at a time leaves this machine."
                : "Off. Nothing about your notes leaves this machine."}
            </p>
            <button
              type="button"
              onClick={() => {
                onClose();
                onOpenAiSettings();
              }}
              className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors hover:border-accent hover:text-accent"
            >
              {aiEnabled ? "Change" : "Turn on assistance"}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}

/**
 * One palette, previewed in its own colours.
 *
 * The two data attributes are the whole trick. tokens.css addresses palettes
 * with `[data-palette][data-theme]` rather than `:root`, so setting both on
 * this wrapper redefines the variables *for its subtree* — and the swatches
 * below can be painted with the ordinary `bg-canvas` / `bg-accent` utilities
 * while still showing a palette that is not the active one. No hex reaches
 * this file, which is the rule.
 *
 * The preview is drawn in the mode currently in force, because that is the
 * question being asked: not "what does Slate look like" but "what would this
 * window look like".
 */
function PaletteCard({
  id,
  label,
  note,
  mode,
  chosen,
  onChoose,
}: {
  id: PaletteId;
  label: string;
  note: string;
  mode: ResolvedTheme;
  chosen: boolean;
  onChoose: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={chosen}
      onClick={onChoose}
      className={[
        "flex items-center gap-3 rounded-xl border px-3 py-2.5 text-left transition-colors",
        chosen
          ? "border-accent bg-accent-bg"
          : "border-border hover:border-accent",
      ].join(" ")}
    >
      <span
        data-palette={id}
        data-theme={mode}
        aria-hidden
        className="flex shrink-0 overflow-hidden rounded-lg border border-border"
      >
        <span className="size-6 bg-rail" />
        <span className="size-6 bg-canvas" />
        <span className="size-6 bg-accent" />
        <span className="size-6 bg-highlight" />
      </span>
      <span className="min-w-0">
        <span
          className={[
            "block truncate text-sm",
            chosen ? "font-semibold text-accent" : "font-medium text-ink",
          ].join(" ")}
        >
          {label}
        </span>
        <span className="block text-xs text-ink-muted">{note}</span>
      </span>
    </button>
  );
}
