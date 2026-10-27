import { useEffect, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { CustomizationState, EditorPreferences, ThemeKind, ThemeRecord } from "./contracts";
import { executeCommand, windowReady } from "./ipc";
import { preferenceCssVariables, preferenceWindowTheme, type WindowThemePayload } from "./preferences";

const CUSTOMIZATION_CHANGED_EVENT = "nulis://customization-changed";

/**
 * Shows this secondary window once `ready` is true and the page has painted.
 * The shell creates it hidden so it never appears blank, and shows it anyway
 * after a short time if this never happens.
 */
export function useShowWhenReady(ready: boolean): void {
  const shown = useRef(false);
  useEffect(() => {
    if (!ready || shown.current) return undefined;
    shown.current = true;
    // The themed page is in the DOM now. Animation frames never run while the
    // window is hidden, so waiting for one only ever hit the shell's fallback
    // timer. Lay the page out now instead; the first frame drawn when the
    // window appears is then the finished, themed page.
    void window.document.documentElement.offsetHeight;
    void windowReady().catch(() => undefined);
    return undefined;
  }, [ready]);
}


/**
 * Gives the native window the theme's light or dark appearance. Without it the
 * title bar and the open list of a dropdown, which the system draws, follow the
 * system setting instead of the chosen theme.
 */
function setWindowAppearance(kind: ThemeKind): void {
  try {
    void getCurrentWindow().setTheme(kind).catch(() => undefined);
  } catch {
    // Outside the desktop shell (a browser or a test) there is no native window.
  }
}

/**
 * Paints the exported theme colors on a window root as CSS custom properties.
 * Applied user-theme colors (readability floors included) drive the chrome; built-in
 * themes export their own palette unchanged. Returns a restore that puts the root
 * back exactly as it was, including the `data-custom` marker for user themes.
 */
export function exportWindowTheme(
  element: HTMLElement,
  payload: WindowThemePayload,
  baseColors: Record<string, string>,
): () => void {
  const record: ThemeRecord = {
    id: payload.theme,
    name: payload.theme,
    version: 1,
    kind: payload.kind,
    colors: { ...baseColors, ...payload.colors },
    builtIn: payload.customTheme === undefined,
    ...(payload.customTheme === undefined ? {} : { custom: payload.customTheme }),
  };
  const vars: Record<string, string> = {};
  // Map the colors through the same table the workbench uses; the font entries
  // come from preferences here, so only color variables are exported.
  const fonts: EditorPreferences = { theme: payload.theme, fontFamily: " ", fontSize: 0, lineHeight: 0, fontLigatures: false };
  for (const [name, value] of Object.entries(preferenceCssVariables(fonts, [record]))) {
    if (value && !name.includes("font") && !name.includes("line-height") && !name.includes("ligature")) vars[name] = value;
  }
  const userTheme = payload.customTheme !== undefined;
  const prior = new Map<string, string | null>();
  for (const key of Object.keys(vars)) prior.set(key, element.style.getPropertyValue(key));
  const priorCustom = element.dataset.custom ?? null;
  const priorScheme = element.style.getPropertyValue("color-scheme");
  for (const [key, value] of Object.entries(vars)) element.style.setProperty(key, value);
  // Native controls in the page (scrollbars, checkboxes) follow the theme too.
  element.style.setProperty("color-scheme", payload.kind);
  setWindowAppearance(payload.kind);
  if (userTheme) element.dataset.custom = "true"; else delete element.dataset.custom;
  return () => {
    for (const [key, value] of prior) {
      if (value === null) element.style.removeProperty(key); else element.style.setProperty(key, value);
    }
    if (priorCustom === null) delete element.dataset.custom; else element.dataset.custom = priorCustom;
    if (priorScheme) element.style.setProperty("color-scheme", priorScheme); else element.style.removeProperty("color-scheme");
  };
}


/** CSS variables for a secondary window, following the theme and font chosen in Settings. */
export function useWindowTheme(windowId: string): CSSProperties | undefined {
  const [style, setStyle] = useState<CSSProperties | undefined>(undefined);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    let restore: (() => void) | undefined;
    const applyTheme = () => {
      void Promise.all([
        executeCommand<CustomizationState>({ id: "customization.state", context: { windowId }, arguments: {} }),
        executeCommand<ThemeRecord[]>({ id: "theme.list", context: { windowId }, arguments: {} }),
      ]).then(([state, themes]) => {
        if (!disposed && state?.document?.appearance) {
          const appearance = state.document.appearance;
          setStyle(preferenceCssVariables(appearance, themes) as CSSProperties);
          // The applied theme's colors repaint this window's chrome; built-ins
          // export their own palette, so their appearance never shifts.
          restore?.();
          const payload = preferenceWindowTheme(appearance, themes);
          const base = themes.find((theme) => theme.id === payload.theme) ?? themes[0];
          restore = exportWindowTheme(window.document.documentElement, payload, base?.colors ?? {});
        }
      }).catch(() => undefined);
    };
    applyTheme();
    void listen(CUSTOMIZATION_CHANGED_EVENT, applyTheme).then((unlisten) => {
      if (disposed) unlisten(); else stop = unlisten;
    }).catch(() => undefined);
    return () => { disposed = true; stop?.(); restore?.(); };
  }, [windowId]);
  useShowWhenReady(style !== undefined);
  return style;
}
