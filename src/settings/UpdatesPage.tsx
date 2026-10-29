import { useEffect, useState } from "react";
import { diagnostics, openLink, revealLog } from "../lib/api";
import { checkForUpdate, currentVersion, installUpdate, restartApp, type UpdateState } from "../lib/updater";

const REPO_URL = "https://github.com/ai-rhapsody/ContextLens";
const PRIVACY_URL = `${REPO_URL}/blob/main/PRIVACY.md`;
const ISSUES_URL = `${REPO_URL}/issues`;
const KOFI_URL = "https://ko-fi.com/airhapsody";

export function UpdatesPage({ state, setState }: { state: UpdateState; setState: (s: UpdateState) => void }) {
  const [version, setVersion] = useState("");
  const [copied, setCopied] = useState<string | null>(null);
  useEffect(() => { currentVersion().then(setVersion); }, []);

  const copyDiagnostics = async () => {
    try {
      await navigator.clipboard.writeText(await diagnostics());
      setCopied("已複製，可以貼到回報裡。");
    } catch (e) {
      setCopied(`無法複製：${e}`);
    }
  };

  const check = async () => {
    setState({ phase: "checking" });
    setState(await checkForUpdate());
  };
  const pct = state.phase === "downloading" && state.total ? Math.min(100, Math.round((state.received / state.total) * 100)) : null;

  return (
    <>
      <h1>更新</h1>
      <p className="lede">ContextLens 會在背景檢查新版本。更新檔經過簽章驗證，通過才會安裝。</p>
      <div className="panel">
        <div className="row">
          <div className="grow">
            <div className="label">目前版本</div>
            <div className="hint num">{version || "…"}</div>
          </div>
          <button className="btn" onClick={check} disabled={state.phase === "checking" || state.phase === "downloading"}>
            {state.phase === "checking" ? "檢查中…" : "檢查更新"}
          </button>
        </div>

        {state.phase === "current" && <div className="row"><span className="muted">已經是最新版本。</span></div>}

        {state.phase === "available" && (
          <div className="row" style={{ alignItems: "flex-start", flexDirection: "column", gap: 4 }}>
            <div style={{ display: "flex", width: "100%", alignItems: "center", gap: 12 }}>
              <div className="grow"><span className="label">有新版本 <span className="num">{state.version}</span></span></div>
              <button className="btn primary" onClick={() => installUpdate(setState)}>下載並安裝</button>
            </div>
            {state.notes && <div className="release-notes">{state.notes}</div>}
          </div>
        )}

        {state.phase === "downloading" && (
          <div className="row" style={{ flexDirection: "column", alignItems: "stretch", gap: 0 }}>
            <div className="label">正在下載 <span className="num">{state.version}</span>{pct !== null && <span className="muted num"> · {pct}%</span>}</div>
            <div className="progress"><i style={{ width: `${pct ?? 100}%` }} /></div>
          </div>
        )}

        {state.phase === "ready" && (
          <div className="row">
            <div className="grow"><span className="label">已安裝 <span className="num">{state.version}</span></span><div className="hint">重新啟動後生效。</div></div>
            <button className="btn primary" onClick={() => restartApp()}>立即重新啟動</button>
          </div>
        )}

        {state.phase === "error" && <div className="row"><span className="status-line err" style={{ margin: 0 }}>無法更新：{state.message}</span></div>}
      </div>
      <h2 className="group">回報問題與支持</h2>
      <div className="panel">
        <div className="row">
          <div className="grow">
            <div className="label">診斷資訊</div>
            <div className="hint">
              版本、設定（不含金鑰）和最近的記錄。記錄檔只存在這台電腦上，不含你翻譯的內容，也不會自動上傳。
              回報問題時貼上，比較容易找出原因。
            </div>
            {copied && <div className="hint">{copied}</div>}
          </div>
          <button className="btn quiet" onClick={() => revealLog().catch((e) => setCopied(String(e)))}>開啟記錄檔</button>
          <button className="btn" onClick={copyDiagnostics}>複製診斷資訊</button>
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">回報問題</div>
            <div className="hint">在 GitHub 開一則 issue，附上診斷資訊和發生的步驟。</div>
          </div>
          <button className="btn" onClick={() => openLink(ISSUES_URL)}>回報問題</button>
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">支持開發</div>
            <div className="hint">ContextLens 免費、開源，沒有廣告。覺得好用的話，歡迎到 Ko-fi 請我喝杯咖啡。</div>
          </div>
          <button className="btn" onClick={() => openLink(KOFI_URL)}>Ko-fi ☕</button>
        </div>
      </div>
      <p className="hint about">
        檢查更新會連到 GitHub。ContextLens 不收集使用統計，你的內容只送到你設定的 AI 服務。{" "}
        <button className="link" onClick={() => openLink(PRIVACY_URL)}>隱私政策</button>
        {" · "}
        <button className="link" onClick={() => openLink(REPO_URL)}>原始碼</button>
      </p>
    </>
  );
}
