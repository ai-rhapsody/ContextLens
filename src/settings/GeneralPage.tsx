import { useEffect, useState } from "react";
import { autostartGet, autostartSet, type Settings } from "../lib/api";
import { LOCALES, t } from "../lib/i18n";
import { TextField } from "./TextField";

const LANGUAGES = ["繁體中文", "简体中文", "English", "日本語", "한국어", "Español", "Français", "Deutsch"];

function Toggle({ on, label, onClick }: { on: boolean; label: string; onClick: () => void }) {
  return <button className="switch" role="switch" aria-checked={on} aria-label={label} onClick={onClick} />;
}

export function GeneralPage({ settings, commit }: { settings: Settings; commit: (s: Settings) => void }) {
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartError, setAutostartError] = useState<string | null>(null);
  useEffect(() => { autostartGet().then(setAutostart).catch(() => setAutostart(false)); }, []);
  const toggleAutostart = async () => {
    const next = !autostart;
    setAutostartError(null);
    try {
      await autostartSet(next);
      setAutostart(next);
    } catch (e) {
      setAutostartError(String(e));
    }
  };

  return (
    <>
      <h1>{t("一般")}</h1>
      <p className="lede">{t("要翻譯、解釋或搜尋，以及要不要附上教學說明，由每個熱鍵自己決定，在翻譯視窗裡也能隨時切換。")}</p>
      <div className="panel">
        <div className="row">
          <div className="grow">
            <div className="label">{t("介面語言")}</div>
            <div className="hint">{t("選單、按鈕和說明文字的語言。自動會跟著系統語言。")}</div>
          </div>
          <select
            className="field" value={settings.ui_language ?? ""} aria-label={t("介面語言")}
            onChange={(e) => commit({ ...settings, ui_language: e.target.value })}
          >
            <option value="">{t("自動（跟著系統）")}</option>
            {LOCALES.map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
          </select>
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">{t("我的語言")}</div>
            <div className="hint">{t("譯文和教學說明使用這個語言。")}</div>
          </div>
          <TextField
            value={settings.target_language}
            list="languages"
            ariaLabel={t("我的語言")}
            onCommit={(v) => v && commit({ ...settings, target_language: v })}
          />
          <datalist id="languages">{LANGUAGES.map((l) => <option key={l} value={l} />)}</datalist>
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">{t("已經是我的語言時翻成英文")}</div>
            <div className="hint">{t("選的文字已經是我的語言時，改翻成英文。")}</div>
          </div>
          <Toggle on={settings.own_language_to_english} label={t("已經是我的語言時翻成英文")}
            onClick={() => commit({ ...settings, own_language_to_english: !settings.own_language_to_english })} />
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">{t("保存歷史紀錄")}</div>
            <div className="hint">{t("翻譯完成後存在這台電腦上，可以在「歷史紀錄」搜尋。關閉後不再新增，舊紀錄保留。")}</div>
          </div>
          <button
            className="switch" role="switch" aria-checked={settings.save_history} aria-label={t("保存歷史紀錄")}
            onClick={() => commit({ ...settings, save_history: !settings.save_history })}
          />
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">{t("Live 閒置時自動暫停")}</div>
            <div className="hint">{t("一段時間沒有聲音或畫面變化就停止，避免忘記關閉而持續計費。")}</div>
          </div>
          <select
            className="field" value={settings.live_idle_minutes} aria-label={t("Live 閒置自動暫停")}
            onChange={(e) => commit({ ...settings, live_idle_minutes: Number(e.target.value) })}
          >
            {[1, 3, 5, 10, 0].map((m) => <option key={m} value={m}>{m ? t("{n} 分鐘後", { n: m }) : t("不自動暫停")}</option>)}
          </select>
        </div>
      </div>

      <h2 className="group">{t("系統匣")}</h2>
      <p className="lede">{t("ContextLens 在背景執行時只留下右下角系統匣的圖示（可能收在 ^ 裡），熱鍵照常可用。點一下圖示開關這個視窗，按右鍵可以結束。")}</p>
      <div className="panel">
        <div className="row">
          <div className="grow">
            <div className="label">{t("關閉視窗時縮到系統匣")}</div>
            <div className="hint">{t("關掉後，按視窗的 ✕ 會直接結束 ContextLens，熱鍵也會停止。")}</div>
          </div>
          <Toggle on={settings.close_to_tray} label={t("關閉視窗時縮到系統匣")}
            onClick={() => commit({ ...settings, close_to_tray: !settings.close_to_tray })} />
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">{t("最小化時縮到系統匣")}</div>
            <div className="hint">{t("最小化後不佔用工作列。")}</div>
          </div>
          <Toggle on={settings.minimize_to_tray} label={t("最小化時縮到系統匣")}
            onClick={() => commit({ ...settings, minimize_to_tray: !settings.minimize_to_tray })} />
        </div>
        <div className="row">
          <div className="grow">
            <div className="label">{t("開機時自動啟動")}</div>
            <div className="hint">{t("登入電腦後在系統匣裡待命，不會跳出視窗。")}</div>
          </div>
          <Toggle on={!!autostart} label={t("開機時自動啟動")} onClick={toggleAutostart} />
        </div>
        {autostartError && <div className="row-note">{autostartError}</div>}
        <div className="row">
          <div className="grow">
            <div className="label">{t("啟動時只顯示系統匣圖示")}</div>
            <div className="hint">{t("手動打開 ContextLens 時也不跳出這個視窗。")}</div>
          </div>
          <Toggle on={settings.start_in_tray} label={t("啟動時只顯示系統匣圖示")}
            onClick={() => commit({ ...settings, start_in_tray: !settings.start_in_tray })} />
        </div>
      </div>
    </>
  );
}
