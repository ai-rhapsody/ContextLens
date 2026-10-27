import type { Caps } from "../lib/api";

const ITEMS: [keyof Caps | "text", string][] = [
  ["text", "文字"], ["image", "圖片"], ["audio", "音訊"], ["video", "影片"], ["live", "Live"],
];

/** What a model can read, as a row of labels; unsupported ones are struck through and faint. */
export function CapsRow({ caps }: { caps: Caps | undefined }) {
  if (!caps) return null;
  return (
    <span className="caps" aria-label="支援的內容">
      {ITEMS.map(([k, label]) => {
        const ok = k === "text" || caps[k];
        return <span key={k} className={ok ? "cap on" : "cap off"} title={ok ? `支援${label}` : `不支援${label}`}>{label}</span>;
      })}
    </span>
  );
}
