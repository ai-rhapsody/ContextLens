import { useEffect, useMemo, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { cancelSnip, finishSnip, snipView, type Rect, type SnipView, type WinRect } from "../lib/api";

type Mode = "region" | "window" | "screen";

/** Full-screen overlay over a frozen screenshot: drag a region, click a window, or take the screen. */
export function Snip() {
  const [view, setView] = useState<SnipView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>("region");
  const [drag, setDrag] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const [hover, setHover] = useState<WinRect | null>(null);
  const [cursor, setCursor] = useState<{ x: number; y: number } | null>(null);
  const done = useRef(false);

  // Each monitor has its own overlay; a mode change on one switches them all.
  const pickMode = (m: Mode) => {
    setMode(m);
    setHover(null);
    emit("snip-mode", m).catch(() => {});
  };

  useEffect(() => {
    // The capture is stored before overlays open; retry briefly anyway in case one loads early.
    let alive = true;
    const load = (tries: number) => snipView().then((v) => alive && setView(v)).catch((e) => {
      if (alive && tries > 0 && String(e).includes("沒有進行中")) setTimeout(() => load(tries - 1), 100);
      else if (alive) setError(String(e));
    });
    load(10);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancelSnip();
      if (e.key === "1") pickMode("region");
      if (e.key === "2") pickMode("window");
      if (e.key === "3") pickMode("screen");
    };
    window.addEventListener("keydown", onKey);
    const un = listen<Mode>("snip-mode", ({ payload }) => { setMode(payload); setHover(null); });
    return () => {
      alive = false;
      window.removeEventListener("keydown", onKey);
      un.then((f) => f());
    };
  }, []);

  // CSS pixels to the screenshot's physical pixels.
  const k = useMemo(() => (view ? view.width / window.innerWidth : 1), [view]);

  const submit = (r: Rect, label: string | null = null) => {
    if (done.current) return;
    done.current = true;
    finishSnip(r, label).catch((e) => { done.current = false; setError(String(e)); });
  };

  const windowAt = (x: number, y: number) =>
    view?.windows.find((w) => x * k >= w.x && x * k < w.x + w.w && y * k >= w.y && y * k < w.y + w.h) ?? null;

  const onDown = (e: React.PointerEvent) => {
    if ((e.target as HTMLElement).closest(".snip-bar")) return;
    if (mode === "screen" && view) return submit({ x: 0, y: 0, w: view.width, h: view.height }, "全螢幕");
    if (mode === "window") {
      const w = windowAt(e.clientX, e.clientY);
      if (w) submit({ x: w.x, y: w.y, w: w.w, h: w.h }, w.title ? `視窗：${w.title}` : "視窗");
      return;
    }
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setDrag({ x0: e.clientX, y0: e.clientY, x1: e.clientX, y1: e.clientY });
  };

  const onMove = (e: React.PointerEvent) => {
    setCursor({ x: e.clientX, y: e.clientY });
    if (mode === "window") setHover(windowAt(e.clientX, e.clientY));
    if (drag) setDrag({ ...drag, x1: e.clientX, y1: e.clientY });
  };

  const onUp = () => {
    if (!drag) return;
    const r = { x: Math.min(drag.x0, drag.x1), y: Math.min(drag.y0, drag.y1), w: Math.abs(drag.x1 - drag.x0), h: Math.abs(drag.y1 - drag.y0) };
    setDrag(null);
    if (r.w > 6 && r.h > 6) submit({ x: r.x * k, y: r.y * k, w: r.w * k, h: r.h * k });
  };

  // The lit rectangle, in CSS pixels.
  const lit = (() => {
    if (drag) return { x: Math.min(drag.x0, drag.x1), y: Math.min(drag.y0, drag.y1), w: Math.abs(drag.x1 - drag.x0), h: Math.abs(drag.y1 - drag.y0) };
    if (mode === "window" && hover) return { x: hover.x / k, y: hover.y / k, w: hover.w / k, h: hover.h / k };
    if (mode === "screen") return { x: 0, y: 0, w: window.innerWidth, h: window.innerHeight };
    return null;
  })();

  if (error) {
    return (
      <div className="snip snip-error" onClick={() => cancelSnip()}>
        <p>無法擷取：{error}</p><p className="faint">按一下或 Esc 關閉</p>
      </div>
    );
  }
  if (!view) return <div className="snip" />;

  return (
    <div
      className={`snip mode-${mode}`} onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp}
      onPointerLeave={() => { setCursor(null); setHover(null); }}
    >
      <img className="snip-shot" src={view.preview} alt="" draggable={false} />
      {lit ? (
        <div className="snip-lit" style={{ left: lit.x, top: lit.y, width: lit.w, height: lit.h }}>
          {(drag || (mode === "window" && hover)) && (
            <span className="snip-size num">
              {mode === "window" && hover?.title ? `${hover.title} · ` : ""}
              {Math.round(lit.w * k)} × {Math.round(lit.h * k)}
            </span>
          )}
        </div>
      ) : (
        <div className="snip-dim" />
      )}
      {mode === "region" && !drag && cursor && (
        <>
          <i className="snip-cross h" style={{ top: cursor.y }} />
          <i className="snip-cross v" style={{ left: cursor.x }} />
        </>
      )}

      <div className="snip-bar" role="toolbar" aria-label="擷取模式">
        {([["region", "區域"], ["window", "視窗"], ["screen", "全螢幕"]] as const).map(([m, label], i) => (
          <button key={m} aria-pressed={mode === m} onClick={() => pickMode(m)}>
            {label}<kbd>{i + 1}</kbd>
          </button>
        ))}
        <span className="sep" />
        <span className="hint">
          {view.purpose === "live_target" && <strong>選擇要讓 Live 看的範圍 · </strong>}
          {mode === "region" ? "拖曳框選" : mode === "window" ? "點選一個視窗" : "按一下選取這個螢幕"}
        </span>
        <button onClick={() => cancelSnip()}>取消<kbd>Esc</kbd></button>
      </div>
    </div>
  );
}
