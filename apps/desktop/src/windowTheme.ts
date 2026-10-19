import { useEffect, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import type { CustomizationState, ThemeRecord } from "./contracts";
import { executeCommand } from "./ipc";
import { preferenceCssVariables } from "./preferences";

const CUSTOMIZATION_CHANGED_EVENT = "nulis://customization-changed";

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
  return style;
}
