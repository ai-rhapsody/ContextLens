import { describe, expect, it } from "vitest";
import { toSrt, toText, type Caption } from "./captions";

const t0 = Date.UTC(2026, 9, 29, 7, 0, 0);
const caps: Caption[] = [
  { input: "Hello everyone.", output: "大家好。\n註：everyone 指在場所有人", start: t0, end: t0 + 1800 },
  { input: "", output: "", divider: "15:10" },
  { input: "Thank you.", output: "謝謝。", start: t0 + 65_250, end: t0 + 65_400 },
  { input: "…", output: "", start: t0 + 70_000, end: t0 + 71_000 },
];

describe("captions", () => {
  it("writes text with originals, notes and session marks", () => {
    expect(toText(caps)).toBe(
      "Hello everyone.\n大家好。\n註：everyone 指在場所有人\n\n—— 15:10 開始 ——\n\nThank you.\n謝謝。\n\n…\n",
    );
  });

  it("writes SRT timed from the first caption, at least a second each, without notes", () => {
    expect(toSrt(caps)).toBe(
      "1\n00:00:00,000 --> 00:00:01,800\n大家好。\nHello everyone.\n\n" +
      "2\n00:01:05,250 --> 00:01:06,250\n謝謝。\nThank you.\n",
    );
    expect(toSrt([])).toBe("");
  });
});
