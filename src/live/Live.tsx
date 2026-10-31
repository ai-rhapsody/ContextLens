import { useCallback, useEffect, useRef, useState } from "react";
import { inline } from "../lib/Rich";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isNote, toSrt, toText, type Caption } from "../lib/captions";
import { closeLive, liveFollowUp, liveInfo, livePickScreen, liveSetProvider, liveStar, liveStart, liveStop, PROVIDERS, showSettings,
  type Provider, type LiveEvent, type LiveInfo, type LiveSource } from "../lib/api";
import { locale, N_, t } from "../lib/i18n";
import {
  background, COLORS, floating, loadLook, nextScale, outlined, saveLook, SCALES, subtitleTextHeight, textColor, withBackground,
  type Look,
} from "./look";

type State = "idle" | "connecting" | "listening" | "reconnecting" | "paused" | "stopped" | "error";
/** A caption line, or (with `divider`) the start of a later session. */
type Turn = Caption & { starred?: boolean };

/** The key that reaches the bar while clicks pass through it; Linux cannot read one (tray instead). */
const PEEK_KEY = /Mac/.test(navigator.userAgent) ? "⌘" : /Windows/.test(navigator.userAgent) ? "Ctrl" : null;

const STATE_LABEL: Record<State, string> = {
  idle: N_("未開始"), connecting: N_("連線中…"), listening: N_("聆聽中"), reconnecting: N_("重新連線中…"),
  paused: N_("已暫停"), stopped: N_("已停止"), error: N_("發生錯誤"),
};

