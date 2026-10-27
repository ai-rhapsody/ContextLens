import { useEffect, useState } from "react";
import type { ActionId, Hotkey, Settings } from "../lib/api";
import { eventToAccelerator, prettyAccelerator } from "../lib/hotkey";

const ACTIONS: { id: ActionId; label: string }[] = [
  { id: "translate_selection", label: "翻譯選取的文字" },
  { id: "tutor_selection", label: "翻譯並教學" },
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
  const patch = (id: string, change: Partial<Hotkey>) =>
    commit({ ...settings, hotkeys: settings.hotkeys.map((h) => (h.id === id ? { ...h, ...change } : h)) });

  const add = () => {
    const taken = new Set(settings.hotkeys.map((h) => h.accelerator));
    const free = ["Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5"].find((a) => !taken.has(a)) ?? "Alt+9";
    commit({
      ...settings,
      hotkeys: [...settings.hotkeys, { id: `hk-${Date.now().toString(36)}`, accelerator: free, action: "translate_selection" }],
    });
  };

  return (
    <>
      <h1>熱鍵</h1>
      <p className="lede">反白文字後按熱鍵，ContextLens 會在游標旁開出翻譯視窗。點組合鍵按鈕後直接按下新的組合。</p>
      <div className="panel">
        {settings.hotkeys.length === 0 && <div className="empty">還沒有熱鍵。新增一個開始使用。</div>}
        {settings.hotkeys.map((h) => (
          <div className="row" key={h.id}>
            <Recorder value={h.accelerator} onPick={(acc) => patch(h.id, { accelerator: acc })} />
            <select
              className="field grow" value={h.action} aria-label="動作"
              onChange={(e) => patch(h.id, { action: e.target.value as ActionId })}
            >
              {ACTIONS.map((a) => <option key={a.id} value={a.id}>{a.label}</option>)}
              <option disabled>擷取圖片（即將推出）</option>
              <option disabled>擷取影片片段（即將推出）</option>
            </select>
            <button
              className="btn quiet danger"
              onClick={() => commit({ ...settings, hotkeys: settings.hotkeys.filter((x) => x.id !== h.id) })}
            >刪除</button>
          </div>
        ))}
        <div className="row"><button className="btn" onClick={add}>新增熱鍵</button></div>
        <div className="soon">圖片和影片片段的擷取還在規劃中，之後會出現在動作清單裡。</div>
      </div>
    </>
  );
}
