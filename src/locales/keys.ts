/**
 * Finds the texts to translate: `t("…")` and `N_("…")` in the frontend, `t!("…")` and
 * `n_!("…")` in Rust. Used by the locale tests and to list keys for translators.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(__dirname, "..", "..");
const CJK = /[\u3000-\u9fff\uff00-\uffef]/;

function files(dir: string, ext: RegExp): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return name === "node_modules" || name === "locales" ? [] : files(p, ext);
    return ext.test(name) && !/\.test\.tsx?$/.test(name) ? [p] : [];
  });
}

/** The code of a file without its tests (Rust test modules). */
function source(path: string): string {
  const text = readFileSync(path, "utf8");
  const cut = path.endsWith(".rs") ? text.indexOf("#[cfg(test)]") : -1;
  return cut >= 0 ? text.slice(0, cut) : text;
}

function unescapeTs(s: string): string {
  return s.replace(/\\(.)/g, (_, c: string) => ({ n: "\n", t: "\t" } as Record<string, string>)[c] ?? c);
}

/** Rust string literal rules: `\` at a line end drops the newline and the next line's indent. */
function unescapeRust(s: string): string {
  return s.replace(/\\\n\s*/g, "").replace(/\\(.)/g, (_, c: string) => ({ n: "\n", t: "\t" } as Record<string, string>)[c] ?? c);
}

const TS_CALL = /\b(?:t\(|N_\(|variants\(|isKey\([^,()]*,)\s*"((?:[^"\\]|\\.)*)"/g;
const RS_CALL = /\b(?:t|n_)!\(\s*"((?:[^"\\]|\\.)*)"/g;

export const tsFiles = () => files(join(ROOT, "src"), /\.tsx?$/);
export const rsFiles = () => [...files(join(ROOT, "crates", "core", "src"), /\.rs$/), ...files(join(ROOT, "src-tauri", "src"), /\.rs$/)];

// Comments go first; string contents are kept, so a `//` inside a string survives as `"…//…"`.
const noTsComments = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, "")).replace(/(^|[^:"'\\])\/\/.*$/gm, "$1");
const noRsComments = (s: string) => s.replace(/(^|[^:"\\])\/\/.*$/gm, "$1");

/** Every key, sorted. Examples in comments are not keys. */
export function collectKeys(): string[] {
  const keys = new Set<string>();
  for (const f of tsFiles()) for (const m of noTsComments(source(f)).matchAll(TS_CALL)) keys.add(unescapeTs(m[1]));
  for (const f of rsFiles()) for (const m of noRsComments(source(f)).matchAll(RS_CALL)) keys.add(unescapeRust(m[1]));
  return [...keys].sort();
}

/** `{name}` placeholders in a text, sorted. */
export const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

/**
 * Chinese left outside `t`/`N_`/`t!`/`n_!` in code (not comments): text a user would see
 * untranslated. `allowed` lines are data, not interface text.
 */
export function untranslated(allowed: RegExp[]): string[] {
  const found: string[] = [];
  const check = (f: string, calls: RegExp, strip: (s: string) => string) => {
    const code = strip(source(f)).replace(calls, '""');
    code.split("\n").forEach((line, i) => {
      if (CJK.test(line) && !allowed.some((a) => a.test(line))) found.push(`${f.slice(ROOT.length + 1)}:${i + 1}: ${line.trim()}`);
    });
  };
  // Multi-line calls: remove the whole literal, continuation lines included.
  for (const f of tsFiles()) check(f, /\b(?:t\(|N_\(|variants\(|isKey\([^,()]*,)\s*"(?:[^"\\]|\\.)*"/g, noTsComments);
  for (const f of rsFiles()) check(f, /\b(?:t|n_)!\(\s*"(?:[^"\\]|\\[\s\S])*"/g, noRsComments);
  return found;
}
