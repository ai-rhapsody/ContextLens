import type { Settings } from "../lib/api";
import { TextField } from "./TextField";

const LANGUAGES = ["繁體中文", "简体中文", "English", "日本語", "한국어", "Español", "Français", "Deutsch"];

export function GeneralPage({ settings, commit }: { settings: Settings; commit: (s: Settings) => void }) {
  return (
    <>
      <h1>一般</h1>
      <p className="lede">要不要附上教學說明，由每個熱鍵自己決定，在翻譯視窗裡也能隨時切換。</p>
      <div className="panel">
        <div className="row">
          <div className="grow">
            <div className="label">我的語言</div>
            <div className="hint">譯文和教學說明使用這個語言。</div>
          </div>
          <TextField
            value={settings.target_language}
            list="languages"
            ariaLabel="我的語言"
            onCommit={(v) => v && commit({ ...settings, target_language: v })}
          />
          <datalist id="languages">{LANGUAGES.map((l) => <option key={l} value={l} />)}</datalist>
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">保存歷史紀錄</div>
            <div className="hint">翻譯完成後存在這台電腦上，可以在「歷史紀錄」搜尋。關閉後不再新增，舊紀錄保留。</div>
          </div>
          <button
            className="switch" role="switch" aria-checked={settings.save_history} aria-label="保存歷史紀錄"
            onClick={() => commit({ ...settings, save_history: !settings.save_history })}
          />
        </div>
      </div>
    </>
  );
}
