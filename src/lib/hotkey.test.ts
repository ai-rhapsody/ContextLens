import { describe, expect, it } from "vitest";
import { eventToAccelerator, prettyAccelerator } from "./hotkey";

const ev = (code: string, m: Partial<Record<"ctrlKey" | "metaKey" | "altKey" | "shiftKey", boolean>> = {}) => ({
  code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...m,
});

describe("eventToAccelerator", () => {
  it("maps the primary modifier to CommandOrControl on each OS", () => {
    expect(eventToAccelerator(ev("KeyT", { ctrlKey: true, shiftKey: true }), false)).toBe("CommandOrControl+Shift+T");
    expect(eventToAccelerator(ev("KeyT", { metaKey: true, shiftKey: true }), true)).toBe("CommandOrControl+Shift+T");
  });
  it("keeps Control distinct from Command on macOS", () => {
    expect(eventToAccelerator(ev("KeyT", { ctrlKey: true }), true)).toBe("Control+T");
  });
  it("refuses bare letters but allows bare F-keys", () => {
    expect(eventToAccelerator(ev("KeyT"), false)).toBeNull();
    expect(eventToAccelerator(ev("F8"), false)).toBe("F8");
  });
  it("ignores keys that are only modifiers", () => {
    expect(eventToAccelerator(ev("ShiftLeft", { shiftKey: true }), false)).toBeNull();
  });
  it("handles digits and space", () => {
    expect(eventToAccelerator(ev("Digit1", { altKey: true }), false)).toBe("Alt+1");
    expect(eventToAccelerator(ev("Space", { ctrlKey: true }), false)).toBe("CommandOrControl+Space");
  });
});

describe("prettyAccelerator", () => {
  it("renders per platform", () => {
    expect(prettyAccelerator("CommandOrControl+Shift+T", false)).toBe("Ctrl + Shift + T");
    expect(prettyAccelerator("CommandOrControl+Shift+T", true)).toBe("⌘ ⇧ T");
  });
});
