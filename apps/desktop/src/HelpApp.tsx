import { useEffect, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import type { CustomizationState, ThemeRecord } from "./contracts";
import { HelpWindow } from "./HelpWindow";
import { closeHelpWindow, executeCommand } from "./ipc";
import { preferenceCssVariables } from "./preferences";

const CUSTOMIZATION_CHANGED_EVENT = "nulis://customization-changed";
const CONTEXT = { windowId: "help" };

/** Root of the separate desktop Help window. */
export function HelpApp() {
  const [style, setStyle] = useState<CSSProperties | undefined>(undefined);

  useEffect(() => {
    document.title = "Nulis Help";
    let disposed = false;
    let stop: (() => void) | undefined;
    // The window follows the theme and editor font chosen in Settings.
    const applyTheme = () => {
      void Promise.all([
        executeCommand<CustomizationState>({ id: "customization.state", context: CONTEXT, arguments: {} }),
        executeCommand<ThemeRecord[]>({ id: "theme.list", context: CONTEXT, arguments: {} }),
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
  }, []);

  return <HelpWindow mode="window" style={style} onClose={() => { void closeHelpWindow().catch(() => undefined); }} />;
}
