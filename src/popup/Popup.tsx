import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  getSettings, hidePopup, PROVIDERS, runTranslation, saveSettings, showSettings, takePendingCapture,
  type Settings,
} from "../lib/api";
import { parseSections } from "../lib/sections";
import { Rich } from "../lib/Rich";

type Status = "idle" | "streaming" | "done" | "error";

export function Popup() {
  const [source, setSource] = useState("");
  const [result, setResult] = useState("");
  const [tutor, setTutor] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState<string | null>(null);
  const [needsKey, setNeedsKey] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [copied, setCopied] = useState(false);
  const run = useRef(0); // identifies the current translation; stale events are dropped
  const pinnedRef = useRef(false);
  pinnedRef.current = pinned;

  const start = useCallback((text: string, asTutor: boolean) => {
    const id = ++run.current;
    setSource(text);
    setResult("");
    setError(null);
    setNeedsKey(false);
    setStatus("streaming");
    setExpanded(false);
    runTranslation(text, asTutor, (e) => {
      if (id !== run.current) return;
      if (e.kind === "delta") setResult((r) => r + e.text);
      else if (e.kind === "done") setStatus("done");
      else {
        setStatus("error");
        setError(e.message);
        setNeedsKey(e.message.includes("API 金鑰"));
      }
    }).catch((err) => {
      if (id !== run.current) return;
      setStatus("error");
      setError(String(err));
    });
  }, []);

  const pickUp = useCallback(async () => {
    const [cap, s] = await Promise.all([takePendingCapture(), getSettings()]);
    setSettings(s);
    if (!cap) return;
    setTutor(cap.tutor);
    if (cap.error) {
      run.current++;
      setSource("");
      setResult("");
      setStatus("error");
      setError(cap.error);
      setNeedsKey(false);
      return;
    }
    start(cap.text, cap.tutor);
  }, [start]);

  useEffect(() => {
    pickUp();
    const un = listen("capture", pickUp);
    const win = getCurrentWindow();
    const unFocus = win.onFocusChanged(({ payload: focused }) => {
      if (!focused && !pinnedRef.current) hidePopup();
    });
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && hidePopup();
    window.addEventListener("keydown", onKey);
    return () => {
      un.then((f) => f());
      unFocus.then((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, [pickUp]);

  const toggleTutor = () => {
    const next = !tutor;
    setTutor(next);
    if (source) start(source, next);
  };

  const makeDefaultTutor = async () => {
    if (!settings) return;
    const next = { ...settings, tutor_mode: tutor };
    setSettings(next);
    await saveSettings(next).catch(() => {});
  };

  const copy = async () => {
    const sections = parseSections(result);
    const text = tutor && sections[0]?.title === "翻譯" ? sections[0].body : result;
    await navigator.clipboard.writeText(text).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  const provider = PROVIDERS.find((p) => p.id === settings?.provider);
  const sections = useMemo(() => (tutor ? parseSections(result) : []), [tutor, result]);
  const streaming = status === "streaming";
  const showDefaultButton = settings !== null && settings.tutor_mode !== tutor;

  return (
    <div className="popup">
      <div className="pop-head" data-tauri-drag-region>
        <span className="where" data-tauri-drag-region>
          {provider ? `${provider.name} · ${settings!.models[provider.id]}` : "ContextLens"}
        </span>
        <span className="mode">
          教學模式
          <button className="switch" role="switch" aria-checked={tutor} aria-label="教學模式" onClick={toggleTutor} />
        </span>
        <button className="btn quiet" onClick={() => setPinned((v) => !v)} aria-pressed={pinned} title="固定視窗，失焦時不自動關閉">
          {pinned ? "已固定" : "固定"}
        </button>
        <button className="btn quiet" onClick={() => hidePopup()} aria-label="關閉">✕</button>
      </div>

      <div className="pop-body">
        {source && (
          <p className={`src${expanded ? " open" : ""}`} onClick={() => setExpanded((v) => !v)} title="點一下展開原文">
            {source}
          </p>
        )}

        {status === "error" && error && (
          <div className="notice" role="alert">
            {error}
            {needsKey && (
              <div><button className="btn" onClick={() => { showSettings(); hidePopup(); }}>開啟設定</button></div>
            )}
          </div>
        )}

        {status !== "error" && !tutor && (
          <p className={`result${streaming ? " caret" : ""}`}>{result}</p>
        )}

        {status !== "error" && tutor && sections.map((s, i) => (
          <section className={`sec${i === 0 && s.title === "翻譯" ? " lead" : ""}`} key={s.title ?? i}>
            {s.title && <h2>{s.title}</h2>}
            <div className={streaming && i === sections.length - 1 ? "caret" : undefined}><Rich text={s.body} /></div>
          </section>
        ))}

        {status === "idle" && <p className="muted">反白一段文字，按下熱鍵。</p>}
      </div>

      <div className="pop-foot">
        <span className="grow">
          {streaming ? "翻譯中…" : status === "done" ? (tutor ? "已完成 · 教學模式" : "已完成") : ""}
        </span>
        {showDefaultButton && (
          <button className="btn quiet" onClick={makeDefaultTutor}>
            設為預設{tutor ? "開啟" : "關閉"}
          </button>
        )}
        <button className="btn" onClick={copy} disabled={!result}>{copied ? "已複製" : "複製譯文"}</button>
      </div>
    </div>
  );
}
