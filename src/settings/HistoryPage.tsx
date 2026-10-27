import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { historyClear, historyDelete, historyList, openLink, PROVIDERS, type HistoryEntry, type Settings } from "../lib/api";
import { KIND_LABEL } from "../lib/files";
import { parseSections } from "../lib/sections";
import { Rich } from "../lib/Rich";

function when(ms: number): string {
  const diff = (Date.now() - ms) / 1000;
  if (diff < 60) return "剛剛";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分鐘前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小時前`;
  const d = new Date(ms);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString("zh-TW", sameYear ? { month: "numeric", day: "numeric" } : undefined);
}

/** The line shown in the list: the translation, the explanation or the identification. */
function translationOf(e: HistoryEntry): string {
  const s = parseSections(e.result + "\n");
  return (s.find((x) => x.title === "翻譯") ?? s.find((x) => x.title === "說明") ?? s.find((x) => x.title === "這是什麼"))?.body
    ?? e.result;
}

export function HistoryPage({ settings, onOpenGeneral }: { settings: Settings; onOpenGeneral: () => void }) {
  const [query, setQuery] = useState("");
  const [rows, setRows] = useState<HistoryEntry[] | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [copied, setCopied] = useState<number | null>(null);

  const load = useCallback(() => { historyList(query).then(setRows); }, [query]);
  useEffect(() => {
    const t = setTimeout(load, 150);
    return () => clearTimeout(t);
  }, [load]);
  useEffect(() => {
    const un = listen("history-changed", load);
    return () => { un.then((f) => f()); };
  }, [load]);

  const remove = async (at: number) => {
    await historyDelete(at);
    if (open === at) setOpen(null);
    load();
  };

  const copy = async (e: HistoryEntry) => {
    await navigator.clipboard.writeText(translationOf(e)).catch(() => {});
    setCopied(e.at_ms);
    setTimeout(() => setCopied(null), 1200);
  };

  return (
    <>
      <h1>歷史紀錄</h1>
      <p className="lede">翻譯完成後的結果，存在這台電腦上，最多 1000 筆。</p>
      {!settings.save_history && (
        <div className="panel"><div className="row">
          <span className="grow muted">保存歷史紀錄目前是關閉的，新的翻譯不會出現在這裡。</span>
          <button className="btn" onClick={onOpenGeneral}>前往設定</button>
        </div></div>
      )}
      <div className="toolbar">
        <input className="field grow" type="search" placeholder="搜尋原文或譯文" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="搜尋" />
        <button
          className={`btn quiet${confirmClear ? " danger" : ""}`}
          disabled={!rows?.length}
          onClick={async () => {
            if (!confirmClear) { setConfirmClear(true); setTimeout(() => setConfirmClear(false), 3000); return; }
            await historyClear();
            setConfirmClear(false);
            load();
          }}
        >{confirmClear ? "再按一次清除全部" : "清除全部"}</button>
      </div>
      <div className="panel history">
        {rows === null && <div className="empty">載入中…</div>}
        {rows?.length === 0 && (
          <div className="empty">{query ? `找不到「${query}」。` : "還沒有紀錄。翻譯完成後會出現在這裡。"}</div>
        )}
        {rows?.map((e) => {
          const isOpen = open === e.at_ms;
          const provider = PROVIDERS.find((p) => p.id === e.provider)?.name;
          return (
            <div className={`h-row${isOpen ? " open" : ""}`} key={e.at_ms}>
              <button className="h-main" onClick={() => setOpen(isOpen ? null : e.at_ms)} aria-expanded={isOpen}>
                {e.thumb ? <img className="h-thumb" src={e.thumb} alt="" /> : e.kind !== "text" ? <span className="h-kind">{KIND_LABEL[e.kind]}</span> : null}
                <span className="h-text">
                  <span className="h-src">{e.source}</span>
                  <span className="h-res">{translationOf(e)}</span>
                </span>
                <span className="h-meta">
                  <span className="num">{when(e.at_ms)}</span>
                  {e.tutor && <span className="faint">教學</span>}
                </span>
              </button>
              {isOpen && (
                <div className="h-detail">
                  {parseSections(e.result + "\n").map((s, i) => (
                    <section className="sec" key={i}>{s.title && <h2>{s.title}</h2>}<Rich text={s.body} /></section>
                  ))}
                  {!!e.sources?.length && (
                    <section className="sec sources">
                      <h2>來源</h2>
                      <ol>
                        {e.sources.map(([title, url]) => (
                          <li key={url}><button className="link" onClick={() => openLink(url)} title={url}>{title}</button></li>
                        ))}
                      </ol>
                    </section>
                  )}
                  <div className="h-actions">
                    <span className="faint grow">{provider} · {e.model}</span>
                    <button className="btn" onClick={() => copy(e)}>{copied === e.at_ms ? "已複製" : "複製譯文"}</button>
                    <button className="btn quiet danger" onClick={() => remove(e.at_ms)}>刪除</button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </>
  );
}
