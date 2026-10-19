import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { FindApp } from "./FindApp";
import { HelpApp } from "./HelpApp";
import { ResultsApp } from "./ResultsApp";
import { SettingsApp } from "./SettingsApp";
import "./styles.css";

// The desktop shell loads the same bundle in its separate Settings and Help windows.
const windowKind = new URLSearchParams(window.location.search).get("window");

// This is a desktop application: the webview's own menu (Reload, Inspect
// Element) never appears. Nulis shows its own menus where they apply.
window.addEventListener("contextmenu", (event) => event.preventDefault());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {windowKind === "settings" ? <SettingsApp /> : windowKind === "help" ? <HelpApp />
      : windowKind === "find" ? <FindApp /> : windowKind === "find-results" ? <ResultsApp /> : <App />}
  </React.StrictMode>,
);
