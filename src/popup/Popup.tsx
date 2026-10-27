import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  dataUrl, getSettings, hidePopup, providerInfo, PROVIDERS, runTranslation, setPopupPinned, showSettings,
  takePendingCapture, type Media, type Provider, type ProviderInfo, type Settings,
} from "../lib/api";
import { fileToMedia, KIND_LABEL, kindOf } from "../lib/files";
import { parseSections } from "../lib/sections";
import { Rich } from "../lib/Rich";
import { Thinking } from "../lib/Thinking";

type Status = "idle" | "streaming" | "done" | "error";
type Input = { text: string; media: Media | null };
/** The default model cannot read this input; these providers can, and have a key. */
type Blocked = { kind: "image" | "audio" | "video"; model: string; alternatives: ProviderInfo[] };

/** Wait this long without a token before showing the skeleton, so fast answers never flash it. */
const SKELETON_DELAY_MS = 250;

export function Popup() {
  const [input, setInput] = useState<Input | null>(null);
  const [result, setResult] = useState("");
  const [tutor, setTutor] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState<string | null>(null);
  const [needsKey, setNeedsKey] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [kept, setKept] = useState(false); // touched since shown: blur no longer closes
  const [expanded, setExpanded] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [copied, setCopied] = useState(false);
  const [startedAt, setStartedAt] = useState(0);
  const [showSkeleton, setShowSkeleton] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [blocked, setBlocked] = useState<Blocked | null>(null);
  const [override, setOverride] = useState<Provider | null>(null); // this request only
  const infoRef = useRef<ProviderInfo[]>([]);
  const settingsRef = useRef<Settings | null>(null);
  const run = useRef(0); // identifies the current translation; stale events are dropped
  const keepRef = useRef(false);
  keepRef.current = pinned || kept;
  const fileInput = useRef<HTMLInputElement>(null);

  const fail = useCallback((message: string) => {
    run.current++;
    setStatus("error");
    setError(message);
    setNeedsKey(message.includes("API 金鑰"));
  }, []);

  const start = useCallback((next: Input, asTutor: boolean, via: Provider | null = null) => {
    const id = ++run.current;
    setInput(next);
    setResult("");
    setError(null);
    setNeedsKey(false);
    setOverride(via);
    setBlocked(null);

    // Check the capability table before sending, and offer providers that can read this input.
    const kind = next.media ? kindOf(next.media.mime) : null;
    const info = infoRef.current;
    const chosen = info.find((p) => p.provider === (via ?? settingsRef.current?.provider));
    if (kind && chosen && !chosen.caps[kind]) {
      setStatus("error");
      setBlocked({
        kind,
        model: `${PROVIDERS.find((p) => p.id === chosen.provider)?.name} · ${chosen.model}`,
        alternatives: info.filter((p) => p.caps[kind] && p.has_key),
      });
      return;
    }

    setStatus("streaming");
    setExpanded(false);
    setStartedAt(Date.now());
    setShowSkeleton(false);
    runTranslation(next, asTutor, (e) => {
      if (id !== run.current) return;
      if (e.kind === "delta") setResult((r) => r + e.text);
      else if (e.kind === "done") setStatus("done");
      else fail(e.message);
    }, via).catch((err) => id === run.current && fail(String(err)));
  }, [fail]);

  const pickUp = useCallback(async () => {
    const [cap, s, info] = await Promise.all([takePendingCapture(), getSettings(), providerInfo()]);
    setSettings(s);
    settingsRef.current = s;
    infoRef.current = info;
    if (!cap) return;
    setKept(false); // a fresh capture is a fresh peek
    setTutor(cap.tutor);
    if (cap.error) {
      setInput(null);
      setResult("");
      return fail(cap.error);
    }
    if (!cap.text && !cap.media) {
      // Opened from the tray: an empty window to drop files into.
      run.current++;
      setInput(null);
      setResult("");
      setStatus("idle");
      setKept(true);
      return;
    }
    start({ text: cap.text, media: cap.media }, cap.tutor);
  }, [start, fail]);

  useEffect(() => {
    pickUp();
    const un = listen("capture", pickUp);
    const unFocus = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused && !keepRef.current) hidePopup();
    });
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") hidePopup();
      else setKept(true);
    };
    window.addEventListener("keydown", onKey);
    return () => {
      un.then((f) => f());
      unFocus.then((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, [pickUp]);

  // The skeleton appears only if the first token is slow.
  useEffect(() => {
    if (status !== "streaming" || result) return setShowSkeleton(false);
    const t = setTimeout(() => setShowSkeleton(true), SKELETON_DELAY_MS);
    return () => clearTimeout(t);
  }, [status, result]);

  const useFiles = useCallback(async (files: FileList | File[] | null) => {
    const file = files?.[0];
    if (!file) return;
    setKept(true);
    try {
      start({ text: "", media: await fileToMedia(file) }, tutor);
    } catch (e) {
      setInput(null);
      fail(e instanceof Error ? e.message : String(e));
    }
  }, [start, fail, tutor]);

  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const files = e.clipboardData?.files;
      if (files && files.length) {
        e.preventDefault();
        useFiles(files);
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [useFiles]);

  const toggleTutor = () => {
    const next = !tutor;
    setTutor(next);
    setKept(true);
    if (input) start(input, next, override);
  };

  const togglePin = () => {
    const next = !pinned;
    setPinned(next);
    setPopupPinned(next).catch(() => {});
  };

  const sections = useMemo(() => parseSections(result), [result]);
  const hasHeadings = sections.some((s) => s.title);
  const translation = sections.find((s) => s.title === "翻譯")?.body;

  const copy = async () => {
    await navigator.clipboard.writeText(translation ?? result).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  const activeId = override ?? settings?.provider;
  const provider = PROVIDERS.find((p) => p.id === activeId);
  const streaming = status === "streaming";
  const media = input?.media ?? null;
  const kind = media ? kindOf(media.mime) : null;
  const skeletonGroups = tutor ? 3 : media ? 2 : 1;

  return (
    <div
      className={`popup${dragging ? " dropping" : ""}`}
      onPointerDown={() => setKept(true)}
      onWheel={() => setKept(true)}
      onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
      onDragLeave={(e) => e.currentTarget === e.target && setDragging(false)}
      onDrop={(e) => { e.preventDefault(); setDragging(false); useFiles(e.dataTransfer.files); }}
    >
      <div className="pop-head" data-tauri-drag-region>
        <span className="where" data-tauri-drag-region>
          {provider ? `${provider.name} · ${settings!.models[provider.id]}${override ? "（這次）" : ""}` : "ContextLens"}
        </span>
        <span className="mode">
          教學
          <button className="switch" role="switch" aria-checked={tutor} aria-label="教學模式" onClick={toggleTutor} />
        </span>
        <button
          className={`btn quiet${pinned ? " on" : ""}`} onClick={togglePin} aria-pressed={pinned}
          title={pinned ? "取消固定：新的翻譯會跳到游標旁" : "固定位置：新的翻譯會在這裡更新"}
        >
          {pinned ? "已固定" : "固定"}
        </button>
        <button className="btn quiet" onClick={() => hidePopup()} aria-label="關閉" title="關閉（Esc）">✕</button>
      </div>

      <div className="pop-body">
        {input?.text && !media && (
          <p className={`src${expanded ? " open" : ""}`} onClick={() => setExpanded((v) => !v)} title="點一下展開原文">
            {input.text}
          </p>
        )}

        {media && (
          <div className="src-media">
            {kind === "image" && <img src={dataUrl(media)} alt={media.name ?? "擷取的圖片"} />}
            {kind === "audio" && <audio controls src={dataUrl(media)} />}
            {kind === "video" && <video controls src={dataUrl(media)} />}
            <span className="faint">{media.name ?? (kind ? KIND_LABEL[kind] : "檔案")}</span>
          </div>
        )}

        {blocked && (
          <div className="notice soft" role="alert">
            <strong>{blocked.model}</strong> 不能讀取{KIND_LABEL[blocked.kind]}。
            {blocked.alternatives.length > 0 ? (
              <div className="alts">
                {blocked.alternatives.map((p) => {
                  const name = PROVIDERS.find((x) => x.id === p.provider)?.name;
                  return (
                    <button key={p.provider} className="btn" onClick={() => input && start(input, tutor, p.provider)}>
                      改用 {name} 處理這次
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="alts">
                <span>到設定換一個支援{KIND_LABEL[blocked.kind]}的模型，例如 Gemini。</span>
                <button className="btn" onClick={() => { showSettings(); hidePopup(); }}>開啟設定</button>
              </div>
            )}
          </div>
        )}

        {status === "error" && error && (
          <div className="notice" role="alert">
            {error}
            {needsKey && (
              <div><button className="btn" onClick={() => { showSettings(); hidePopup(); }}>開啟設定</button></div>
            )}
          </div>
        )}

        {status === "idle" && (
          <div className="dropzone">
            <p>反白文字按熱鍵，或把圖片、音訊、影片拖到這裡。</p>
            <p className="faint">也可以按 Ctrl+V 貼上圖片。</p>
            <button className="btn" onClick={() => fileInput.current?.click()}>選擇檔案</button>
          </div>
        )}

        {showSkeleton && <Thinking groups={skeletonGroups} startedAt={startedAt} />}

        {status !== "error" && result && !hasHeadings && (
          <p className={`result${streaming ? " caret" : ""}`}>{result}</p>
        )}

        {status !== "error" && hasHeadings && sections.map((s, i) => (
          <section
            key={s.title ?? i}
            className={`sec${s.title === "翻譯" ? " lead" : ""}${s.title === "原文" ? " orig" : ""}`}
          >
            {s.title && <h2>{s.title}</h2>}
            <div className={streaming && i === sections.length - 1 ? "caret" : undefined}><Rich text={s.body} /></div>
          </section>
        ))}
      </div>

      <div className="pop-foot">
        <span className="grow">
          {streaming ? (result ? "翻譯中…" : "") : status === "done" ? "已完成" : ""}
          {!pinned && kept && status !== "idle" && <span className="faint"> · 已保留，Esc 關閉</span>}
        </span>
        <button className="btn quiet" onClick={() => fileInput.current?.click()} title="翻譯圖片、音訊或影片">檔案…</button>
        <button className="btn" onClick={copy} disabled={!result}>{copied ? "已複製" : "複製譯文"}</button>
        <span
          className="grip" aria-hidden
          onPointerDown={(e) => { e.preventDefault(); setKept(true); getCurrentWindow().startResizeDragging("SouthEast"); }}
        />
      </div>

      <input
        ref={fileInput} type="file" hidden accept="image/*,audio/*,video/*"
        onChange={(e) => { useFiles(e.target.files); e.target.value = ""; }}
      />
      {dragging && <div className="drop-veil">放開以翻譯</div>}
    </div>
  );
}
