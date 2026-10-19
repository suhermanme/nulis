import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { FIND_EVENTS, ResultsWindow, type FindMessage, type ResultList } from "./FindReplacePanel";
import { closeFindWindow, sendFindMessage } from "./ipc";
import { useWindowTheme } from "./windowTheme";

type ResultsPayload = ResultList & { openDocumentIds: string[] };

function send(message: FindMessage) {
  void sendFindMessage(message).catch(() => undefined);
}

/** Root of the separate desktop window listing matching lines. */
export function ResultsApp() {
  const style = useWindowTheme("find-results");
  const [list, setList] = useState<ResultsPayload | null>(null);
  const [message, setMessage] = useState("");

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<ResultsPayload>(FIND_EVENTS.results, (event) => setList(event.payload)).then((unlisten) => {
      if (disposed) { unlisten(); return; }
      stop = unlisten;
      send({ kind: "resultsReady" });
    }).catch(() => undefined);
    return () => { disposed = true; stop?.(); };
  }, []);
  useEffect(() => { document.title = list?.title ?? "Lines Containing"; }, [list?.title]);

  return <div className="aux-root" style={style}>
    {list ? <ResultsWindow framed={false} list={list} openDocumentIds={new Set(list.openDocumentIds)}
      onClose={() => { void closeFindWindow().catch(() => undefined); }}
      onOpenMatch={(target) => send({ kind: "openMatch", target })}
      onStatus={(text) => { setMessage(text); send({ kind: "status", message: text }); }} />
      : <p className="results-empty" role="status">Waiting for results…</p>}
    {message ? <p className="screen-reader-only" role="status">{message}</p> : null}
  </div>;
}
