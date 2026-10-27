import { describe, expect, it } from "vitest";
import { kindOf, mimeFor } from "./files";

describe("mimeFor", () => {
  it("trusts a specific browser type", () => {
    expect(mimeFor("a.bin", "image/png")).toBe("image/png");
  });
  it("falls back to the extension when the type is empty or generic", () => {
    expect(mimeFor("talk.M4A", "")).toBe("audio/mp4");
    expect(mimeFor("clip.mov", "application/octet-stream")).toBe("video/quicktime");
  });
});

describe("kindOf", () => {
  it("classifies by the top-level type", () => {
    expect(kindOf("video/mp4")).toBe("video");
    expect(kindOf("application/pdf")).toBeNull();
  });
});
