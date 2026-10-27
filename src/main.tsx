import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Popup } from "./popup/Popup";
import { SettingsApp } from "./settings/SettingsApp";
import "./styles.css";

function windowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return "main"; // plain browser during `vite` development
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{windowLabel() === "popup" ? <Popup /> : <SettingsApp />}</React.StrictMode>,
);
