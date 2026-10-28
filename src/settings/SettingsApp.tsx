import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getSettings, openLive, saveSettings, type Settings } from "../lib/api";
import { prettyAccelerator } from "../lib/hotkey";
import { checkForUpdate, type UpdateState } from "../lib/updater";
import { GeneralPage } from "./GeneralPage";
import { HistoryPage } from "./HistoryPage";
import { HotkeysPage } from "./HotkeysPage";
import { ModelsPage } from "./ModelsPage";
import { UpdatesPage } from "./UpdatesPage";

const PAGES = [
  { id: "general", label: "一般" },
  { id: "models", label: "模型與金鑰" },
  { id: "hotkeys", label: "熱鍵" },
  { id: "history", label: "歷史紀錄" },
  { id: "updates", label: "更新" },
] as const;
type PageId = (typeof PAGES)[number]["id"];

export function SettingsApp() {
  const [page, setPage] = useState<PageId>("general");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<{ kind: "ok" | "err"; text: string } | null>(null);
  const [update, setUpdate] = useState<UpdateState>({ phase: "idle" });

  const [loadError, setLoadError] = useState<string | null>(null);
  useEffect(() => {
    getSettings().then(setSettings).catch((e) => setLoadError(String(e)));
    // Changed elsewhere, e.g. the Live service picked in the caption bar.
    const un = listen("settings-changed", () => { getSettings().then(setSettings).catch(() => {}); });
    return () => { un.then((f) => f()); };
  }, []);

  // Quiet check at start: failures (offline, unsigned dev build) stay silent.
  useEffect(() => {
    checkForUpdate().then((s) => setUpdate(s.phase === "error" ? { phase: "idle" } : s));
  }, []);

  // Saves first, then adopts: if the backend refuses (e.g. a hotkey another app owns), the old value stays.
  const commit = useCallback(async (next: Settings) => {
    try {
      await saveSettings(next);
      setSettings(next);
      setStatus({ kind: "ok", text: "已儲存" });
    } catch (e) {
      setStatus({ kind: "err", text: String(e) });
    }
  }, []);

  useEffect(() => {
    if (status?.kind !== "ok") return;
    const t = setTimeout(() => setStatus(null), 1500);
    return () => clearTimeout(t);
  }, [status]);

  if (!settings) {
    return (
      <div className="boot" role={loadError ? "alert" : "status"}>
        {loadError ? <>無法載入設定：{loadError}</> : "載入中…"}
      </div>
    );
  }

  const liveKey = settings.hotkeys.find((h) => h.source === "live")?.accelerator;
  return (
    <div className="shell">
      <nav className="nav" aria-label="設定">
        <div className="brand">ContextLens</div>
        {PAGES.map((p) => (
          <button key={p.id} aria-current={page === p.id ? "page" : undefined} onClick={() => setPage(p.id)}>
            {p.label}
            {p.id === "updates" && update.phase === "available" && <span className="flag">有新版</span>}
          </button>
        ))}
        <span className="grow" />
        <button className="nav-live" onClick={() => openLive()} title="開啟 Live 字幕列">
          開啟 Live
          {liveKey && <kbd>{prettyAccelerator(liveKey)}</kbd>}
        </button>
      </nav>
      <main className="canvas">
        <div className="page">
          {!settings.welcome_seen && (
            <Welcome settings={settings} onDone={() => commit({ ...settings, welcome_seen: true })} onHotkeys={() => setPage("hotkeys")} />
          )}
          {page === "general" && <GeneralPage settings={settings} commit={commit} />}
          {page === "models" && <ModelsPage settings={settings} commit={commit} />}
          {page === "hotkeys" && <HotkeysPage settings={settings} commit={commit} />}
          {page === "history" && <HistoryPage settings={settings} onOpenGeneral={() => setPage("general")} />}
          {page === "updates" && <UpdatesPage state={update} setState={setUpdate} />}
          <div className={`status-line ${status?.kind ?? ""}`} role="status">{status?.text}</div>
        </div>
      </main>
    </div>
  );
}

const SOURCE_LABEL: Record<string, string> = {
  selection: "翻譯反白的文字", screen: "截圖", clipboard_image: "剪貼簿圖片", live: "開關 Live 即時翻譯",
};

/** First run: where ContextLens lives once this window closes, and how to reach each part. */
function Welcome({ settings, onDone, onHotkeys }: { settings: Settings; onDone: () => void; onHotkeys: () => void }) {
  return (
    <div className="panel welcome">
      <div className="welcome-body">
        <div className="label">ContextLens 會在背景待命</div>
        <p className="hint">
          關掉這個視窗後，ContextLens 留在右下角的系統匣（Windows 可能收在 ^ 裡），用熱鍵叫出來：
        </p>
        <ul>
          {settings.hotkeys.map((h) => (
            <li key={h.id}><kbd>{prettyAccelerator(h.accelerator)}</kbd> {SOURCE_LABEL[h.source] ?? h.source}{h.tutor ? "（教學）" : ""}</li>
          ))}
        </ul>
      </div>
      <div className="welcome-actions">
        <button className="btn" onClick={onHotkeys}>改熱鍵</button>
        <button className="btn primary" onClick={onDone}>知道了</button>
      </div>
    </div>
  );
}
