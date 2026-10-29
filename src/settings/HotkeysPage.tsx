import { useEffect, useState } from "react";
import {
  platformInfo, providerInfo, PROVIDERS, type PlatformInfo, TASKS, type Hotkey, type LiveSource, type ProviderInfo, type Settings, type Source,
  type Task,
} from "../lib/api";
import { eventToAccelerator, prettyAccelerator } from "../lib/hotkey";
import { N_, t } from "../lib/i18n";
import { TextField } from "./TextField";

/** Ready-made custom actions to start from. */
const EXAMPLES: { name: string; instruction: string }[] = [
  { name: N_("潤飾英文"), instruction: N_("把內容潤飾成自然、正式的英文 email，保留原意。") },
  { name: N_("摘要"), instruction: N_("把內容摘要成三到五個重點。") },
  { name: N_("解釋程式碼"), instruction: N_("逐段解釋這段程式碼在做什麼，指出可能的問題。") },
  { name: N_("改成口語"), instruction: N_("把內容改寫成輕鬆、口語的說法，語言不變。") },
];

const SOURCES: { id: Source; label: string; needsImage: boolean }[] = [
  { id: "screen", label: N_("截圖"), needsImage: true },
  { id: "selection", label: N_("選取的文字"), needsImage: false },
  { id: "clipboard_image", label: N_("剪貼簿圖片"), needsImage: true },
  { id: "live", label: N_("Live 即時翻譯（開／關）"), needsImage: false },
];

const LIVE_SOURCES: { id: LiveSource; label: string }[] = [
  { id: "mic", label: N_("麥克風") },
  { id: "system", label: N_("系統聲音") },
  { id: "screen", label: N_("螢幕") },
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
      {recording ? t("按下新的組合鍵…") : <kbd>{prettyAccelerator(value)}</kbd>}
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
      <h1>{t("熱鍵")}</h1>
      <p className="lede">
        {t("每個熱鍵決定從哪裡取得內容、要做什麼：翻譯、解釋、搜尋，或用「自訂」寫你自己的指令（例如潤飾英文 email）。翻譯視窗裡也能隨時切換。點組合鍵按鈕後直接按下新的組合。")}
      </p>
      <div className="panel">
        {settings.hotkeys.length === 0 && <div className="empty">{t("還沒有熱鍵。新增一個開始使用。")}</div>}
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
                  className="field grow" value={h.source} aria-label={t("來源")}
                  onChange={(e) => patch(h.id, { source: e.target.value as Source })}
                >
                  {SOURCES.map((s) => <option key={s.id} value={s.id}>{t(s.label)}</option>)}
                </select>
                {!isLive && (
                  <select
                    className="field" value={h.task} aria-label={t("動作")} style={{ width: 92 }}
                    onChange={(e) => {
                      const task = e.target.value as Task;
                      patch(h.id, task === "custom" ? { task, custom: h.custom ?? { name: "", instruction: "" } } : { task });
                    }}
                  >
                    {TASKS.map((k) => <option key={k.id} value={k.id}>{t(k.label)}</option>)}
                    <option value="custom">{t("自訂…")}</option>
                  </select>
                )}
                <label className="inline-switch" style={{ visibility: isLive || h.task === "translate" ? "visible" : "hidden" }}>
                  {isLive ? t("註解") : t("教學")}
                  <button
                    className="switch" role="switch" aria-checked={h.tutor}
                    aria-label={isLive ? t("Live 教學註解") : t("附上教學說明")}
                    onClick={() => patch(h.id, { tutor: !h.tutor })}
                  />
                </label>
                <button
                  className="btn quiet danger"
                  onClick={() => commit({ ...settings, hotkeys: settings.hotkeys.filter((x) => x.id !== h.id) })}
                >{t("刪除")}</button>
              </div>
              {!isLive && h.task === "custom" && (
                <div className="row hk-live hk-custom">
                  <TextField
                    className="hk-name" value={h.custom?.name ?? ""} placeholder={t("名稱")} ariaLabel={t("自訂動作名稱")}
                    onCommit={(v) => patch(h.id, { custom: { name: v, instruction: h.custom?.instruction ?? "" } })}
                  />
                  <TextField
                    className="grow" value={h.custom?.instruction ?? ""} placeholder={t("要做什麼，例如：把內容潤飾成正式的英文 email")}
                    ariaLabel={t("自訂指令")}
                    onCommit={(v) => patch(h.id, { custom: { name: h.custom?.name ?? "", instruction: v } })}
                  />
                </div>
              )}
              {!isLive && h.task === "custom" && !h.custom?.instruction && (
                <div className="row hk-live">
                  <span className="hint">{t("範例：")}</span>
                  {EXAMPLES.map((x) => (
                    <button key={x.name} className="chip sm" onClick={() => patch(h.id, { custom: { name: t(x.name), instruction: t(x.instruction) } })}>{t(x.name)}</button>
                  ))}
                  <span className="hint grow">{t("還沒寫指令時，按下會改用「解釋」。")}</span>
                </div>
              )}
              {isLive && (
                <div className="row hk-live">
                  <span className="hint">{t("按下時開啟")}</span>
                  <div className="lv-sources" role="group" aria-label={t("Live 來源")}>
                    {LIVE_SOURCES.map((src) => {
                      const on = (h.live_sources ?? []).includes(src.id);
                      return (
                        <button
                          key={src.id} aria-pressed={on}
                          onClick={() => {
                            const cur = h.live_sources ?? [];
                            patch(h.id, { live_sources: on ? cur.filter((x) => x !== src.id) : [...cur, src.id] });
                          }}
                        >{t(src.label)}</button>
                      );
                    })}
                  </div>
                  <span className="hint grow">
                    {!(h.live_sources ?? []).length
                      ? t("沒選的話用上次在字幕列選的來源。")
                      : (h.live_sources ?? []).includes("screen")
                        ? t("還沒選過螢幕範圍時，按下後會先讓你框選。")
                        : ""}
                  </span>
                </div>
              )}
              {noImage && (
                <div className="row-note">
                  {t("目前的模型（{name} · {model}）不能讀取圖片。按下時會提供改用其他服務的選項。", { name: name ?? "", model: current!.model })}
                </div>
              )}
              {noWeb && (
                <div className="row-note">
                  {t("目前的模型（{name} · {model}）無法上網，搜尋會改用模型本身的知識。Gemini 和 Claude 可以上網搜尋。", { name: name ?? "", model: current!.model })}
                </div>
              )}
            </div>
          );
        })}
        <div className="row"><button className="btn" onClick={add}>{t("新增熱鍵")}</button></div>
        {platform?.wayland && settings.hotkeys.length > 0 && (
          <div className="soon wayland">
            <p>
              {t("Wayland 不允許程式自己註冊全域熱鍵，上面的組合鍵可能沒有反應。可以到桌面的「設定 → 鍵盤 → 自訂快捷鍵」新增快捷鍵，指令填下面對應的那一行：")}
            </p>
            <ol>
              {settings.hotkeys.map((h, i) => (
                <li key={h.id}>
                  <code className="mono">{`"${platform.exe}" --hotkey ${i + 1}`}</code>
                  <span className="faint"> · {t(SOURCES.find((x) => x.id === h.source)?.label ?? "")}</span>
                </li>
              ))}
            </ol>
          </div>
        )}
        <div className="soon">
          {t("截圖：按下熱鍵後畫面凍結，直接框選區域、點選視窗或選全螢幕（按 1、2、3 切換），不必先另外截圖。")}
        </div>
      </div>
    </>
  );
}
