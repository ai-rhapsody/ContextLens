import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { pickLocale, setLocale, t } from "./lib/i18n";
import { Popup } from "./popup/Popup";
import { SettingsApp } from "./settings/SettingsApp";
import { Snip } from "./snip/Snip";
import { Live } from "./live/Live";
import { Chat } from "./chat/Chat";
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
    box.textContent = `${t("ContextLens 無法啟動：")}\n${message}`;
    root.appendChild(box);
  }
}
window.addEventListener("error", (e) => showFatal(String(e.error ?? e.message)));
window.addEventListener("unhandledrejection", (e) => showFatal(String(e.reason)));

/** The interface language chosen in settings or, on automatic, the system's. */
async function pickLanguage() {
  try {
    setLocale(pickLocale(await invoke<string>("ui_locale")));
  } catch {
    setLocale(pickLocale(navigator.language)); // plain browser during `vite` development
  }
  // Changing the language in settings redraws every window in the new one.
  listen("locale-changed", () => location.reload()).catch(() => {});
}

pickLanguage().then(() => {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      {windowLabel().startsWith("snip-") ? <Snip /> : ({ popup: <Popup />, live: <Live />, chat: <Chat /> }[windowLabel()] ?? <SettingsApp />)}
    </React.StrictMode>,
  );
});
