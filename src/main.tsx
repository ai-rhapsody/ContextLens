import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Popup } from "./popup/Popup";
import { SettingsApp } from "./settings/SettingsApp";
import { Snip } from "./snip/Snip";
import { Live } from "./live/Live";
import "./styles.css";

function windowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return "main"; // plain browser during `vite` development
  }
}

// If anything fails before or during the first render, show it instead of an empty window.
function showFatal(message: string) {
  const root = document.getElementById("root");
  if (root && !root.childElementCount) {
    root.innerHTML = "";
    const box = document.createElement("pre");
    box.className = "boot";
    box.textContent = `ContextLens 無法啟動：\n${message}`;
    root.appendChild(box);
  }
}
window.addEventListener("error", (e) => showFatal(String(e.error ?? e.message)));
window.addEventListener("unhandledrejection", (e) => showFatal(String(e.reason)));

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {{ popup: <Popup />, snip: <Snip />, live: <Live /> }[windowLabel()] ?? <SettingsApp />}
  </React.StrictMode>,
);
