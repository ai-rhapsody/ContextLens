import { useEffect, useState } from "react";
import { checkForUpdate, currentVersion, installUpdate, restartApp, type UpdateState } from "../lib/updater";

export function UpdatesPage({ state, setState }: { state: UpdateState; setState: (s: UpdateState) => void }) {
  const [version, setVersion] = useState("");
  useEffect(() => { currentVersion().then(setVersion); }, []);

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
            {state.notes && <pre className="release-notes">{state.notes}</pre>}
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
    </>
  );
}
