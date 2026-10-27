import { useEffect, useState } from "react";
import {
  platformInfo, providerInfo, PROVIDERS, type PlatformInfo, TASKS, type Hotkey, type LiveSource, type ProviderInfo, type Settings, type Source,
  type Task,
} from "../lib/api";
import { eventToAccelerator, prettyAccelerator } from "../lib/hotkey";

const SOURCES: { id: Source; label: string; needsImage: boolean }[] = [
  { id: "screen", label: "截圖", needsImage: true },
  { id: "selection", label: "選取的文字", needsImage: false },
  { id: "clipboard_image", label: "剪貼簿圖片", needsImage: true },
  { id: "live", label: "Live 即時翻譯（開／關）", needsImage: false },
];

const LIVE_SOURCES: { id: LiveSource; label: string }[] = [
  { id: "mic", label: "麥克風" },
  { id: "system", label: "系統聲音" },
  { id: "screen", label: "螢幕" },
];

function Recorder({ value, onPick }: { value: string; onPick: (acc: string) => void }) {
  const [recording, setRecording] = useState(false);
  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") return setRecording(false);
      const acc = eventToAccelerator(e);
      if (acc) {
        setRecording(false);
        onPick(acc);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onPick]);

  return (
    <button className="btn" style={{ minWidth: 150 }} onClick={() => setRecording((r) => !r)} onBlur={() => setRecording(false)}>
      {recording ? "按下新的組合鍵…" : <kbd>{prettyAccelerator(value)}</kbd>}
    </button>
  );
}

export function HotkeysPage(props: { settings: Settings; commit: (s: Settings) => void }) {
  const { settings, commit } = props;
  const [info, setInfo] = useState<ProviderInfo[]>([]);
  const [platform, setPlatform] = useState<PlatformInfo | null>(null);
  useEffect(() => { platformInfo().then(setPlatform).catch(() => {}); }, []);
  useEffect(() => { providerInfo().then(setInfo); }, [settings]);
  const current = info.find((p) => p.provider === settings.provider);
  const name = PROVIDERS.find((p) => p.id === settings.provider)?.name;

  const patch = (id: string, change: Partial<Hotkey>) =>
    commit({ ...settings, hotkeys: settings.hotkeys.map((h) => (h.id === id ? { ...h, ...change } : h)) });

  const add = () => {
    const taken = new Set(settings.hotkeys.map((h) => h.accelerator));
    const free = ["Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5"].find((a) => !taken.has(a)) ?? "Alt+9";
    commit({
      ...settings,
      hotkeys: [
        ...settings.hotkeys,
        { id: `hk-${Date.now().toString(36)}`, accelerator: free, source: "screen", task: "explain", tutor: false },
      ],
    });
  };

  return (
    <>
      <h1>熱鍵</h1>
      <p className="lede">
        每個熱鍵決定從哪裡取得內容、要做什麼：翻譯、解釋或搜尋。翻譯視窗裡也能隨時切換。點組合鍵按鈕後直接按下新的組合。
      </p>
      <div className="panel">
        {settings.hotkeys.length === 0 && <div className="empty">還沒有熱鍵。新增一個開始使用。</div>}
        {settings.hotkeys.map((h) => {
          const isLive = h.source === "live";
          const needsImage = SOURCES.find((s) => s.id === h.source)?.needsImage;
          const noImage = needsImage && current && !current.caps.image;
          const noWeb = !isLive && h.task === "search" && current && !current.web;
          return (
            <div className="hk" key={h.id}>
              <div className="row">
                <Recorder value={h.accelerator} onPick={(acc) => patch(h.id, { accelerator: acc })} />
                <select
                  className="field grow" value={h.source} aria-label="來源"
                  onChange={(e) => patch(h.id, { source: e.target.value as Source })}
                >
                  {SOURCES.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}
                </select>
                {!isLive && (
                  <select
                    className="field" value={h.task} aria-label="動作" style={{ width: 92 }}
                    onChange={(e) => patch(h.id, { task: e.target.value as Task })}
                  >
                    {TASKS.map((t) => <option key={t.id} value={t.id}>{t.label}</option>)}
                  </select>
                )}
                <label className="inline-switch" style={{ visibility: isLive || h.task === "translate" ? "visible" : "hidden" }}>
                  {isLive ? "註解" : "教學"}
                  <button
                    className="switch" role="switch" aria-checked={h.tutor}
                    aria-label={isLive ? "Live 教學註解" : "附上教學說明"}
                    onClick={() => patch(h.id, { tutor: !h.tutor })}
                  />
                </label>
                <button
                  className="btn quiet danger"
                  onClick={() => commit({ ...settings, hotkeys: settings.hotkeys.filter((x) => x.id !== h.id) })}
                >刪除</button>
              </div>
              {isLive && (
                <div className="row hk-live">
                  <span className="hint">按下時開啟</span>
                  <div className="lv-sources" role="group" aria-label="Live 來源">
                    {LIVE_SOURCES.map((src) => {
                      const on = (h.live_sources ?? []).includes(src.id);
                      return (
                        <button
                          key={src.id} aria-pressed={on}
                          onClick={() => {
                            const cur = h.live_sources ?? [];
                            patch(h.id, { live_sources: on ? cur.filter((x) => x !== src.id) : [...cur, src.id] });
                          }}
                        >{src.label}</button>
                      );
                    })}
                  </div>
                  <span className="hint grow">
                    {!(h.live_sources ?? []).length
                      ? "沒選的話用上次在字幕列選的來源。"
                      : (h.live_sources ?? []).includes("screen")
                        ? "還沒選過螢幕範圍時，按下後會先讓你框選。"
                        : ""}
                  </span>
                </div>
              )}
              {noImage && (
                <div className="row-note">
                  目前的模型（{name} · {current!.model}）不能讀取圖片。按下時會提供改用其他服務的選項。
                </div>
              )}
              {noWeb && (
                <div className="row-note">
                  目前的模型（{name} · {current!.model}）無法上網，搜尋會改用模型本身的知識。Gemini 和 Claude 可以上網搜尋。
                </div>
              )}
            </div>
          );
        })}
        <div className="row"><button className="btn" onClick={add}>新增熱鍵</button></div>
        {platform?.wayland && settings.hotkeys.length > 0 && (
          <div className="soon wayland">
            <p>
              Wayland 不允許程式自己註冊全域熱鍵，上面的組合鍵可能沒有反應。可以到桌面的「設定 → 鍵盤 → 自訂快捷鍵」
              新增快捷鍵，指令填下面對應的那一行：
            </p>
            <ol>
              {settings.hotkeys.map((h, i) => (
                <li key={h.id}>
                  <code className="mono">{`"${platform.exe}" --hotkey ${i + 1}`}</code>
                  <span className="faint"> · {SOURCES.find((x) => x.id === h.source)?.label}</span>
                </li>
              ))}
            </ol>
          </div>
        )}
        <div className="soon">
          截圖：按下熱鍵後畫面凍結，直接框選區域、點選視窗或選全螢幕（按 1、2、3 切換），不必先另外截圖。
        </div>
      </div>
    </>
  );
}
