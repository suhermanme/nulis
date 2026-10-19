import { useEffect } from "react";
import { HelpWindow } from "./HelpWindow";
import { closeHelpWindow } from "./ipc";
import { useWindowTheme } from "./windowTheme";

/** Root of the separate desktop Help window. */
export function HelpApp() {
  const style = useWindowTheme("help");
  useEffect(() => { document.title = "Nulis Help"; }, []);
  return <HelpWindow mode="window" style={style} onClose={() => { void closeHelpWindow().catch(() => undefined); }} />;
}
