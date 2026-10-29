import { useEffect, useMemo, useRef, useState } from "react";
import { t } from "./i18n";

/**
 * Skeleton shown while the model thinks. Every bar paints a slice of one viewport-sized
 * aurora (background-attachment: fixed), so the bars read as slits onto the same moving light.
 * Three blobs drift on a random walk and their hues wander, so no two waits look alike.
 */

const HUES = [262, 228, 196, 318, 282, 172, 344];

type Blob = { x: number; y: number; vx: number; vy: number; h: number; vh: number };

function rand(min: number, max: number) {
  return min + Math.random() * (max - min);
}

function newBlobs(): Blob[] {
  const hues = [...HUES].sort(() => Math.random() - 0.5).slice(0, 3);
  return hues.map((h) => ({
    x: rand(0, 100), y: rand(0, 100),
    vx: rand(-0.25, 0.25), vy: rand(-0.25, 0.25),
    h: h + rand(-12, 12), vh: rand(-0.15, 0.15),
  }));
}

/**
 * Blob positions live in the skeleton's own box (0–100), but the gradients are painted in
 * viewport space (background-attachment: fixed), so convert. This keeps the light on the bars
 * instead of drifting into empty space below them.
 */
function vars(blobs: Blob[], box: DOMRect) {
  const out: Record<string, string> = {};
  const vw = window.innerWidth || 1;
  const vh = window.innerHeight || 1;
  // Only the part of the box that is on screen.
  const top = Math.max(box.top, 0);
  const height = Math.max(Math.min(box.bottom, vh) - top, 1);
  blobs.forEach((b, i) => {
    out[`--x${i}`] = `${(((box.left + (b.x / 100) * box.width) / vw) * 100).toFixed(2)}%`;
    out[`--y${i}`] = `${(((top + (b.y / 100) * height) / vh) * 100).toFixed(2)}%`;
    out[`--h${i}`] = b.h.toFixed(1);
  });
  return out;
}

/** Random line widths: lines in a paragraph run long, the last one stops short. */
function layout(groups: number): { head: number; lines: number[] }[] {
  return Array.from({ length: groups }, (_, g) => {
    const n = g === 0 ? Math.round(rand(2, 3)) : Math.round(rand(2, 4));
    const lines = Array.from({ length: n }, (_, i) => (i === n - 1 ? rand(32, 62) : rand(78, 100)));
    return { head: rand(14, 26), lines };
  });
}

export function Thinking({ groups = 1, startedAt }: { groups?: number; startedAt: number }) {
  const root = useRef<HTMLDivElement>(null);
  const blobs = useRef<Blob[]>(newBlobs());
  const shape = useMemo(() => layout(groups), [groups]);
  const [seconds, setSeconds] = useState(0);

  useEffect(() => {
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const el = root.current;
    if (!el) return;
    const paint = () =>
      Object.entries(vars(blobs.current, el.getBoundingClientRect())).forEach(([k, v]) => el.style.setProperty(k, v));
    paint();
    if (reduce) return;
    let raf = 0;
    const step = () => {
      for (const b of blobs.current) {
        // Brownian nudge, gentle damping, soft walls.
        b.vx = (b.vx + rand(-0.03, 0.03)) * 0.985;
        b.vy = (b.vy + rand(-0.03, 0.03)) * 0.985;
        b.vh = (b.vh + rand(-0.01, 0.01)) * 0.99;
        b.x += b.vx; b.y += b.vy; b.h += b.vh;
        if (b.x < -10 || b.x > 110) b.vx *= -1;
        if (b.y < -10 || b.y > 110) b.vy *= -1;
      }
      paint();
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, []);

  useEffect(() => {
    const t = setInterval(() => setSeconds(Math.floor((Date.now() - startedAt) / 1000)), 500);
    return () => clearInterval(t);
  }, [startedAt]);

  return (
    <div className="thinking" ref={root} role="status" aria-label={t("思考中")}>
      {shape.map((g, i) => (
        <div className="sk-group" key={i}>
          {groups > 1 && <i className="sk-bar sk-head" style={{ width: `${g.head}%` }} />}
          {g.lines.map((w, j) => (
            <i className="sk-bar" key={j} style={{ width: `${w}%`, animationDelay: `${rand(0, 1.6).toFixed(2)}s` }} />
          ))}
        </div>
      ))}
      <p className="sk-note num">{seconds >= 2 ? t("思考中 · {seconds} 秒", { seconds }) : t("思考中")}</p>
    </div>
  );
}
