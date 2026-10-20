import { useEffect, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import type { CustomizationState, ThemeRecord } from "./contracts";
import { executeCommand, windowReady } from "./ipc";
import { preferenceCssVariables } from "./preferences";

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
    // Two frames: the first lays out the themed page, the second paints it.
    let second = 0;
    const first = window.requestAnimationFrame(() => {
      second = window.requestAnimationFrame(() => { void windowReady().catch(() => undefined); });
    });
    return () => { window.cancelAnimationFrame(first); window.cancelAnimationFrame(second); };
  }, [ready]);
}

/** CSS variables for a secondary window, following the theme and font chosen in Settings. */
export function useWindowTheme(windowId: string): CSSProperties | undefined {
  const [style, setStyle] = useState<CSSProperties | undefined>(undefined);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    const context = { windowId };
    const applyTheme = () => {
      void Promise.all([
        executeCommand<CustomizationState>({ id: "customization.state", context, arguments: {} }),
        executeCommand<ThemeRecord[]>({ id: "theme.list", context, arguments: {} }),
      ]).then(([state, themes]) => {
        if (!disposed && state?.document?.appearance) {
          setStyle(preferenceCssVariables(state.document.appearance, themes) as CSSProperties);
        }
      }).catch(() => undefined);
    };
    applyTheme();
    void listen(CUSTOMIZATION_CHANGED_EVENT, applyTheme).then((unlisten) => {
      if (disposed) unlisten(); else stop = unlisten;
    }).catch(() => undefined);
    return () => { disposed = true; stop?.(); };
  }, [windowId]);
  useShowWhenReady(style !== undefined);
  return style;
}
