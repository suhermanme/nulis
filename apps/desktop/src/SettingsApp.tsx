import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { SettingsWindow } from "./SettingsWindow";
import { closeSettingsWindow, notifySettingsChanged, previewAppearance, requestMainAction } from "./ipc";

const CUSTOMIZATION_CHANGED_EVENT = "nulis://customization-changed";

/** Root of the separate desktop Settings window. */
export function SettingsApp() {
  const [closeSignal, setCloseSignal] = useState(0);
  const [externalChangeSignal, setExternalChangeSignal] = useState(0);

  useEffect(() => {
    document.title = "Settings";
    let disposed = false;
    const stops: Array<() => void> = [];
    const keep = (stop: () => void) => { if (disposed) stop(); else stops.push(stop); };
    // Closing always goes through the unapplied-changes check.
    void getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault();
      setCloseSignal((signal) => signal + 1);
    }).then(keep).catch(() => undefined);
    void listen(CUSTOMIZATION_CHANGED_EVENT, () => setExternalChangeSignal((signal) => signal + 1))
      .then(keep).catch(() => undefined);
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, []);

  return <SettingsWindow mode="window" closeSignal={closeSignal} externalChangeSignal={externalChangeSignal}
    onPreview={(appearance) => { void previewAppearance(appearance).catch(() => undefined); }}
    onSaved={() => { void notifySettingsChanged().catch(() => undefined); }}
    onMainAction={(action) => { void requestMainAction(action).catch(() => undefined); }}
    onClose={() => { void closeSettingsWindow().catch(() => undefined); }} />;
}
