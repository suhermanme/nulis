import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { HelpApp } from "./HelpApp";
import { SettingsApp } from "./SettingsApp";
import "./styles.css";

// The desktop shell loads the same bundle in its separate Settings and Help windows.
const windowKind = new URLSearchParams(window.location.search).get("window");

// This is a desktop application: the webview's own menu (Reload, Inspect
// Element) never appears. Nulis shows its own menus where they apply.
window.addEventListener("contextmenu", (event) => event.preventDefault());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {windowKind === "settings" ? <SettingsApp /> : windowKind === "help" ? <HelpApp /> : <App />}
  </React.StrictMode>,
);
