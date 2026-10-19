import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FIND_EVENTS, FindReplacePanel, type FindContext, type FindMessage, type FindTab,
} from "./FindReplacePanel";
import { closeFindWindow, sendFindMessage } from "./ipc";
import { useWindowTheme } from "./windowTheme";

function send(message: FindMessage) {
  void sendFindMessage(message).catch(() => undefined);
}

/** Root of the separate desktop Find and Replace window. */
export function FindApp() {
  const style = useWindowTheme("find");
  const [tab, setTab] = useState<FindTab>("find");
  const [seed, setSeed] = useState<{ id: number; text: string } | undefined>();
  const [context, setContext] = useState<FindContext>({ activeDocument: null, textDocuments: [] });

  useEffect(() => {
    document.title = "Find and Replace";
    let disposed = false;
    const stops: Array<() => void> = [];
    const keep = (stop: () => void) => { if (disposed) stop(); else stops.push(stop); };
    void Promise.all([
      listen<FindContext>(FIND_EVENTS.context, (event) => setContext(event.payload)).then(keep),
      listen<{ tab?: FindTab; seed?: string }>(FIND_EVENTS.open, (event) => {
        if (event.payload.tab) setTab(event.payload.tab);
        const text = event.payload.seed;
        if (text) setSeed((previous) => ({ id: (previous?.id ?? 0) + 1, text }));
      }).then(keep),
    ]).then(() => { if (!disposed) send({ kind: "ready" }); }).catch(() => undefined);
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, []);

  return <div className="aux-root" style={style}>
    <FindReplacePanel framed={false} tab={tab} onTab={setTab} seed={seed}
      activeDocument={context.activeDocument} textDocuments={context.textDocuments}
      selection={context.selection} defaultFolder={context.defaultFolder}
      onClose={() => { void closeFindWindow().catch(() => undefined); }}
      onEditorSearch={(request) => send({ kind: "search", request })}
      onOpenMatch={(target) => send({ kind: "openMatch", target })}
      onDocumentChange={(document) => send({ kind: "documentChange", document })}
      onResults={(list) => send({ kind: "results", list })}
      onChooseFolder={async () => {
        const selected = await open({ multiple: false, directory: true, title: "Choose a folder to search" });
        return typeof selected === "string" ? selected : null;
      }}
      onStatus={(message) => send({ kind: "status", message })} />
  </div>;
}
