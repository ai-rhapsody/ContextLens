import { useEffect, useState } from "react";
import { openLive, providerInfo, type Settings } from "../lib/api";
import { TextField } from "./TextField";

const IDLE = [1, 3, 5, 10, 0];

export function LivePage({ settings, commit }: { settings: Settings; commit: (s: Settings) => void }) {
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  useEffect(() => {
    providerInfo().then((rows) => setHasKey(rows.find((r) => r.provider === "gemini")?.has_key ?? false));
  }, [settings]);
  const model = settings.models.gemini_live;
  const looksLive = /live|native-audio/i.test(model);

  return (
    <>
      <h1>Live 即時翻譯</h1>
      <p className="lede">
        持續聽麥克風或電腦播放的聲音，在畫面底部的字幕列即時顯示翻譯。只能使用 Gemini 的 Live 模型，
        開啟期間持續計費。
      </p>
      <div className="panel">
        <div className="row">
          <div className="grow">
            <div className="label">Live 模型</div>
            <div className="hint">Gemini 的 Live 模型名稱，例如 gemini-live-2.5-flash-preview。名稱含 live 或 native-audio 才會被視為 Live 模型。</div>
          </div>
          <TextField
            value={model} ariaLabel="Live 模型"
            onCommit={(v) => v && commit({ ...settings, models: { ...settings.models, gemini_live: v } })}
          />
        </div>
        {!looksLive && <div className="row-note">這個名稱看起來不是 Live 模型，字幕列會無法開始。</div>}
        {hasKey === false && <div className="row-note">還沒有 Gemini 金鑰。到「模型與金鑰」貼上 Gemini 的 API 金鑰。</div>}
        <div className="row">
          <div className="grow">
            <div className="label">沒有聲音時自動暫停</div>
            <div className="hint">避免忘記關閉而持續計費。</div>
          </div>
          <select
            className="field" value={settings.live_idle_minutes} aria-label="自動暫停"
            onChange={(e) => commit({ ...settings, live_idle_minutes: Number(e.target.value) })}
          >
            {IDLE.map((m) => <option key={m} value={m}>{m ? `${m} 分鐘後` : "不自動暫停"}</option>)}
          </select>
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">字幕列</div>
            <div className="hint">也可以在「熱鍵」新增「Live 即時翻譯（開／關）」，或從系統匣開啟。</div>
          </div>
          <button className="btn" onClick={() => openLive()}>開啟字幕列</button>
        </div>
        <div className="soon">
          系統聲音目前只支援 Windows。螢幕畫面會在下一步加入。公司網路若必須透過 Proxy 才能連外，Live 可能無法連線。
        </div>
      </div>
    </>
  );
}
