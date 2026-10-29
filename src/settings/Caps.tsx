import type { Caps } from "../lib/api";
import { N_, t } from "../lib/i18n";

const ITEMS: [keyof Caps | "text", string][] = [
  ["text", N_("文字")], ["image", N_("圖片")], ["audio", N_("音訊")], ["video", N_("影片")], ["live", N_("Live 聽聲音")],
];

/** What a model can read, as a row of labels; unsupported ones are struck through and faint. */
export function CapsRow({ caps }: { caps: Caps | undefined }) {
  if (!caps) return null;
  return (
    <span className="caps" aria-label={t("支援的內容")}>
      {ITEMS.map(([k, key]) => {
        const ok = k === "text" || caps[k];
        const label = t(key);
        return <span key={k} className={ok ? "cap on" : "cap off"} title={ok ? t("支援{label}", { label }) : t("不支援{label}", { label })}>{label}</span>;
      })}
    </span>
  );
}
