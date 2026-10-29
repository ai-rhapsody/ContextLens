import type { GlossaryEntry, Settings } from "../lib/api";
import { TextField } from "./TextField";

/** Fixed renderings for names and terms the models keep getting wrong. */
export function GlossaryPage({ settings, commit }: { settings: Settings; commit: (s: Settings) => void }) {
  const list = settings.glossary;
  const save = (glossary: GlossaryEntry[]) => commit({ ...settings, glossary });
  const patch = (i: number, change: Partial<GlossaryEntry>) => save(list.map((e, j) => (j === i ? { ...e, ...change } : e)));

  return (
    <>
      <h1>詞彙表</h1>
      <p className="lede">
        固定專有名詞的譯法，例如歌名、人名、作品名。翻譯、解釋、搜尋、追問和 Live 都會照這裡的寫法。{""}
        只有內容裡出現的詞才會送出，不會拖慢翻譯。
      </p>
      <div className="panel glossary">
        {list.length > 0 && (
          <div className="row gl-head" aria-hidden>
            <span>原文</span><span>譯為</span><span>說明（選填）</span><span />
          </div>
        )}
        {list.map((e, i) => (
          <div className="row gl-row" key={i}>
            <TextField value={e.term} placeholder="例如 SWIM" ariaLabel="原文" onCommit={(v) => patch(i, { term: v })} />
            <TextField value={e.rendering} placeholder="例如 SWIM" ariaLabel="譯為" onCommit={(v) => patch(i, { rendering: v })} />
            <TextField value={e.note} placeholder="例如：BTS 的歌名" ariaLabel="說明" onCommit={(v) => patch(i, { note: v })} />
            <button className="btn quiet" aria-label="刪除" title="刪除" onClick={() => save(list.filter((_, j) => j !== i))}>✕</button>
          </div>
        ))}
        {!list.length && <div className="empty">還沒有詞彙。按「新增」加入第一個。</div>}
        <div className="row">
          <span className="grow hint">
            同步口譯模型（Live 只聽聲音、沒開註解時）不接受指令，詞彙表對它無效；打開「註解」就會生效。
          </span>
          <button className="btn" onClick={() => save([...list, { term: "", rendering: "", note: "" }])}>新增</button>
        </div>
      </div>
    </>
  );
}
