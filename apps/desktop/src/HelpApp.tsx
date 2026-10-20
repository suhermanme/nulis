import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { HelpWindow } from "./HelpWindow";
import { closeHelpWindow, type HelpTopic } from "./ipc";
import { useWindowTheme } from "./windowTheme";

/** Asks an open Help window to show a topic (see `open_help_window`). */
const HELP_TOPIC_EVENT = "nulis://help-topic";

/** The topic the window was opened at, from its address. */
function initialTopic(): { id: number; section: string; heading?: string } | undefined {
  const params = new URLSearchParams(window.location.search);
  const section = params.get("section");
  return section ? { id: 1, section, heading: params.get("heading") ?? undefined } : undefined;
}

/** Root of the separate desktop Help window. */
export function HelpApp() {
  const style = useWindowTheme("help");
  const [topic, setTopic] = useState(initialTopic);
  useEffect(() => { document.title = "Nulis Help"; }, []);
  useEffect(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    void listen<HelpTopic>(HELP_TOPIC_EVENT, (event) => {
      setTopic((previous) => ({ id: (previous?.id ?? 0) + 1, section: event.payload.section, heading: event.payload.heading || undefined }));
    }).then((unlisten) => { if (disposed) unlisten(); else stop = unlisten; }).catch(() => undefined);
    return () => { disposed = true; stop?.(); };
  }, []);
  return <HelpWindow mode="window" style={style} topic={topic}
    onClose={() => { void closeHelpWindow().catch(() => undefined); }} />;
}
