import { useEffect, useState } from "react";
import { providerInfo, PROVIDERS, type Hotkey, type ProviderInfo, type Settings, type Source } from "../lib/api";
import { eventToAccelerator, prettyAccelerator } from "../lib/hotkey";

const SOURCES: { id: Source; label: string; needsImage: boolean }[] = [
  { id: "selection", label: "選取的文字", needsImage: false },
  { id: "screen", label: "螢幕擷取（區域、視窗、全螢幕）", needsImage: true },
  { id: "clipboard_image", label: "剪貼簿裡的圖片", needsImage: true },
  { id: "live", label: "Live 即時翻譯（開／關）", needsImage: false },
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
      hotkeys: [...settings.hotkeys, { id: `hk-${Date.now().toString(36)}`, accelerator: free, source: "selection", tutor: false }],
    });
  };

  return (
    <>
      <h1>熱鍵</h1>
      <p className="lede">每個熱鍵決定從哪裡取得內容，以及要不要附上教學說明。點組合鍵按鈕後直接按下新的組合。</p>
      <div className="panel">
        {settings.hotkeys.length === 0 && <div className="empty">還沒有熱鍵。新增一個開始使用。</div>}
        {settings.hotkeys.map((h) => {
          const needsImage = SOURCES.find((s) => s.id === h.source)?.needsImage;
          const warn = needsImage && current && !current.caps.image;
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
                <label className="inline-switch">
                  教學
                  <button
                    className="switch" role="switch" aria-checked={h.tutor} aria-label="附上教學說明"
                    onClick={() => patch(h.id, { tutor: !h.tutor })}
                  />
                </label>
                <button
                  className="btn quiet danger"
                  onClick={() => commit({ ...settings, hotkeys: settings.hotkeys.filter((x) => x.id !== h.id) })}
                >刪除</button>
              </div>
              {warn && (
                <div className="row-note">
                  目前的模型（{name} · {current!.model}）不能讀取圖片。按下時會提供改用其他服務的選項。
                </div>
              )}
            </div>
          );
        })}
        <div className="row"><button className="btn" onClick={add}>新增熱鍵</button></div>
        <div className="soon">
          螢幕擷取：按下後畫面會凍結，可切換區域、視窗、全螢幕（按 1、2、3）。剪貼簿圖片適合搭配 Windows 的 Win+Shift+S。
        </div>
      </div>
    </>
  );
}
