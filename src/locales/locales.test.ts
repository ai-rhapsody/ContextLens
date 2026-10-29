import { describe, expect, it } from "vitest";
import en from "./en.json";
import es from "./es.json";
import ja from "./ja.json";
import ko from "./ko.json";
import { collectKeys, placeholders, untranslated } from "./keys";

const LOCALES: Record<string, Record<string, string>> = { en, ja, ko, es };
const keys = collectKeys();

/** Chinese in code that is data, not interface text. */
const DATA = [
  /"(繁體中文|简体中文|簡體中文|正體中文|中文|中文（繁體）|中文（简体）|日本語|日文|한국어|韓文|英文|英語)"/, // language names: setting values
  /'[^']'(, '[^']')?\)?,? ?/, // single characters: punctuation sets, Chinese script pairs
  /JAPANESE_ONLY: &str/,
  /^\s*[^\s:"]+: ""\)?,?$/, // lookup keys of an N_() table, e.g. CHIP_QUESTION
  /\/[^/\n]*[「」\u3040-\u30ff\u4e00-\u9fff][^/\n]*\/(g|\.test)/, // regular expressions that detect scripts
  /"　"|`　/, // a full-width space as a separator
];

describe("locales", () => {
  it("finds the keys", () => {
    expect(keys.length).toBeGreaterThan(300);
    expect(keys).toContain("翻譯");
  });

  for (const [name, dict] of Object.entries(LOCALES)) {
    it(`${name} translates every key, keeping its placeholders`, () => {
      const missing = keys.filter((k) => !dict[k]?.trim());
      expect(missing).toEqual([]);
      const wrong = keys.filter((k) => dict[k] && placeholders(dict[k]).join() !== placeholders(k).join());
      expect(wrong).toEqual([]);
    });

    it(`${name} has no stale keys`, () => {
      expect(Object.keys(dict).filter((k) => !keys.includes(k))).toEqual([]);
    });
  }

  it("leaves no Chinese outside t() in code", () => {
    expect(untranslated(DATA)).toEqual([]);
  });
});
