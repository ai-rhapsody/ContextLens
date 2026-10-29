import en from "../locales/en.json";
import es from "../locales/es.json";
import ja from "../locales/ja.json";
import ko from "../locales/ko.json";

/**
 * Interface languages. Keys are the Traditional Chinese text itself, so Chinese needs no
 * dictionary and a missing translation falls back to Chinese. The same files serve the Rust
 * side (`contextlens_core::i18n`). A key can start with a context, `heading::翻譯`, when the
 * same Chinese needs different words elsewhere (the 翻譯 tab is "Translate", the heading
 * "Translation"); Chinese shows the text after `::`.
 */
export type Locale = "zh-TW" | "en" | "ja" | "ko" | "es";

export const LOCALES: { id: Locale; name: string }[] = [
  { id: "zh-TW", name: "繁體中文" },
  { id: "en", name: "English" },
  { id: "ja", name: "日本語" },
  { id: "ko", name: "한국어" },
  { id: "es", name: "Español" },
];

const DICTS: Record<Exclude<Locale, "zh-TW">, Record<string, string>> = { en, ja, ko, es };

let current: Locale = "zh-TW";

export function setLocale(l: Locale) {
  current = l;
  document.documentElement.lang = l;
}

export const locale = () => current;

/** Maps a BCP 47 tag (`en-US`, `zh-Hant-TW`) to a supported language: any Chinese reads
 * Traditional, anything unsupported English. */
export function pickLocale(tag: string | undefined | null): Locale {
  const t = (tag ?? "").toLowerCase();
  if (t.startsWith("zh")) return "zh-TW";
  for (const l of ["ja", "ko", "es", "en"] as const) if (t.startsWith(l)) return l;
  return "en";
}

/** The Chinese text of a key, without its context. */
const plain = (key: string) => key.replace(/^[a-z]+::/, "");

/** `key` (the Chinese text) in the interface language, with `{name}` filled from `vars`. */
export function t(key: string, vars?: Record<string, string | number>): string {
  const s = (current === "zh-TW" ? null : DICTS[current][key]) || plain(key);
  return vars ? s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : s;
}

/** Marks text for translation where it is defined (a constant); `t()` it where it is shown. */
export const N_ = (key: string) => key;

/** Every language's version of `key`, to recognise text written under another language
 * (answer headings saved in history). */
export function variants(key: string): string[] {
  return [plain(key), ...Object.values(DICTS).map((d) => d[key]).filter(Boolean)];
}

/** Whether `text` is `key` in any language, e.g. an answer heading. */
export const isKey = (text: string | null | undefined, key: string) => !!text && variants(key).includes(text.trim());
