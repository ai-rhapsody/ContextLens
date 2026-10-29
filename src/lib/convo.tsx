import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { followUp, openLink, searchWeb, type PhraseAction, type StreamEvent, type Task } from "./api";
import { parseSections } from "./sections";
import { Rich } from "./Rich";
import { isKey, N_, t } from "./i18n";

/** The section each task leads with, and the one "copy" takes. */
export const LEAD: Record<Task, string> = { translate: N_("翻譯"), explain: N_("說明"), search: N_("這是什麼"), custom: "" };
export const QUERIES = N_("搜尋關鍵字");
const MUTED = new Set([N_("原文"), N_("內容")]);

export function host(url: string) {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

/** A model answer: headed sections when the prompt asked for them, otherwise plain text. */
export function Answer({ text, task, streaming }: { text: string; task: Task; streaming: boolean }) {
  const sections = useMemo(() => parseSections(text), [text]);
  if (!sections.some((s) => s.title)) {
    return <div className={`answer plain${streaming ? " caret" : ""}`}><Rich text={text} /></div>;
  }
  return (
    <>
      {sections.map((s, i) => (
        <section
          key={s.title ?? i}
          className={`sec${isKey(s.title, LEAD[task]) ? " lead" : ""}${[...MUTED].some((k) => isKey(s.title, k)) ? " orig" : ""}`}
        >
          {s.title && <h2>{s.title}</h2>}
          {isKey(s.title, QUERIES) ? (
            <div className="queries">
              {s.body.split("\n").map((l) => l.replace(/^\s*[-*•]\s*/, "").replace(/^["「]|["」]$/g, "").trim())
                .filter(Boolean)
                .map((q) => (
                  <button key={q} className="chip" onClick={() => searchWeb(q)} title={t("用瀏覽器搜尋")}>{q} ↗</button>
                ))}
            </div>
          ) : (
            <div className={streaming && i === sections.length - 1 ? "caret" : undefined}><Rich text={s.body} /></div>
          )}
        </section>
      ))}
    </>
  );
}

export function Sources({ list }: { list: { title: string; url: string }[] }) {
  if (!list.length) return null;
  return (
    <section className="sec sources">
      <h2>{t("來源")}</h2>
      <ol>
        {list.map((s) => (
          <li key={s.url}>
            <button className="link" onClick={() => openLink(s.url)} title={s.url}>
              {s.title !== s.url ? s.title : host(s.url)}
            </button>
            <span className="faint"> · {host(s.url)}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}

// ---------- follow-ups ----------

export type Exchange = {
  question: string;
  answer: string;
  sources: { title: string; url: string }[];
  state: "streaming" | "done" | "error";
  error?: string;
  /** What was asked, so a failed exchange can be asked again. */
  ask?: Ask;
};

export type Ask = { question: string } | { action: PhraseAction; phrase: string };

/** Follow-up exchanges for conversation `id`, starting from `initial` (already answered ones). */
export function useThread(id: number | null) {
  const [items, setItems] = useState<Exchange[]>([]);
  const run = useRef(0);
  const busy = items.some((x) => x.state === "streaming");

  const reset = useCallback((initial: [string, string][] = []) => {
    run.current++;
    setItems(initial.map(([question, answer]) => ({ question, answer, sources: [], state: "done" })));
  }, []);

  const ask = useCallback((a: Ask) => {
    if (id === null) return;
    const mine = ++run.current;
    const label = "question" in a ? a.question : a.phrase;
    setItems((xs) => [...xs, { question: label, answer: "", sources: [], state: "streaming", ask: a }]);
    const update = (f: (x: Exchange) => Exchange) =>
      setItems((xs) => (mine === run.current ? [...xs.slice(0, -1), f(xs[xs.length - 1])] : xs));
    followUp(id, a, (e: StreamEvent) => {
      if (e.kind === "question") update((x) => ({ ...x, question: e.text }));
      else if (e.kind === "delta") update((x) => ({ ...x, answer: x.answer + e.text }));
      else if (e.kind === "source") update((x) => ({ ...x, sources: [...x.sources, { title: e.title, url: e.url }] }));
      else if (e.kind === "done") update((x) => ({ ...x, state: "done" }));
      else if (e.kind === "error") update((x) => ({ ...x, state: "error", error: e.message }));
    }).catch((err) => update((x) => ({ ...x, state: "error", error: String(err) })));
  }, [id]);

  /** Drops a failed exchange and asks it again at the end. */
  const retry = useCallback((i: number) => {
    const failed = items[i];
    if (!failed?.ask || failed.state !== "error" || busy) return;
    setItems((xs) => xs.filter((_, j) => j !== i));
    ask(failed.ask);
  }, [items, busy, ask]);

  return { items, ask, retry, busy, reset };
}

export function Thread({ items, onRetry }: { items: Exchange[]; onRetry?: (i: number) => void }) {
  return (
    <>
      {items.map((x, i) => (
        <div className="ex" key={i}>
          <p className="ex-q">{x.question}</p>
          {x.state === "streaming" && !x.answer && <p className="ex-wait faint">{t("思考中…")}</p>}
          {x.answer && <div className="ex-a"><Answer text={x.answer} task="explain" streaming={x.state === "streaming"} /></div>}
          {x.state === "error" && (
            <div className="notice" role="alert">
              {x.error}
              {x.ask && onRetry && <div><button className="btn" onClick={() => onRetry(i)}>{t("重試")}</button></div>}
            </div>
          )}
          <Sources list={x.sources} />
        </div>
      ))}
    </>
  );
}

/** One-tap follow-ups that fit what was just done. */
export const CHIPS: Record<Task, string[]> = {
  translate: [N_("更口語"), N_("更正式"), N_("再給例句"), N_("換個說法")],
  explain: [N_("再簡單一點"), N_("舉個例子"), N_("整理成重點")],
  search: [N_("比較其他選項"), N_("價格與哪裡買")],
  custom: [N_("更簡短"), N_("再給一個版本"), N_("說明改了什麼")],
};
/** Keyed by the untranslated chip label; the question is sent in the interface language. */
const CHIP_QUESTION: Record<string, string> = {
  更口語: N_("把譯文改得更口語、更自然。"),
  更正式: N_("把譯文改成更正式的書面語氣。"),
  再給例句: N_("再給三個用到同樣說法的例句，附上翻譯。"),
  換個說法: N_("原文還有哪些常見的說法？說明語氣差別。"),
  再簡單一點: N_("用更簡單的話再解釋一次，像對完全不懂的人說明。"),
  舉個例子: N_("舉一個具體的例子說明。"),
  整理成重點: N_("把上面的內容整理成三到五個重點。"),
  比較其他選項: N_("和類似的選項比較優缺點。"),
  價格與哪裡買: N_("大概的價格是多少？可以在哪裡買到或取得？"),
  更簡短: N_("再精簡一點，保留重點。"),
  再給一個版本: N_("換一種寫法再給一個版本。"),
  說明改了什麼: N_("說明你做了哪些修改，以及為什麼。"),
};

export function FollowBar({ task, onAsk, busy, autoFocus = false }: {
  task: Task; onAsk: (a: Ask) => void; busy: boolean; autoFocus?: boolean;
}) {
  const [text, setText] = useState("");
  const send = () => {
    const q = text.trim();
    if (!q || busy) return;
    onAsk({ question: q });
    setText("");
  };
  return (
    <div className="follow">
      <div className="follow-chips">
        {CHIPS[task].map((c) => (
          <button key={c} className="chip sm" disabled={busy} onClick={() => onAsk({ question: t(CHIP_QUESTION[c] ?? c) })}>{t(c)}</button>
        ))}
      </div>
      <form className="follow-input" onSubmit={(e) => { e.preventDefault(); send(); }}>
        <input
          className="field" value={text} onChange={(e) => setText(e.target.value)} autoFocus={autoFocus}
          placeholder={t("追問這段內容…")} aria-label={t("追問")}
        />
        <button className="btn primary" type="submit" disabled={busy || !text.trim()}>{t("送出")}</button>
      </form>
    </div>
  );
}

// ---------- selected phrase ----------

/** Reads `text` aloud with the system voice, guessing the language from its script. */
export function speak(text: string) {
  const synth = window.speechSynthesis;
  if (!synth) return;
  const u = new SpeechSynthesisUtterance(text);
  u.lang = /[぀-ヿ]/.test(text) ? "ja-JP"
    : /[가-힯]/.test(text) ? "ko-KR"
    : /[一-鿿]/.test(text) ? "zh-TW"
    : /[Ѐ-ӿ]/.test(text) ? "ru-RU"
    : "en-US";
  synth.cancel();
  synth.speak(u);
}

type Pick = { phrase: string; x: number; y: number };

/** A small toolbar over text selected inside `root`: ask about it, hear it, copy it. */
export function SelectionMenu({ root, onAsk, busy }: {
  root: RefObject<HTMLElement | null>; onAsk: (a: Ask) => void; busy: boolean;
}) {
  const [pick, setPick] = useState<Pick | null>(null);
  const [copied, setCopied] = useState(false);
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const read = () => {
      const sel = window.getSelection();
      const phrase = sel?.toString().trim() ?? "";
      const el = root.current;
      if (!sel || !phrase || !el || sel.rangeCount === 0 || phrase.length > 400) return setPick(null);
      const range = sel.getRangeAt(0);
      if (!el.contains(range.commonAncestorContainer)) return setPick(null);
      const r = range.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      setCopied(false);
      setPick({
        phrase,
        x: Math.min(Math.max(r.left + r.width / 2, box.left + 120), box.right - 120),
        y: r.top - 8 < box.top + 40 ? r.bottom + 44 : r.top - 8,
      });
    };
    const onUp = (e: MouseEvent) => {
      if (menu.current?.contains(e.target as Node)) return;
      setTimeout(read, 0); // let a click inside the selection clear it first
    };
    const onKey = (e: KeyboardEvent) => { if (e.shiftKey || e.key === "Escape") setTimeout(read, 0); };
    const onScroll = () => setPick(null);
    document.addEventListener("mouseup", onUp);
    document.addEventListener("keyup", onKey);
    root.current?.addEventListener("scroll", onScroll);
    const el = root.current;
    return () => {
      document.removeEventListener("mouseup", onUp);
      document.removeEventListener("keyup", onKey);
      el?.removeEventListener("scroll", onScroll);
    };
  }, [root]);

  if (!pick) return null;
  const act = (action: PhraseAction) => {
    onAsk({ action, phrase: pick.phrase });
    window.getSelection()?.removeAllRanges();
    setPick(null);
  };
  return (
    <div
      ref={menu} className="sel-menu" role="toolbar" aria-label={t("選取的文字")}
      style={{ left: pick.x, top: pick.y }} onMouseDown={(e) => e.preventDefault()}
    >
      <button disabled={busy} onClick={() => act("explain")}>{t("解釋")}</button>
      <button disabled={busy} onClick={() => act("examples")}>{t("例句")}</button>
      <button disabled={busy} onClick={() => act("rephrase")}>{t("換個說法")}</button>
      <span className="sep" />
      <button onClick={() => speak(pick.phrase)} title={t("用系統語音念出來")}>{t("發音")}</button>
      <button
        onClick={async () => {
          await navigator.clipboard.writeText(pick.phrase).catch(() => {});
          setCopied(true);
          setTimeout(() => setPick(null), 700);
        }}
      >{copied ? t("已複製") : t("複製")}</button>
    </div>
  );
}

/** Keeps a scroll container pinned to the bottom while content streams in, unless the user scrolls up. */
export function useStickToBottom(ref: RefObject<HTMLElement | null>, dep: unknown) {
  const stick = useRef(true);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => { stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40; };
    el.addEventListener("scroll", onScroll);
    return () => el.removeEventListener("scroll", onScroll);
  }, [ref]);
  useEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [ref, dep]);
  return stick;
}
