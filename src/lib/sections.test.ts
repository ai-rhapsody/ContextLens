import { describe, expect, it } from "vitest";
import { parseSections } from "./sections";

describe("parseSections", () => {
  it("splits on headings in order", () => {
    const s = parseSections("## 翻譯\n你好\n## 結構\n- 主詞\n- 動詞\n");
    expect(s.map((x) => x.title)).toEqual(["翻譯", "結構"]);
    expect(s[1].body).toBe("- 主詞\n- 動詞");
  });

  it("holds back a heading that is still arriving", () => {
    expect(parseSections("## 翻譯\n你好\n## 結")).toEqual([{ title: "翻譯", body: "你好" }]);
    expect(parseSections("## 翻譯\n你")).toEqual([{ title: "翻譯", body: "你" }]);
  });

  it("keeps plain text as one untitled section", () => {
    expect(parseSections("你好")).toEqual([{ title: null, body: "你好" }]);
  });

  it("returns nothing for empty input", () => {
    expect(parseSections("")).toEqual([]);
  });
});
