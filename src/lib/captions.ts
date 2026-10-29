import { N_, t, variants } from "./i18n";

/** A Live caption: what was heard, its translation, and when (ms since 1970) it was heard. */
export type Caption = { input: string; output: string; divider?: string; start?: number; end?: number };

const NOTE = N_("註：");

/** Whether an output line is a tutor note; the model writes the prefix in the interface language. */
export const isNote = (line: string) => variants(NOTE).some((p) => line.startsWith(p));

/** Translation lines without the tutor notes. */
function lines(output: string): string[] {
  return output.split("\n").map((l) => l.trim()).filter((l) => l && !isNote(l));
}

/** Plain text: each original above its translation and notes, sessions marked. */
export function toText(captions: Caption[]): string {
  const out: string[] = [];
  for (const c of captions) {
    if (c.divider) {
      out.push(t("—— {time} 開始 ——", { time: c.divider }), "");
      continue;
    }
    if (c.input.trim()) out.push(c.input.trim());
    out.push(...c.output.split("\n").map((l) => l.trim()).filter(Boolean));
    out.push("");
  }
  return out.join("\n").trim() + "\n";
}

function srtTime(ms: number): string {
  const t = Math.max(0, Math.round(ms));
  const pad = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${pad(Math.floor(t / 3_600_000))}:${pad(Math.floor(t / 60_000) % 60)}:${pad(Math.floor(t / 1000) % 60)},${pad(t % 1000, 3)}`;
}

/**
 * SubRip subtitles timed from the first caption: the translation, with the original under it.
 * Captions without times (none in practice) are left out.
 */
export function toSrt(captions: Caption[]): string {
  const timed = captions.filter((c) => !c.divider && c.start !== undefined && lines(c.output).length);
  if (!timed.length) return "";
  const base = timed[0].start!;
  return timed.map((c, i) => {
    const start = c.start! - base;
    const end = Math.max(c.end ?? c.start!, c.start! + 1000) - base;
    const text = [...lines(c.output), c.input.trim()].filter(Boolean).join("\n");
    return `${i + 1}\n${srtTime(start)} --> ${srtTime(end)}\n${text}\n`;
  }).join("\n");
}
