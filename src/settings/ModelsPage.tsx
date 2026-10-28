import { useEffect, useState } from "react";
import {
  deleteApiKey, liveSetProvider, testLive, providerInfo, PROVIDERS, setApiKey, testProvider, type Caps, type Provider,
  type ProviderInfo, type Settings,
} from "../lib/api";
import { LIVE_PRESETS, MODEL_PRESETS } from "../lib/models";
import { CapsRow } from "./Caps";
import { ModelPicker } from "./ModelPicker";

type Note = { kind: "ok" | "err"; text: string } | null;

/** Providers with a real-time API, and which setting holds its model. */
const LIVE_FIELD: Partial<Record<Provider, "gemini_live" | "openai_realtime">> = {
  gemini: "gemini_live",
  openai: "openai_realtime",
};
const LIVE_HINT: Partial<Record<Provider, string>> = {
  gemini: "例如 gemini-3.8-live",
  openai: "例如 gpt-realtime-2.1",
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
        <ModelPicker
          value={settings.models[id]} presets={MODEL_PRESETS[id]} ariaLabel={`${props.name} 模型`}
          onCommit={(v) => props.commit({ ...settings, models: { ...settings.models, [id]: v } })}
        />
      </div>
      {LIVE_FIELD[id] && (
        <div className="provider-line">
          <span className="muted" style={{ width: 40 }} title="Live 即時翻譯用的模型">即時</span>
          <ModelPicker
            value={settings.models[LIVE_FIELD[id]!]} presets={LIVE_PRESETS[id] ?? []} ariaLabel={`${props.name} 即時模型`}
            placeholder={LIVE_HINT[id]}
            onCommit={(v) => props.commit({ ...settings, models: { ...settings.models, [LIVE_FIELD[id]!]: v } })}
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
        {LIVE_FIELD[id] && (
          <button
            className="btn" disabled={busy || !hasKey} title="真的開一條即時連線，送一句話，確認即時模型能用"
            onClick={() => guard(async () => {
              setNote({ kind: "ok", text: "正在連線即時模型…" });
              const model = await testLive(id);
              setNote({ kind: "ok", text: `Live 連線成功（${model}）。` });
            })}
          >測試 Live</button>
        )}
        <button
          className="btn quiet danger" disabled={busy || !hasKey}
          onClick={() => guard(async () => { await deleteApiKey(id); props.refresh(); setNote(null); })}
        >移除</button>
      </div>
      {note && <div className={`status-line ${note.kind}`} style={{ marginTop: 0 }}>{note.text}</div>}
    </div>
  );
}

const nameOf = (p: Provider) => PROVIDERS.find((x) => x.id === p)?.name ?? p;

/** What Live will use, mirroring the backend: the chosen service, else the default, then the others. */
function liveResolution(settings: Settings, rows: ProviderInfo[]) {
  const by = (p: Provider) => rows.find((r) => r.provider === p);
  const order = settings.live_provider
    ? [settings.live_provider]
    : [settings.provider, ...PROVIDERS.map((p) => p.id).filter((p) => p !== settings.provider)];
  const audio = order.map(by).find((r) => r?.has_key && r.caps.live && r.live_model);
  const screen = order.map(by).find((r) => r?.has_key && r.caps.image);
  return {
    audio: audio ? `${nameOf(audio.provider)} · ${audio.live_model}` : null,
    screen: screen ? `${nameOf(screen.provider)} · ${screen.model}` : null,
  };
}

function LivePanel({ settings, rows }: { settings: Settings; rows: ProviderInfo[] }) {
  const [error, setError] = useState<string | null>(null);
  const r = liveResolution(settings, rows);
  const choose = (v: string) => {
    setError(null);
    liveSetProvider((v || null) as Provider | null).catch((e) => setError(String(e)));
  };
  return (
    <>
      <h2 className="group">Live 即時翻譯</h2>
      <p className="lede">一般翻譯和 Live 可以用不同服務，例如平常用 Claude、Live 用 Gemini。</p>
      <div className="panel">
        <div className="row">
          <div className="grow">
            <div className="label">Live 使用的服務</div>
            <div className="hint">「自動」先試預設服務，做不到時改用其他有金鑰的服務。字幕列也能切換。</div>
          </div>
          <select
            className="field" value={settings.live_provider ?? ""} aria-label="Live 使用的服務"
            onChange={(e) => choose(e.target.value)}
          >
            <option value="">自動</option>
            {PROVIDERS.map((p) => {
              const row = rows.find((x) => x.provider === p.id);
              return (
                <option key={p.id} value={p.id}>
                  {p.name}{row && !row.caps.live ? "（只能看螢幕）" : ""}
                </option>
              );
            })}
          </select>
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">目前會用</div>
            <div className="hint">
              聽聲音：{r.audio ?? <span className="warn">沒有可用的即時模型（需要 Gemini 或 ChatGPT 的金鑰）</span>}
              <br />
              只看螢幕：{r.screen ?? <span className="warn">沒有能讀圖片且有金鑰的模型</span>}
            </div>
          </div>
        </div>
        {error && <div className="row-note">{error}</div>}
      </div>
    </>
  );
}

export function ModelsPage({ settings, commit }: { settings: Settings; commit: (s: Settings) => void }) {
  const [info, setInfo] = useState<Record<Provider, { has_key: boolean; caps: Caps; web: boolean }> | null>(null);
  const [rows, setRows] = useState<ProviderInfo[]>([]);
  const refresh = () => {
    providerInfo().then((rows) => {
      setRows(rows);
      setInfo(Object.fromEntries(rows.map((r) => [r.provider, r])) as never);
    });
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
      <LivePanel settings={settings} rows={rows} />
    </>
  );
}
