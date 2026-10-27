import type { Settings } from "../lib/api";
import { TextField } from "./TextField";

const LANGUAGES = ["繁體中文", "简体中文", "English", "日本語", "한국어", "Español", "Français", "Deutsch"];

export function GeneralPage({ settings, commit }: { settings: Settings; commit: (s: Settings) => void }) {
  return (
    <>
      <h1>一般</h1>
      <p className="lede">翻譯會寫成你的語言。教學模式會多解釋句子結構、用法和背景知識。</p>
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
            <div className="label">教學模式</div>
            <div className="hint">
              除了翻譯，也說明這段文字的結構、用法和延伸知識。翻譯熱鍵會依這個開關決定；「翻譯並教學」熱鍵不受影響。
            </div>
          </div>
          <button
            className="switch"
            role="switch"
            aria-checked={settings.tutor_mode}
            aria-label="教學模式"
            onClick={() => commit({ ...settings, tutor_mode: !settings.tutor_mode })}
          />
        </div>
      </div>
    </>
  );
}
