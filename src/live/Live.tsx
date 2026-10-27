import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { closeLive, liveInfo, livePickScreen, liveStart, liveStop, showSettings, type LiveEvent, type LiveInfo, type LiveSource } from "../lib/api";

type State = "idle" | "connecting" | "listening" | "reconnecting" | "paused" | "stopped" | "error";
type Turn = { input: string; output: string };

const STATE_LABEL: Record<State, string> = {
  idle: "未開始", connecting: "連線中…", listening: "聆聽中", reconnecting: "重新連線中…",
  paused: "已暫停", stopped: "已停止", error: "發生錯誤",
};

const NOTE = "註：";

function clock(s: number) {
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

/** Translation lines, with tutor notes set apart. */
function Output({ text }: { text: string }) {
  return (
    <>
      {text.split("\n").filter((l) => l.trim()).map((line, i) =>
        line.trim().startsWith(NOTE)
          ? <p className="lv-note" key={i}>{line.trim()}</p>
          : <p className="lv-out" key={i}>{line}</p>,
      )}
    </>
  );
}

export function Live() {
  const [info, setInfo] = useState<LiveInfo | null>(null);
  const [state, setState] = useState<State>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [sources, setSources] = useState<LiveSource[]>(["mic"]);
  const [tutor, setTutor] = useState(false);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [current, setCurrent] = useState<Turn>({ input: "", output: "" });
  const [level, setLevel] = useState(0);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [now, setNow] = useState(Date.now());
  const body = useRef<HTMLDivElement>(null);
  const cur = useRef<Turn>({ input: "", output: "" }); // mirrors `current` for the event handler
  const stick = useRef(true); // follow new captions unless the user scrolled up

  const refresh = useCallback(async () => {
    const i = await liveInfo();
    setInfo(i);
    setSources(i.sources.length ? i.sources : ["mic"]);
    setTutor(i.tutor);
    if (!i.running) setState((s) => (s === "listening" || s === "connecting" || s === "reconnecting" ? "stopped" : s));
  }, []);

  useEffect(() => {
    refresh();
    const unLive = listen<LiveEvent>("live", ({ payload: e }) => {
      if (e.kind === "status") {
        setState(e.state);
        setMessage(e.message);
        if (e.state === "listening") setStartedAt((t) => t ?? Date.now());
        if (e.state === "error" || e.state === "paused" || e.state === "stopped") setLevel(0);
      } else if (e.kind === "input" || e.kind === "output") {
        cur.current = { ...cur.current, [e.kind]: cur.current[e.kind] + e.text };
        setCurrent(cur.current);
      } else if (e.kind === "turn_complete") {
        const done = cur.current;
        if (done.input.trim() || done.output.trim()) setTurns((t) => [...t.slice(-200), done]);
        cur.current = { input: "", output: "" };
        setCurrent(cur.current);
      } else if (e.kind === "level") {
        setLevel(e.value);
      }
    });
    const unEnded = listen("live-ended", () => { setLevel(0); setStartedAt(null); refresh(); });
    const unChanged = listen("live-changed", refresh);
    const unFocus = listen("tauri://focus", refresh);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeLive();
    window.addEventListener("keydown", onKey);
    return () => {
      unLive.then((f) => f());
      unEnded.then((f) => f());
      unChanged.then((f) => f());
      unFocus.then((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, [refresh]);

  useEffect(() => {
    if (!startedAt) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [startedAt]);

  useEffect(() => {
    if (stick.current && body.current) body.current.scrollTop = body.current.scrollHeight;
  }, [turns, current]);

  const running = state === "connecting" || state === "listening" || state === "reconnecting";

  const start = async (src = sources, tut = tutor) => {
    setMessage(null);
    setState("connecting");
    setStartedAt(null);
    try {
      await liveStart(src, tut);
    } catch (e) {
      setState("error");
      setMessage(String(e));
    }
  };

  const stop = async () => {
    await liveStop();
    setState("stopped");
    setStartedAt(null);
  };

  const toggleScreen = () => {
    if (sources.includes("screen")) return toggleSource("screen");
    // Turning the screen on always starts with choosing what to watch; the backend
    // restarts the session with the screen added once a target is picked.
    setMessage(null);
    livePickScreen().catch((e) => { setState("error"); setMessage(String(e)); });
  };

  const toggleSource = (s: LiveSource) => {
    const next = sources.includes(s) ? sources.filter((x) => x !== s) : [...sources, s];
    setSources(next);
    // Changing what we listen to restarts the session with the new sources.
    if (running && next.length) start(next, tutor);
    if (running && !next.length) stop();
  };

  const toggleTutor = () => {
    const next = !tutor;
    setTutor(next);
    if (running) start(sources, next);
  };

  const elapsed = startedAt ? Math.max(0, Math.floor((now - startedAt) / 1000)) : 0;
  const empty = !turns.length && !current.input && !current.output;

  return (
    <div className="live">
      <div className="lv-head" data-tauri-drag-region>
        <span className={`lv-dot s-${state}`} aria-hidden />
        <span className="lv-state" data-tauri-drag-region>{STATE_LABEL[state]}</span>
        {startedAt && <span className="lv-clock num" title="已開啟時間，Live 會持續計費">{clock(elapsed)}</span>}
        <span className="lv-meter" aria-label="音量"><i style={{ width: `${Math.round(level * 100)}%` }} /></span>
        <span className="grow" data-tauri-drag-region />
        <div className="lv-sources" role="group" aria-label="聲音來源">
          <button aria-pressed={sources.includes("mic")} onClick={() => toggleSource("mic")}>麥克風</button>
          <button
            aria-pressed={sources.includes("system")} onClick={() => toggleSource("system")}
            disabled={!info?.system_audio} title={info?.system_audio ? "錄製電腦正在播放的聲音" : "系統聲音目前只支援 Windows"}
          >系統聲音</button>
          <button
            aria-pressed={sources.includes("screen")} onClick={toggleScreen}
            title={sources.includes("screen") ? `正在看：${info?.screen ?? ""}` : "選擇要讓 Live 看的區域、視窗或螢幕"}
          >螢幕</button>
        </div>
        <label className="lv-tutor">
          註解
          <button className="switch" role="switch" aria-checked={tutor} aria-label="教學註解" onClick={toggleTutor} />
        </label>
        {running
          ? <button className="btn" onClick={stop}>停止</button>
          : <button className="btn primary" onClick={() => start()} disabled={!info?.model_ok || !sources.length}>開始</button>}
        <button className="btn quiet" onClick={() => closeLive()} aria-label="關閉並停止" title="關閉並停止（Esc）">✕</button>
      </div>

      <div
        className="lv-body" ref={body}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
      >
        {message && <div className={`lv-msg${state === "error" ? " err" : ""}`} role="status">{message}</div>}

        {sources.includes("screen") && info?.screen && (
          <div className="lv-screen">
            <span className="faint">
              {sources.some((s) => s !== "screen") ? "畫面作為上下文" : "翻譯畫面上的新文字"} · 正在看：
            </span>
            <span className="lv-target">{info.screen}</span>
            <button className="btn quiet" onClick={() => livePickScreen()}>重新選擇</button>
          </div>
        )}

        {info && !info.model_ok && (
          <div className="lv-msg err">
            {info.model} 不是 Live 模型。
            <button className="btn" onClick={() => showSettings()}>到設定更改</button>
          </div>
        )}

        {empty && !running && !message && info?.model_ok && (
          <p className="lv-hint">
            選好聲音來源後按「開始」。使用 <span className="num">{info.model}</span>，開啟期間會持續計費，
            一段時間沒有聲音會自動暫停。
          </p>
        )}
        {empty && state === "listening" && <p className="lv-hint">正在聆聽…說話或播放聲音就會開始翻譯。</p>}

        {turns.map((t, i) => (
          <div className="lv-turn" key={i}>
            {t.input && <p className="lv-in">{t.input.trim()}</p>}
            <Output text={t.output} />
          </div>
        ))}
        {(current.input || current.output) && (
          <div className="lv-turn now">
            {current.input && <p className="lv-in">{current.input.trim()}</p>}
            <Output text={current.output} />
          </div>
        )}
      </div>
    </div>
  );
}
