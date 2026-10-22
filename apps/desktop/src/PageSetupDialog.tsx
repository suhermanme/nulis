import { useEffect, useState } from "react";
import type { PrintPageFrame, PrintedPage } from "./contracts";
import { keepFocusInDialog } from "./dialogFocus";
import { ScaledPageView } from "./PrintedPageView";
import {
  BAND_FIELDS, FONT_SIZE_RANGE, MARGIN_PRESETS, MARGIN_RANGE, PAPER_SIZES, cleanBandText, defaultPrintSettings,
  marginPreset, normalizePrintSettings, type Band, type ColorMode, type MarginPreset, type Margins, type PrintSettings,
} from "./printSettings";
import "./print.css";

/** The first page laid out for some settings, or why it cannot be. */
export type PreviewPage = (settings: PrintSettings) => Promise<{ frame: PrintPageFrame; page: PrintedPage }>;

/** Waits this long after a change before asking the core for a new preview. */
const PREVIEW_DELAY_MS = 200;
const PREVIEW_WIDTH_PX = 250;

const COLOR_LABELS: Record<ColorMode, string> = {
  print: "Print colors (light)",
  blackAndWhite: "Black and white",
  theme: "Current theme",
};

function errorText(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  if (reason && typeof reason === "object" && "message" in reason) return String((reason as { message: unknown }).message);
  return String(reason);
}

function BandFields({ legend, band, onChange }: { legend: string; band: Band; onChange(band: Band): void }) {
  return (
    <fieldset className="page-setup-band">
      <legend>{legend}</legend>
      {(["left", "center", "right"] as const).map((part) => (
        <label key={part}>
          <span>{part === "left" ? "Left" : part === "center" ? "Center" : "Right"}</span>
          <input type="text" value={band[part]} spellCheck={false} aria-label={`${legend} ${part}`}
            onChange={(event) => onChange({ ...band, [part]: cleanBandText(event.target.value) })} />
        </label>
      ))}
    </fieldset>
  );
}

/**
 * Page Setup (`IMPROVEMENT_PLAN_3.md` §3.2): paper, margins, font, what
 * each page shows, and colors, with a live preview of the first page laid
 * out by the core.
 */
