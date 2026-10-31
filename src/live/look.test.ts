import { describe, expect, it } from "vitest";
import { background, DEFAULT_LOOK, floating, nextScale, parseLook, textColor, withBackground } from "./look";

describe("caption look", () => {
  it("starts from the defaults and keeps an earlier caption size", () => {
    expect(parseLook(null)).toEqual(DEFAULT_LOOK);
    expect(parseLook(null, "1.75").scale).toBe(1.75);
    expect(parseLook("not json")).toEqual(DEFAULT_LOOK);
  });

  it("puts values out of range back to their defaults", () => {
    const look = parseLook(JSON.stringify({ scale: 9, bg: 140, color: "red", subtitle: "yes", passthrough: 1, original: false }));
    expect(look).toEqual({ ...DEFAULT_LOOK, bg: 100, original: false });
    expect(parseLook(JSON.stringify({ subtitle: true, passthrough: true }))).toMatchObject({ subtitle: true, passthrough: true });
    expect(parseLook(JSON.stringify({ bg: -5, color: "#FFD400" }))).toMatchObject({ bg: 0, color: "#FFD400" });
    expect(parseLook(JSON.stringify({ bg: null })).bg).toBe(100);
  });

  it("keeps a background for each mode", () => {
    const normal = withBackground(DEFAULT_LOOK, 80);
    expect(normal).toMatchObject({ bg: 80, subBg: 0 });
    const sub = withBackground({ ...normal, subtitle: true }, 20);
    expect(sub).toMatchObject({ bg: 80, subBg: 20 });
    expect(background(sub)).toBe(20);
    expect(background({ ...sub, subtitle: false })).toBe(80);
  });

  it("floats the controls in subtitle mode or over a see-through background", () => {
    expect(floating(DEFAULT_LOOK)).toBe(false);
    expect(floating({ ...DEFAULT_LOOK, bg: 40 })).toBe(true);
    expect(floating({ ...DEFAULT_LOOK, subtitle: true, subBg: 100 })).toBe(true);
  });

  it("turns white over a see-through background unless a colour is picked", () => {
    expect(textColor(DEFAULT_LOOK)).toBeNull();
    expect(textColor({ ...DEFAULT_LOOK, bg: 30 })).toBe("#ffffff");
    expect(textColor({ ...DEFAULT_LOOK, bg: 30, color: "#ffd400" })).toBe("#ffd400");
    expect(textColor({ ...DEFAULT_LOOK, subtitle: true })).toBe("#ffffff"); // subtitle mode starts see-through
  });

  it("steps through the sizes and stops at the ends", () => {
    expect(nextScale(1, 1)).toBe(1.15);
    expect(nextScale(0.8, -1)).toBe(0.8);
    expect(nextScale(3.2, 1)).toBe(3.2);
    expect(nextScale(1.2, 1)).toBe(1.15); // a size no longer offered starts from normal
  });
});
