import { useEffect, useState } from "react";
import {
  deleteApiKey, keyStatus, PROVIDERS, setApiKey, testProvider, type Provider, type Settings,
} from "../lib/api";
import { TextField } from "./TextField";

type Note = { kind: "ok" | "err"; text: string } | null;

function ProviderRow(props: {
  id: Provider; name: string; vendor: string; settings: Settings; hasKey: boolean;
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
  const [keys, setKeys] = useState<Record<Provider, boolean> | null>(null);
  const refresh = () => { keyStatus().then(setKeys); };
  useEffect(refresh, []);

  return (
    <>
      <h1>模型與金鑰</h1>
      <p className="lede">選一個服務，貼上你自己的 API 金鑰。金鑰存在系統鑰匙圈，不會寫進設定檔，也不會送到畫面上。</p>
      <div className="panel">
        {PROVIDERS.map((p) => (
          <ProviderRow key={p.id} {...p} settings={settings} hasKey={keys?.[p.id] ?? false} commit={commit} refresh={refresh} />
        ))}
      </div>
    </>
  );
}
