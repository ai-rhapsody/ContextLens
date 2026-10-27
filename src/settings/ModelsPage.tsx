import { useEffect, useState } from "react";
import {
  deleteApiKey, providerInfo, PROVIDERS, setApiKey, testProvider, type Caps, type Provider, type Settings,
} from "../lib/api";
import { CapsRow } from "./Caps";
import { TextField } from "./TextField";

type Note = { kind: "ok" | "err"; text: string } | null;

/** Providers with a real-time API, and which setting holds its model. */
const LIVE_FIELD: Partial<Record<Provider, "gemini_live" | "openai_realtime">> = {
  gemini: "gemini_live",
  openai: "openai_realtime",
};
const LIVE_HINT: Partial<Record<Provider, string>> = {
  gemini: "例如 gemini-live-2.5-flash-preview",
  openai: "例如 gpt-realtime",
};

function ProviderRow(props: {
  id: Provider; name: string; vendor: string; settings: Settings; hasKey: boolean; caps?: Caps; web?: boolean;
  commit: (s: Settings) => void; refresh: () => void;
}) {
  const { id, settings, hasKey } = props;
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<Note>(null);

  const guard = async (fn: () => Promise<void>) => {
    setBusy(true);
    setNote(null);
    try { await fn(); } catch (e) { setNote({ kind: "err", text: String(e) }); } finally { setBusy(false); }
  };

  return (
    <div className="provider">
      <div className="provider-head">
        <label>
          <input
            type="radio" name="provider" checked={settings.provider === id}
            onChange={() => props.commit({ ...settings, provider: id })}
          />
          {props.name} <span className="faint" style={{ fontWeight: 400 }}>{props.vendor}</span>
        </label>
        <span className="grow" style={{ flex: 1 }} />
        {settings.provider === id && <span className="muted" style={{ fontSize: 12 }}>使用中</span>}
        {hasKey ? <span className="pill ok">金鑰已設定</span> : <span className="faint">未設定</span>}
      </div>
      <div className="provider-line">
        <span className="muted" style={{ width: 40 }}>模型</span>
        <TextField
          value={settings.models[id]} ariaLabel={`${props.name} 模型`}
          onCommit={(v) => v && props.commit({ ...settings, models: { ...settings.models, [id]: v } })}
        />
      </div>
      {LIVE_FIELD[id] && (
        <div className="provider-line">
          <span className="muted" style={{ width: 40 }} title="Live 即時翻譯用的模型">即時</span>
          <TextField
            value={settings.models[LIVE_FIELD[id]!]} ariaLabel={`${props.name} 即時模型`}
            placeholder={LIVE_HINT[id]}
            onCommit={(v) => v && props.commit({ ...settings, models: { ...settings.models, [LIVE_FIELD[id]!]: v } })}
          />
        </div>
      )}
      <div className="provider-line">
        <span className="muted" style={{ width: 40 }}>能力</span>
        <CapsRow caps={props.caps} />
        {props.web && <span className="cap on" title="搜尋動作可以上網查資料">可上網</span>}
      </div>
      <div className="provider-line">
        <span className="muted" style={{ width: 40 }}>金鑰</span>
        <input
          className="field" type="password" value={key} autoComplete="off" spellCheck={false}
          placeholder={hasKey ? "貼上新金鑰以取代" : "貼上 API 金鑰"} aria-label={`${props.name} API 金鑰`}
          onChange={(e) => setKey(e.target.value)}
        />
        <button
          className="btn primary" disabled={busy || !key.trim()}
          onClick={() => guard(async () => {
            await setApiKey(id, key);
            setKey("");
            props.refresh();
            setNote({ kind: "ok", text: "已存進系統鑰匙圈。" });
          })}
        >儲存</button>
        <button
          className="btn" disabled={busy || !hasKey}
          onClick={() => guard(async () => {
            const model = await testProvider(id);
            setNote({ kind: "ok", text: `連線成功（${model}）。` });
          })}
        >測試</button>
        <button
          className="btn quiet danger" disabled={busy || !hasKey}
          onClick={() => guard(async () => { await deleteApiKey(id); props.refresh(); setNote(null); })}
        >移除</button>
      </div>
      {note && <div className={`status-line ${note.kind}`} style={{ marginTop: 0 }}>{note.text}</div>}
    </div>
  );
}

export function ModelsPage({ settings, commit }: { settings: Settings; commit: (s: Settings) => void }) {
  const [info, setInfo] = useState<Record<Provider, { has_key: boolean; caps: Caps; web: boolean }> | null>(null);
  const refresh = () => {
    providerInfo().then((rows) => setInfo(Object.fromEntries(rows.map((r) => [r.provider, r])) as never));
  };
  // Model names change what a model can read, so recheck whenever settings change.
  useEffect(refresh, [settings]);

  return (
    <>
      <h1>模型與金鑰</h1>
      <p className="lede">
        選一個預設服務，貼上你自己的 API 金鑰。金鑰存在系統鑰匙圈，不會寫進設定檔。
        「能力」依模型名稱判斷；預設服務做不到的事，會讓你改用其他有金鑰的服務。
        Live 聽聲音需要 Gemini 或 ChatGPT 的「即時」模型；只看螢幕時，任何能讀圖片的模型都可以。
      </p>
      <div className="panel">
        {PROVIDERS.map((p) => (
          <ProviderRow
            key={p.id} {...p} settings={settings} hasKey={info?.[p.id]?.has_key ?? false} caps={info?.[p.id]?.caps}
            web={info?.[p.id]?.web}
            commit={commit} refresh={refresh}
          />
        ))}
      </div>
    </>
  );
}