export function PageSetupDialog({ initial, preview, onSave, onCancel }: {
  initial: PrintSettings;
  preview: PreviewPage;
  onSave(settings: PrintSettings): void;
  onCancel(): void;
}) {
  const [settings, setSettings] = useState(initial);
  const [shown, setShown] = useState<{ frame: PrintPageFrame; page: PrintedPage } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [preset, setPreset] = useState<MarginPreset>(() => marginPreset(initial.setup.margins));
  const setup = settings.setup;

  useEffect(() => {
    let active = true;
    const timer = window.setTimeout(() => {
      void preview(settings).then((page) => {
        if (!active) return;
        setShown(page);
        setProblem(null);
      }).catch((reason: unknown) => { if (active) setProblem(errorText(reason)); });
    }, PREVIEW_DELAY_MS);
    return () => { active = false; window.clearTimeout(timer); };
  }, [preview, settings]);

  const change = (next: Partial<PrintSettings["setup"]>) => setSettings((current) => ({ ...current, setup: { ...current.setup, ...next } }));
  const changeMargin = (side: keyof Margins, value: number) => {
    if (!Number.isFinite(value)) return;
    change({ margins: { ...setup.margins, [side]: Math.min(MARGIN_RANGE.max, Math.max(MARGIN_RANGE.min, value)) } });
  };
  const choosePreset = (next: MarginPreset) => {
    setPreset(next);
    if (next !== "custom") {
      const size = MARGIN_PRESETS[next];
      change({ margins: { top: size, right: size, bottom: size, left: size } });
    }
  };

  return (
    <div className="decision-backdrop">
      <form className="decision-dialog page-setup-dialog" role="dialog" aria-modal="true" aria-labelledby="page-setup-title"
        onSubmit={(event) => { event.preventDefault(); if (!problem) onSave(normalizePrintSettings(settings)); }}
        onKeyDown={(event) => {
          keepFocusInDialog(event);
          if (event.key === "Escape") onCancel();
        }}>
        <h2 id="page-setup-title">Page Setup</h2>
        <div className="page-setup-body">
          <div className="page-setup-fields">
            <div className="page-setup-grid">
              <label>Paper
                <select autoFocus value={setup.paper} onChange={(event) => change({ paper: event.target.value as PrintSettings["setup"]["paper"] })}>
                  {PAPER_SIZES.map((paper) => <option key={paper.id} value={paper.id}>{paper.label}</option>)}
                </select>
              </label>
              <label>Orientation
                <select value={setup.orientation} onChange={(event) => change({ orientation: event.target.value === "landscape" ? "landscape" : "portrait" })}>
                  <option value="portrait">Portrait</option>
                  <option value="landscape">Landscape</option>
                </select>
              </label>
              <label>Margins
                <select value={preset} onChange={(event) => choosePreset(event.target.value as MarginPreset)}>
                  <option value="narrow">Narrow ({MARGIN_PRESETS.narrow} mm)</option>
                  <option value="normal">Normal ({MARGIN_PRESETS.normal} mm)</option>
                  <option value="wide">Wide ({MARGIN_PRESETS.wide} mm)</option>
                  <option value="custom">Custom</option>
                </select>
              </label>
              <label>Font size (pt)
                <input type="number" min={FONT_SIZE_RANGE.min} max={FONT_SIZE_RANGE.max} step={0.5} value={setup.fontSize}
                  onChange={(event) => {
                    const size = event.target.valueAsNumber;
                    if (Number.isFinite(size)) change({ fontSize: Math.min(FONT_SIZE_RANGE.max, Math.max(FONT_SIZE_RANGE.min, size)) });
                  }} />
              </label>
            </div>
            {preset === "custom" ? (
              <fieldset className="page-setup-margins">
                <legend>Margins (mm)</legend>
                {(["top", "bottom", "left", "right"] as const).map((side) => (
                  <label key={side}>{side[0].toUpperCase() + side.slice(1)}
                    <input type="number" min={MARGIN_RANGE.min} max={MARGIN_RANGE.max} step={0.5} value={setup.margins[side]}
                      onChange={(event) => changeMargin(side, event.target.valueAsNumber)} />
                  </label>
                ))}
              </fieldset>
            ) : null}
            <div className="page-setup-grid">
              <label>Long lines
                <select value={setup.longLines} onChange={(event) => change({ longLines: event.target.value === "cut" ? "cut" : "wrap" })}>
                  <option value="wrap">Wrap onto the next row</option>
                  <option value="cut">Cut off at the edge</option>
                </select>
              </label>
              <label>Colors
                <select value={settings.colors} onChange={(event) => setSettings((current) => ({ ...current, colors: event.target.value as ColorMode }))}>
                  {(Object.keys(COLOR_LABELS) as ColorMode[]).map((mode) => <option key={mode} value={mode}>{COLOR_LABELS[mode]}</option>)}
                </select>
              </label>
            </div>
            <div className="page-setup-checks">
              <label><input type="checkbox" checked={setup.lineNumbers} onChange={(event) => change({ lineNumbers: event.target.checked })} /> Line numbers</label>
              <label><input type="checkbox" checked={setup.syntaxHighlighting} onChange={(event) => change({ syntaxHighlighting: event.target.checked })} /> Syntax highlighting</label>
            </div>
            <BandFields legend="Header" band={setup.header} onChange={(header) => change({ header })} />
            <BandFields legend="Footer" band={setup.footer} onChange={(footer) => change({ footer })} />
            <p className="page-setup-hint">Fields: {BAND_FIELDS.join(" ")}. Leave all three empty for no header or footer.</p>
          </div>
          <div className="page-setup-preview" aria-label="Preview of the first page">
            {shown ? <ScaledPageView page={shown.page} frame={shown.frame} widthPx={PREVIEW_WIDTH_PX} /> : <div className="page-setup-preview-empty">Preview…</div>}
            {problem ? <p role="alert" className="page-setup-problem">{problem}</p> : null}
          </div>
        </div>
        <div className="decision-actions">
          <button type="button" className="page-setup-reset" onClick={() => {
            const defaults = defaultPrintSettings();
            setSettings(defaults);
            setPreset(marginPreset(defaults.setup.margins));
          }}>Restore Defaults</button>
          <button type="button" onClick={onCancel}>Cancel</button>
          <button className="primary" type="submit" disabled={Boolean(problem)}>Save</button>
        </div>
      </form>
    </div>
  );
}
