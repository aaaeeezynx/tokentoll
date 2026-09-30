import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { applyTheme, loadThemePref } from "./lib/appearance";
import { applyAccentVars, loadAccent } from "./lib/theme";

// 首幀就先套用主題與強調色（用 localStorage 快取），避免「深色閃一下」
applyTheme(loadThemePref());
applyAccentVars(loadAccent());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