function clock(s: number) {
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

/** Translation lines, with tutor notes set apart. */
function Output({ text }: { text: string }) {
  return (
    <>
      {text.split("\n").filter((l) => l.trim()).map((line, i) =>
        isNote(line.trim())
          ? <p className="lv-note" key={i}>{inline(line.trim())}</p>
          : <p className="lv-out" key={i}>{inline(line)}</p>,
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
  const [look, setLook] = useState<Look>(loadLook);
  const [lookOpen, setLookOpen] = useState(false);
  const [tip, setTip] = useState<string | null>(null); // a short hint, gone by itself
  const [peek, setPeek] = useState(false); // pass-through is on, but the peek key gave the bar the mouse
  const lookRef = useRef(look);
  lookRef.current = look;
  const lookOpenRef = useRef(false);
  lookOpenRef.current = lookOpen;
  const body = useRef<HTMLDivElement>(null);
  const top = useRef<HTMLDivElement>(null);
  // The bar's size before subtitle mode sized it, to go back to when subtitle mode ends.
  const normalSize = useRef<[number, number] | null>(null);
  const cur = useRef<Turn>({ input: "", output: "" }); // mirrors `current` for the event handler
  const stick = useRef(true); // follow new captions unless the user scrolled up
  const lastState = useRef<State>("idle");

  const refresh = useCallback(async () => {
    const i = await liveInfo();
    setInfo(i);
    setSources(i.sources.length ? i.sources : ["mic"]);
    setTutor(i.tutor);
    if (!i.running) setState((s) => (s === "listening" || s === "connecting" || s === "reconnecting" ? "stopped" : s));
  }, []);

  useEffect(() => {
    refresh();
    // A session can end mid-line (the interpreter only ends a line when the next one starts):
    // keep that line with its own session instead of carrying it into the next.
    const settle = () => {
      const done = { ...cur.current, end: Date.now() };
      if (done.input.trim() || done.output.trim()) setTurns((t) => [...t.slice(-200), done]);
      cur.current = { input: "", output: "" };
      setCurrent(cur.current);
    };
    const unLive = listen<LiveEvent>("live", ({ payload: e }) => {
      if (e.kind === "status") {
        // A new session (not a reconnect or a source change): set the old captions apart.
        const was = lastState.current;
        lastState.current = e.state;
        if (e.state === "connecting") settle();
        if (e.state === "connecting" && ["idle", "stopped", "paused", "error"].includes(was)) {
          const at = new Date().toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
          setTurns((t) => (t.length && !t[t.length - 1].divider ? [...t, { input: "", output: "", divider: at }] : t));
        }
        setState(e.state);
        setMessage(e.message);
        if (e.state === "listening") setStartedAt((t) => t ?? Date.now());
        if (e.state === "error" || e.state === "paused" || e.state === "stopped") setLevel(0);
      } else if (e.kind === "input" || e.kind === "output") {
        cur.current = { ...cur.current, [e.kind]: cur.current[e.kind] + e.text, start: cur.current.start ?? Date.now() };
        setCurrent(cur.current);
      } else if (e.kind === "turn_complete") {
        settle();
      } else if (e.kind === "level") {
        setLevel(e.value);
      }
    });
    const unEnded = listen("live-ended", () => { settle(); setLevel(0); setStartedAt(null); refresh(); });
    const unChanged = listen("live-changed", refresh);
    const unFocus = listen("tauri://focus", refresh);
    const unPeek = listen<boolean>("live-peek", ({ payload }) => setPeek(payload));
    // The tray menu's switch, the way back where the peek key cannot be read (Linux).
    const unThrough = listen("live-through-toggle", () => togglePassthroughRef.current());
    // Esc closes the appearance controls first, then Live.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (lookOpenRef.current) setLookOpen(false);
      else closeLive();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      unLive.then((f) => f());
      unEnded.then((f) => f());
      unChanged.then((f) => f());
      unFocus.then((f) => f());
      unPeek.then((f) => f());
      unThrough.then((f) => f());
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

  // The window itself is transparent; the page paints the background at the chosen opacity.
  useEffect(() => {
    document.documentElement.classList.add("live-window");
  }, []);

  useEffect(() => {
    saveLook(look);
    // A see-through bar would otherwise keep the window's drop shadow around the empty area.
    getCurrentWindow().setShadow(look.bg >= 100).catch(() => {});
  }, [look]);

  const running = state === "connecting" || state === "listening" || state === "reconnecting";
  // What the chosen provider (or, on automatic, any provider with a key) can do.
  const usable = (info?.options ?? []).filter((o) => !info?.chosen || o.provider === info.chosen);
  const canAudio = usable.some((o) => o.audio);
  const canScreen = usable.some((o) => o.screen);
  const noKeys = info !== null && info.options.length === 0;
  const audioWhy = canAudio ? "" : t("聽聲音需要 Gemini 或 ChatGPT 的即時模型。到設定的「模型與金鑰」填寫。");
  const name = (p: Provider) => PROVIDERS.find((x) => x.id === p)?.name ?? p;

  const chooseProvider = async (value: string) => {
    setMessage(null);
    try {
      await liveSetProvider(value ? (value as Provider) : null);
    } catch (e) {
      setState("error");
      setMessage(String(e));
    }
    refresh();
  };

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

  // Audio sources the current provider cannot serve are left out rather than failing the start.
  const effective = sources.filter((s) => (s === "screen" ? canScreen : canAudio));

  const elapsed = startedAt ? Math.max(0, Math.floor((now - startedAt) / 1000)) : 0;
  const ask = (t: Turn) => {
    liveFollowUp(t.input.trim(), t.output.trim()).catch((e) => setMessage(String(e)));
  };
  const star = (i: number) => {
    const t = turns[i];
    if (t.starred) return;
    setTurns((ts) => ts.map((x, j) => (j === i ? { ...x, starred: true } : x)));
    liveStar(t.input.trim(), t.output.trim()).catch((e) => {
      setTurns((ts) => ts.map((x, j) => (j === i ? { ...x, starred: false } : x)));
      setMessage(String(e));
    });
  };

  const empty = !turns.length && !current.input && !current.output;

  const restyle = (change: Partial<Look>) => setLook((l) => ({ ...l, ...change }));
  const tipTimer = useRef<number>();
  const showTip = (text: string | null) => {
    setTip(text);
    window.clearTimeout(tipTimer.current);
    if (text) tipTimer.current = window.setTimeout(() => setTip(null), 7000);
  };
  /** Sizes the bar for subtitles, once, on a click (never while captions come in): tall enough for
   * one subtitle at this size under the controls, which show over the top on hover, and for an
   * open appearance row; with `widen`, also wide like a video. `grow` only ever makes it bigger.
   * The bottom edge stays put, unless the click came from the controls at the top (`keepTop`). */
  const fitSubtitle = (l: Look, { widen = false, grow = false, keepTop = false } = {}) => {
    const el = top.current;
    const controls = el
      ? [...el.children].filter((c) => !c.classList.contains("lv-look")).reduce((h, c) => h + (c as HTMLElement).offsetHeight, 0)
      : 0;
    const height = Math.max(controls + subtitleTextHeight(l) + 24, (el?.offsetHeight ?? 0) + 8);
    if (grow && height <= window.innerHeight) return Promise.resolve(null);
    return invoke<[number, number] | null>("live_resize", { width: null, height, widen, keepTop }).catch(() => null);
  };
  const enterSubtitle = (l: Look) => fitSubtitle(l, { widen: true }).then((before) => { normalSize.current = before; });
  const leaveSubtitle = () => {
    // Opened in subtitle mode, the bar has no earlier size: back to the one it starts with.
    const [width, height] = normalSize.current ?? [760, 240];
    normalSize.current = null;
    invoke("live_resize", { width, height, widen: false, keepTop: false }).catch(() => {});
  };
  /** The caption size or the original line changed: in subtitle mode the bar fits the new text. */
  const restyleText = (change: Partial<Look>) => {
    const next = { ...look, ...change };
    restyle(change);
    if (next.subtitle) fitSubtitle(next, { keepTop: true });
  };
  const toggleSubtitle = () => {
    const on = !look.subtitle;
    restyle({ subtitle: on });
    if (on) enterSubtitle({ ...look, subtitle: true });
    else leaveSubtitle();
    showTip(on ? t("字幕模式：滑鼠移到字幕上才會出現按鈕。拖曳上方可以移動，拉邊角可以縮放。") : null);
  };
  const togglePassthrough = () => {
    const on = !lookRef.current.passthrough;
    // Pass-through belongs to subtitle mode, so switching it on (from the tray too) enters it.
    restyle(on ? { passthrough: true, subtitle: true } : { passthrough: false });
    if (on && !lookRef.current.subtitle) enterSubtitle({ ...lookRef.current, subtitle: true });
    showTip(!on ? null : PEEK_KEY
      ? t("滑鼠穿透：點擊會直接穿過字幕。按住 {key} 把滑鼠移到字幕上，按鈕就會出現。", { key: PEEK_KEY })
      : t("滑鼠穿透：點擊會直接穿過字幕。要使用按鈕，從系統匣選單關閉滑鼠穿透。"));
  };
  const togglePassthroughRef = useRef(togglePassthrough);
  togglePassthroughRef.current = togglePassthrough;
  // Clicks pass through only while captions run in subtitle mode; a stopped bar takes them again.
  const through = look.subtitle && look.passthrough && running;
  // Floating controls hide while the mouse is away, but only while captions run: a stopped or
  // empty bar keeps them, so a see-through window never leaves nothing to click.
  const autohide = floating(look) && running;

  useEffect(() => {
    invoke("live_passthrough", { on: through }).catch(() => {});
    if (!through) setPeek(false);
  }, [through]);
  // Opening in subtitle mode sizes the bar for it once, when the controls are in (the service row
  // comes with the first info). The bar is still hidden then, so only its size changes.
  const fittedOnOpen = useRef(false);
  useEffect(() => {
    if (!info || fittedOnOpen.current) return;
    fittedOnOpen.current = true;
    if (lookRef.current.subtitle) fitSubtitle(lookRef.current, { widen: true });
  }, [info]);
  // The appearance row must fit in a short subtitle bar.
  useEffect(() => {
    if (lookOpen && lookRef.current.subtitle) fitSubtitle(lookRef.current, { grow: true, keepTop: true });
  }, [lookOpen]);
  const color = textColor(look);
  const customColor = !COLORS.some((c) => c.id === look.color);
  // Subtitle mode shows one sentence at a time, like video subtitles: the one being translated, or
  // until its translation starts, the last finished one (with the new original heard so far).
  const lines = turns.map((turn, i) => ({ turn, i })).filter(({ turn }) => !look.subtitle || !turn.divider);
  const shown = look.subtitle ? (current.output.trim() ? [] : lines.slice(-1)) : lines;

  /** Saves the captions so far (and the line in progress) to the Downloads folder. */
  const exportAs = async (ext: "txt" | "srt") => {
    const all: Caption[] = current.input || current.output ? [...turns, { ...current, end: Date.now() }] : turns;
    const content = ext === "srt" ? toSrt(all) : toText(all);
    if (!content.trim()) return setMessage(t("還沒有可以匯出的字幕。"));
    const d = new Date();
    const stamp = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`
      + ` ${String(d.getHours()).padStart(2, "0")}${String(d.getMinutes()).padStart(2, "0")}`;
    try {
      const path = await invoke<string>("save_captions", { stem: `ContextLens Live ${stamp}`, ext, content });
      setMessage(t("已匯出到 {path}", { path }));
    } catch (e) {
      setMessage(String(e));
    }
  };

  return (
    <div
      className={`live${look.subtitle ? " subtitle" : ""}${outlined(look) ? " outlined" : ""}${floating(look) ? " floating" : ""}`
        + `${autohide ? " autohide" : ""}${through ? " through" : ""}${peek ? " peek" : ""}${look.original ? "" : " no-original"}`}
      style={{ "--lv-scale": look.scale, "--lv-bg": `${background(look)}%`, ...(color ? { "--lv-color": color } : {}) } as React.CSSProperties}
    >
      {/* Controls. Over a see-through background they float above the captions and show only while
          the mouse is over the bar, so a subtitle strip is all text the rest of the time. */}
      <div className="lv-top" ref={top}>
      <div className="lv-head" data-tauri-drag-region>
        <span className={`lv-dot s-${state}`} aria-hidden />
        <span className="lv-state" data-tauri-drag-region>{t(STATE_LABEL[state])}</span>
        {startedAt && <span className="lv-clock num" title={t("已開啟時間，Live 會持續計費")}>{clock(elapsed)}</span>}
        <span className="lv-meter" aria-label={t("音量")}><i style={{ width: `${Math.round(level * 100)}%` }} /></span>
        <span className="grow" data-tauri-drag-region />
        <div className="lv-sources" role="group" aria-label={t("聲音來源")}>
          <button
            aria-pressed={sources.includes("mic") && canAudio} onClick={() => toggleSource("mic")}
            disabled={!canAudio} title={audioWhy || t("聽麥克風")}
          >{t("麥克風")}</button>
          <button
            aria-pressed={sources.includes("system") && canAudio} onClick={() => toggleSource("system")}
            disabled={!info?.system_audio || !canAudio}
            title={!info?.system_audio ? t("系統聲音目前只支援 Windows") : audioWhy || t("錄製電腦正在播放的聲音")}
          >{t("系統聲音")}</button>
          <button
            aria-pressed={sources.includes("screen")} onClick={toggleScreen} disabled={!canScreen}
            title={!canScreen ? t("看螢幕需要一個能讀圖片的模型和金鑰")
              : sources.includes("screen") ? t("正在看：{target}", { target: info?.screen ?? "" }) : t("選擇要讓 Live 看的區域、視窗或螢幕")}
          >{t("螢幕")}</button>
        </div>
        <label className="lv-tutor">
          <span className="lv-tutor-text">{t("註解")}</span>
          <button className="switch" role="switch" aria-checked={tutor} aria-label={t("教學註解")} onClick={toggleTutor} />
        </label>
        {running
          ? <button className="btn" onClick={stop}>{t("停止")}</button>
          : (
            <button
              className="btn primary" onClick={() => start(effective)}
              disabled={!effective.length}
              title={!effective.length ? t("先開一個來源") : undefined}
            >{t("開始")}</button>
          )}
        <button
          className={`btn lv-look-btn${look.subtitle ? " on" : ""}`} aria-pressed={look.subtitle} aria-label={t("字幕模式")}
          onClick={toggleSubtitle} title={t("字幕模式：只留最新的字幕疊在畫面上，可以直接當影片字幕")}
        >CC</button>
        {look.subtitle && (
          <button
            className={`btn lv-look-btn${look.passthrough ? " on" : ""}`} aria-pressed={look.passthrough} aria-label={t("滑鼠穿透")}
            onClick={togglePassthrough}
            title={PEEK_KEY
              ? t("滑鼠穿透：點擊會穿過字幕，點到後面的影片。按住 {key} 移到字幕上可以叫出按鈕", { key: PEEK_KEY })
              : t("滑鼠穿透：點擊會穿過字幕，點到後面的影片。從系統匣選單可以關閉")}
          >
            <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
              <path d="M3 1.5v11.2l3-2.9 2 4.7 2-.9-2-4.6h4.2L3 1.5Z" fill="currentColor" />
            </svg>
          </button>
        )}
        <button
          className={`btn lv-look-btn${lookOpen ? " on" : ""}`} aria-expanded={lookOpen} aria-label={t("外觀")}
          onClick={() => setLookOpen((v) => !v)} title={t("字幕大小、顏色和背景透明度")}
        >Aa</button>
        <button
          className="btn quiet" onClick={() => { setTurns([]); cur.current = { input: "", output: "" }; setCurrent(cur.current); }}
          disabled={!turns.length && !current.input && !current.output} title={t("清除字幕（已存進歷史紀錄的不受影響）")}
        >{t("清除")}</button>
        <button className="btn quiet" onClick={() => closeLive()} aria-label={t("關閉並停止")} title={t("關閉並停止（Esc）")}>✕</button>
      </div>

      {info && !noKeys && (
        <div className="lv-sub">
          <span className="faint">{t("服務")}</span>
          <select
            className="field" value={info.chosen ?? ""} aria-label={t("Live 使用的服務")}
            onChange={(e) => chooseProvider(e.target.value)}
          >
            <option value="">{t("自動")}</option>
            {info.options.map((o) => (
              <option key={o.provider} value={o.provider}>
                {name(o.provider)}{o.audio ? "" : t("（只能看螢幕）")}
              </option>
            ))}
          </select>
          {info.label && running && <span className="faint lv-label num">{t("正在使用 {label}", { label: info.label })}</span>}
          <span className="grow" />
          <select
            className="field" value="" aria-label={t("匯出字幕")} disabled={empty}
            onChange={(e) => { const f = e.target.value as "txt" | "srt"; e.target.value = ""; if (f) exportAs(f); }}
          >
            <option value="" disabled>{t("匯出…")}</option>
            <option value="txt">{t("文字檔（TXT）")}</option>
            <option value="srt">{t("字幕檔（SRT）")}</option>
          </select>
        </div>
      )}

      {lookOpen && (
        <div className="lv-look" role="group" aria-label={t("外觀")}>
          <span className="lv-look-item">
            <span className="faint">{t("大小")}</span>
            <button className="btn quiet" onClick={() => restyleText({ scale: nextScale(look.scale, -1) })}
              disabled={look.scale === SCALES[0]} title={t("字幕小一點")}>A−</button>
            <span className="num lv-look-val">{Math.round(look.scale * 100)}%</span>
            <button className="btn quiet" onClick={() => restyleText({ scale: nextScale(look.scale, 1) })}
              disabled={look.scale === SCALES[SCALES.length - 1]} title={t("字幕大一點")}>A+</button>
          </span>
          <label className="lv-look-item" title={t("降低後只剩文字疊在畫面上，可以直接當影片字幕")}>
            <span className="faint">{t("背景")}</span>
            <input type="range" min={0} max={100} step={5} value={background(look)}
              onChange={(e) => setLook((l) => withBackground(l, Number(e.target.value)))} aria-label={t("背景透明度")} />
            <span className="num lv-look-val">{background(look)}%</span>
          </label>
          <span className="lv-look-item" role="radiogroup" aria-label={t("字幕顏色")}>
            <span className="faint">{t("顏色")}</span>
            {COLORS.map((c) => (
              <button
                key={c.id} role="radio" aria-checked={look.color === c.id} title={t(c.label)}
                className={`lv-swatch${c.id === "auto" ? " auto" : ""}`}
                style={c.id === "auto" ? undefined : { background: c.id }}
                onClick={() => restyle({ color: c.id })}
              >{c.id === "auto" ? t("自動") : ""}</button>
            ))}
            <label
              className="lv-swatch pick" role="radio" aria-checked={customColor} title={t("自訂顏色")}
              style={customColor ? { background: look.color } : undefined}
            >
              <input
                type="color" aria-label={t("自訂顏色")}
                value={look.color === "auto" ? "#ffffff" : look.color}
                onChange={(e) => restyle({ color: e.target.value })}
              />
            </label>
          </span>
          <label className="lv-look-item">
            {t("顯示原文")}
            <button className="switch" role="switch" aria-checked={look.original} aria-label={t("顯示原文")}
              onClick={() => restyleText({ original: !look.original })} />
          </label>
        </div>
      )}
      </div>

      <div
        className="lv-body" ref={body}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
      >
        {tip && <div className="lv-msg lv-tip" role="status">{tip}</div>}

        {message && (
          // Stays in view while the captions scroll; closing it only hides it.
          <div className={`lv-msg sticky${state === "error" ? " err" : ""}`} role="status">
            <span className="grow">{message}</span>
            <button className="btn quiet" onClick={() => setMessage(null)} aria-label={t("關閉提示")}>✕</button>
          </div>
        )}

        {sources.includes("screen") && info?.screen && (
          <div className="lv-screen">
            <span className="faint">
              {sources.some((s) => s !== "screen") ? t("畫面作為上下文") : t("翻譯畫面上的新文字")} · {t("正在看：")}
            </span>
            <span className="lv-target">{info.screen}</span>
            <button className="btn quiet" onClick={() => livePickScreen()}>{t("重新選擇")}</button>
          </div>
        )}

        {noKeys && (
          <div className="lv-msg err">
            {t("還沒有任何服務的金鑰。")}
            <button className="btn" onClick={() => showSettings()}>{t("開啟設定")}</button>
          </div>
        )}

        {empty && !running && !message && !noKeys && (
          <p className="lv-hint">
            {canAudio
              ? t("選好來源後按「開始」。聽聲音時會持續計費，一段時間沒有聲音或畫面變化會自動暫停。")
              : t("目前只能看螢幕：按「螢幕」選一塊區域，畫面上出現新文字時就會翻譯。要聽聲音，請在「模型與金鑰」設定 Gemini 或 ChatGPT 的即時模型。")}
          </p>
        )}
        {empty && state === "listening" && <p className="lv-hint">{t("正在聆聽…說話或播放聲音就會開始翻譯。")}</p>}

        {shown.map(({ turn, i }) => turn.divider ? (
          <div className="lv-divider" key={i} role="separator"><span>{t("{time} 開始", { time: turn.divider })}</span></div>
        ) : (
          <div className={`lv-turn${turn.starred ? " starred" : ""}`} key={i}>
            {turn.input && <p className="lv-in">{turn.input.trim()}</p>}
            <Output text={turn.output} />
            <div className="lv-acts">
              <button onClick={() => ask(turn)} title={t("開對話視窗追問這一句，Live 繼續進行")}>{t("追問")}</button>
              <button
                onClick={() => star(i)} aria-pressed={!!turn.starred}
                title={turn.starred ? t("已存到歷史紀錄") : t("把這一句存到歷史紀錄")}
              >{turn.starred ? `★ ${t("已收藏")}` : `☆ ${t("收藏")}`}</button>
            </div>
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
