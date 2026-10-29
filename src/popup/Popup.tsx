import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  dataUrl, getSettings, hidePopup, modelForTask, openChat, providerInfo, PROVIDERS, runTranslation, setPopupPinned,
  showSettings, takePendingCapture, TASKS, type CustomAction, type Media, type Provider, type ProviderInfo, type Settings, type Task,
} from "../lib/api";
import { fileToMedia, KIND_LABEL, kindOf } from "../lib/files";
import { parseSections } from "../lib/sections";
import { Thinking } from "../lib/Thinking";
import { Answer, FollowBar, LEAD, QUERIES, SelectionMenu, Sources, Thread, useStickToBottom, useThread } from "../lib/convo";

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
  const [task, setTask] = useState<Task>("translate");
  const [sources, setSources] = useState<{ title: string; url: string }[]>([]);
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
  const [convo, setConvo] = useState<number | null>(null); // follow-ups go to this conversation
  const [fallback, setFallback] = useState<string | null>(null); // search answered without the web
  const [custom, setCustom] = useState<CustomAction | null>(null); // the hotkey's own action, as a tab
  const customRef = useRef<CustomAction | null>(null);
  customRef.current = custom;
  const thread = useThread(convo);
  const bodyRef = useRef<HTMLDivElement>(null);
  useStickToBottom(bodyRef, thread.items);
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

  const start = useCallback((next: Input, asTask: Task, asTutor: boolean, via: Provider | null = null) => {
    const id = ++run.current;
    setInput(next);
    setResult("");
    setSources([]);
    setError(null);
    setNeedsKey(false);
    setOverride(via);
    setBlocked(null);
    setConvo(null);
    setFallback(null);
    thread.reset();

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
    const action = asTask === "custom" ? customRef.current : null;
    runTranslation(next, asTask, asTutor, (e) => {
      if (id !== run.current) return;
      if (e.kind === "delta") setResult((r) => r + e.text);
      else if (e.kind === "source") setSources((s) => [...s, { title: e.title, url: e.url }]);
      else if (e.kind === "conversation") setConvo(e.id);
      else if (e.kind === "offline") { setFallback(e.reason); setResult(""); }
      else if (e.kind === "done") setStatus("done");
      else if (e.kind === "error") fail(e.message);
    }, via, action).catch((err) => id === run.current && fail(String(err)));
  }, [fail, thread.reset]);

  const pickUp = useCallback(async () => {
    const [cap, s, info] = await Promise.all([takePendingCapture(), getSettings(), providerInfo()]);
    setSettings(s);
    setPinned(s.popup_pinned);
    settingsRef.current = s;
    infoRef.current = info;
    if (!cap) return;
    setKept(false); // a fresh capture is a fresh peek
    setTutor(cap.tutor);
    setTask(cap.task);
    setCustom(cap.custom);
    customRef.current = cap.custom;
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
      setConvo(null);
      thread.reset();
      setStatus("idle");
      setKept(true);
      return;
    }
    start({ text: cap.text, media: cap.media }, cap.task, cap.tutor);
  }, [start, fail, thread.reset]);

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
      start({ text: "", media: await fileToMedia(file) }, task, tutor);
    } catch (e) {
      setInput(null);
      fail(e instanceof Error ? e.message : String(e));
    }
  }, [start, fail, task, tutor]);

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
    if (input) start(input, task, next, override);
  };

  const switchTask = (next: Task) => {
    if (next === task) return;
    setTask(next);
    setKept(true);
    if (input) start(input, next, tutor, override);
  };

  const togglePin = () => {
    const next = !pinned;
    setPinned(next);
    setPopupPinned(next).catch(() => {});
  };

  const sections = useMemo(() => parseSections(result), [result]);
  const hasHeadings = sections.some((s) => s.title);
  const lead = sections.find((s) => s.title === LEAD[task])?.body;

  const copy = async () => {
    const text = task === "search"
      ? sections.filter((s) => s.title !== QUERIES).map((s) => (s.title ? `${s.title}\n${s.body}` : s.body)).join("\n\n")
      : lead ?? result;
    await navigator.clipboard.writeText(text).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  const activeId = override ?? settings?.provider;
  const provider = PROVIDERS.find((p) => p.id === activeId);
  const streaming = status === "streaming";
  const media = input?.media ?? null;
  const kind = media ? kindOf(media.mime) : null;
  const skeletonGroups = task !== "translate" ? 3 : tutor ? 3 : media ? 2 : 1;
  const active = infoRef.current.find((p) => p.provider === activeId);
  const offline = task === "search" && active && !active.web;
  // Gemini sometimes searches without returning the pages it used.
  const noSources = task === "search" && status === "done" && !!active?.web && !fallback && !sources.length;

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
        <div className="tasks" role="tablist" aria-label="動作">
          {TASKS.map((t) => (
            <button key={t.id} role="tab" aria-selected={task === t.id} title={t.hint} onClick={() => switchTask(t.id)}>
              {t.label}
            </button>
          ))}
          {custom && (
            <button role="tab" aria-selected={task === "custom"} title={custom.instruction} onClick={() => switchTask("custom")}>
              {custom.name || "自訂"}
            </button>
          )}
        </div>
        <span className="grow" data-tauri-drag-region />
        {task === "translate" && (
          <span className="mode">
            教學
            <button className="switch" role="switch" aria-checked={tutor} aria-label="教學模式" onClick={toggleTutor} />
          </span>
        )}
        <button
          className={`btn quiet pin${pinned ? " on" : ""}`} onClick={togglePin} aria-pressed={pinned}
          aria-label="固定視窗位置"
          title={pinned
            ? "已固定：視窗留在這裡，新的結果在原處更新。按一下取消固定"
            : "未固定：視窗會出現在游標旁。按一下固定在目前位置"}
        >
          <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
            <path
              d="M6 1.5h4l-.6 4.2 2.6 2.3v1.5H8.75V14L8 15l-.75-1V9.5H4V8l2.6-2.3L6 1.5Z"
              fill={pinned ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"
            />
          </svg>
        </button>
        <button className="btn quiet" onClick={() => hidePopup()} aria-label="關閉" title="關閉（Esc）">✕</button>
      </div>

      <div className="pop-body" ref={bodyRef}>
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
                    <button key={p.provider} className="btn" onClick={() => input && start(input, task, tutor, p.provider)}>
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
            {(needsKey || input) && (
              <div className="alts">
                {input && !needsKey && (
                  <button className="btn" onClick={() => start(input, task, tutor, override)}>重試</button>
                )}
                {needsKey && <button className="btn" onClick={() => { showSettings(); hidePopup(); }}>開啟設定</button>}
              </div>
            )}
          </div>
        )}

        {status === "idle" && (
          <div className="dropzone">
            <p>反白文字或截圖後按熱鍵，或把圖片、音訊、影片拖到這裡。</p>
            <p className="faint">也可以按 Ctrl+V 貼上圖片。</p>
            <button className="btn" onClick={() => fileInput.current?.click()}>選擇檔案</button>
          </div>
        )}

        {showSkeleton && <Thinking groups={skeletonGroups} startedAt={startedAt} />}

        {status !== "error" && result && !hasHeadings && (
          <p className={`result${streaming ? " caret" : ""}`}>{result}</p>
        )}

        {(offline || fallback) && status !== "error" && (result || streaming) && (
          <p className="offline faint">{fallback ?? "這個模型無法上網，以下是模型本身的知識，可能不是最新資訊。"}</p>
        )}

        {status !== "error" && hasHeadings && <Answer text={result} task={task} streaming={streaming} />}

        <Sources list={sources} />
        {noSources && (
          <p className="offline faint">服務這次沒有回傳網頁來源，內容請用上面的搜尋關鍵字自行查證。</p>
        )}

        {status === "done" && convo !== null && (
          <>
            <Thread items={thread.items} onRetry={thread.retry} />
            <FollowBar task={task} busy={thread.busy} onAsk={(a) => { setKept(true); thread.ask(a); }} />
          </>
        )}
      </div>

      <div className="pop-foot">
        <span className="grow foot-status">
          {streaming && result ? <span>{task === "search" ? "搜尋中…" : "處理中…"} · </span> : null}
          <span className="faint">
            {provider ? `${provider.name} · ${modelForTask(settings!, provider.id, task)}${override ? "（這次）" : ""}` : ""}
          </span>
          {!pinned && kept && status !== "idle" && <span className="faint"> · Esc 關閉</span>}
        </span>
        <button className="btn quiet" onClick={() => fileInput.current?.click()} title="分析圖片、音訊或影片">檔案…</button>
        {convo !== null && status === "done" && (
          <button
            className="btn quiet" title="在對話視窗繼續討論"
            onClick={() => { openChat(convo); hidePopup(); }}
          >展開</button>
        )}
        <button className="btn" onClick={copy} disabled={!result}>
          {copied ? "已複製" : task === "translate" ? "複製譯文" : "複製"}
        </button>
        <span
          className="grip" aria-hidden
          onPointerDown={(e) => { e.preventDefault(); setKept(true); getCurrentWindow().startResizeDragging("SouthEast"); }}
        />
      </div>

      <input
        ref={fileInput} type="file" hidden accept="image/*,audio/*,video/*"
        onChange={(e) => { useFiles(e.target.files); e.target.value = ""; }}
      />
      {status === "done" && convo !== null && (
        <SelectionMenu root={bodyRef} busy={thread.busy} onAsk={(a) => { setKept(true); thread.ask(a); }} />
      )}
      {dragging && <div className="drop-veil">放開以{task === "custom" ? custom?.name || "自訂動作" : TASKS.find((t) => t.id === task)?.label}</div>}
    </div>
  );
}
