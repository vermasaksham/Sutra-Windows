import { useState } from "react";
import {
  FONT_CHOICES,
  typographyApi,
  useTypography,
  setTypography,
  type Typography,
} from "./typography";

/**
 * How the app is set in type.
 *
 * Every control here writes through to Rust and back, so what is on screen is
 * always what is stored — there is no local draft that could disagree with the
 * file. It costs a round trip per change over a loopback bridge, which is
 * nothing, and buys a panel that cannot lie about the current state.
 */
export default function TypographySettings({
  onReport,
}: {
  onReport: (message: string, cause: unknown) => void;
}) {
  const type = useTypography();
  const [busy, setBusy] = useState(false);
  const [naming, setNaming] = useState("");

  async function change(patch: Partial<Typography>) {
    try {
      setTypography(await typographyApi.set({ ...type, ...patch }));
    } catch (cause) {
      onReport("Could not save the typography", cause);
    }
  }

  async function addFont() {
    const family = naming.trim();
    if (family === "") return;
    setBusy(true);
    try {
      const next = await typographyApi.importFont(family);
      // Null means the picker was cancelled, which is not a failure and needs
      // no message.
      if (next) {
        setTypography(next);
        setNaming("");
      }
    } catch (cause) {
      onReport("Could not import the font", cause);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <FontPicker
        label="Reading font"
        hint="The note itself."
        value={type.reading}
        custom={type.fonts}
        onPick={(reading) => void change({ reading })}
      />
      <FontPicker
        label="Interface font"
        hint="The rail, the list and the menus."
        value={type.interface}
        custom={type.fonts}
        onPick={(next) => void change({ interface: next })}
      />

      <Slider
        label="Text size"
        value={type.size}
        min={11}
        max={28}
        step={1}
        unit="px"
        onChange={(size) => void change({ size })}
      />
      <Slider
        label="Line spacing"
        value={type.leading}
        min={1.2}
        max={2.4}
        step={0.05}
        unit=""
        onChange={(leading) => void change({ leading })}
      />
      <Slider
        label="Line width"
        value={type.width}
        min={480}
        max={1200}
        step={20}
        unit="px"
        onChange={(width) => void change({ width })}
      />

      <div className="flex flex-col gap-2 border-t border-border pt-3">
        <h4 className="text-xs font-semibold tracking-wide text-ink-soft uppercase">
          Your own fonts
        </h4>
        <p className="text-xs text-ink-muted">
          {/*
            Copied in, not linked. A font referenced where it happens to sit
            today stops working the moment that folder moves, and nothing would
            be able to say why. The copy lives beside the app's settings rather
            than in the vault, because a typeface belongs to this screen and not
            to the notes.
          */}
          Add a <code className="font-mono text-xs">.woff2</code>,{" "}
          <code className="font-mono text-xs">.woff</code>,{" "}
          <code className="font-mono text-xs">.ttf</code> or{" "}
          <code className="font-mono text-xs">.otf</code> file. It is copied
          into Sutra&rsquo;s own folder, so it keeps working offline and after
          the original moves.
        </p>

        {type.fonts.length > 0 && (
          <ul className="flex flex-col gap-1">
            {type.fonts.map((font) => (
              <li
                key={font.family}
                className="flex items-center justify-between gap-2 rounded-lg border border-border px-2.5 py-1.5"
              >
                <span
                  className="min-w-0 flex-1 truncate text-sm text-ink"
                  style={{ fontFamily: `"${font.family}"` }}
                >
                  {font.family}
                </span>
                <button
                  type="button"
                  onClick={() => {
                    void typographyApi
                      .removeFont(font.family)
                      .then(setTypography)
                      .catch((cause) =>
                        onReport("Could not remove the font", cause),
                      );
                  }}
                  className="shrink-0 text-xs text-ink-muted transition-colors duration-150 ease-out hover:text-accent"
                >
                  remove
                </button>
              </li>
            ))}
          </ul>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <input
            value={naming}
            onChange={(e) => setNaming(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addFont();
            }}
            placeholder="Name it, e.g. Charter"
            aria-label="Name for the font you are adding"
            className="min-w-0 flex-1 rounded border border-border bg-surface px-2 py-1 text-sm text-ink outline-none focus:border-accent"
          />
          <button
            type="button"
            disabled={busy || naming.trim() === ""}
            onClick={() => void addFont()}
            className="rounded-lg border border-border px-3 py-1.5 text-sm text-ink-soft transition-colors duration-150 ease-out hover:border-accent hover:text-accent disabled:opacity-50"
          >
            {busy ? "Adding…" : "Choose a file…"}
          </button>
        </div>
      </div>

      <div
        className="rounded-lg border border-border bg-surface p-3"
        style={{
          fontFamily: type.reading
            ? `"${type.reading}", var(--font-body)`
            : "var(--font-body)",
          fontSize: `${type.size}px`,
          lineHeight: type.leading,
        }}
      >
        {/*
          A specimen in the actual settings, with the actual subject matter.
          Lorem ipsum would not show whether the digits line up or the
          subscripts survive, which is most of what matters here.
        */}
        <p className="text-ink">
          Sb₂Se₃ nanowires showed κ = 0.037 ± 0.002 W m⁻¹K⁻¹ at 300 K, roughly
          an order of magnitude below the bulk value — Ko et al., 2024.
        </p>
      </div>
    </div>
  );
}

function FontPicker({
  label,
  hint,
  value,
  custom,
  onPick,
}: {
  label: string;
  hint: string;
  value: string;
  custom: Array<{ family: string }>;
  onPick: (family: string) => void;
}) {
  // A family stored from another machine, or typed by hand, is still a real
  // setting — it is offered back rather than silently reset to the default.
  const known =
    FONT_CHOICES.some((f) => f.id === value) ||
    custom.some((f) => f.family === value);

  return (
    <label className="flex flex-col gap-1 text-sm">
      <span className="text-ink-soft">
        {label} <span className="text-ink-muted">— {hint}</span>
      </span>
      <select
        aria-label={label}
        value={known ? value : "__typed"}
        onChange={(e) => {
          if (e.target.value !== "__typed") onPick(e.target.value);
        }}
        className="rounded border border-border bg-surface px-2 py-1 text-sm text-ink"
      >
        {FONT_CHOICES.map((font) => (
          <option key={font.id || "default"} value={font.id}>
            {font.label} — {font.note}
          </option>
        ))}
        {custom.length > 0 && (
          <optgroup label="Yours">
            {custom.map((font) => (
              <option key={font.family} value={font.family}>
                {font.family}
              </option>
            ))}
          </optgroup>
        )}
        <option value="__typed">Another font…</option>
      </select>
      {!known && (
        <input
          value={value}
          onChange={(e) => onPick(e.target.value)}
          placeholder="Family name, exactly as the system knows it"
          aria-label={`${label}, typed`}
          className="rounded border border-border bg-surface px-2 py-1 text-sm text-ink outline-none focus:border-accent"
        />
      )}
    </label>
  );
}

function Slider({
  label,
  value,
  min,
  max,
  step,
  unit,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit: string;
  onChange: (value: number) => void;
}) {
  return (
    <label className="flex items-center gap-3 text-sm">
      <span className="w-24 shrink-0 text-ink-soft">{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label={label}
        className="min-w-0 flex-1 accent-accent"
      />
      <span className="w-14 shrink-0 text-right font-mono text-xs tabular-nums text-ink-muted">
        {step < 1 ? value.toFixed(2) : Math.round(value)}
        {unit}
      </span>
    </label>
  );
}
