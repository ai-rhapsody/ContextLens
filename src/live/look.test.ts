import { describe, expect, it } from "vitest";
import { DEFAULT_LOOK, nextScale, parseLook, textColor } from "./look";

describe("caption look", () => {
  it("starts from the defaults and keeps an earlier caption size", () => {
    expect(parseLook(null)).toEqual(DEFAULT_LOOK);
    expect(parseLook(null, "1.75").scale).toBe(1.75);
    expect(parseLook("not json")).toEqual(DEFAULT_LOOK);
  });

  it("puts values out of range back to their defaults", () => {
    const look = parseLook(JSON.stringify({ scale: 9, bg: 140, color: "red", subtitle: "yes", original: false }));
    expect(look).toEqual({ ...DEFAULT_LOOK, bg: 100, original: false });
    expect(parseLook(JSON.stringify({ bg: -5, color: "#FFD400" }))).toMatchObject({ bg: 0, color: "#FFD400" });
  });

  it("turns white over a see-through background unless a colour is picked", () => {
    expect(textColor(DEFAULT_LOOK)).toBeNull();
    expect(textColor({ ...DEFAULT_LOOK, bg: 30 })).toBe("#ffffff");
    expect(textColor({ ...DEFAULT_LOOK, bg: 30, color: "#ffd400" })).toBe("#ffd400");
  });

  it("steps through the sizes and stops at the ends", () => {
    expect(nextScale(1, 1)).toBe(1.15);
    expect(nextScale(0.8, -1)).toBe(0.8);
    expect(nextScale(3.2, 1)).toBe(3.2);
    expect(nextScale(1.2, 1)).toBe(1.15); // a size no longer offered starts from normal
  });
});
