/** Turns a keydown into a Tauri accelerator string, or null if it is not a usable global hotkey. */

type KeyLike = Pick<KeyboardEvent, "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">;

export function isMac(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
}

function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1\d|2[0-4])$/.test(code)) return code;
  if (code === "Space") return "Space";
  return null;
}

export function eventToAccelerator(e: KeyLike, mac = isMac()): string | null {
  const key = keyName(e.code);
  if (!key) return null;
  const mods: string[] = [];
  // The platform's usual shortcut key is stored as CommandOrControl so one binding works everywhere.
  const primary = mac ? e.metaKey : e.ctrlKey;
  if (primary) mods.push("CommandOrControl");
  if (mac && e.ctrlKey) mods.push("Control");
  if (!mac && e.metaKey) mods.push("Super");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  // A bare letter would steal normal typing system-wide; only F-keys may stand alone.
  if (mods.length === 0 && !/^F\d+$/.test(key)) return null;
  return [...mods, key].join("+");
}

export function prettyAccelerator(acc: string, mac = isMac()): string {
  return acc
    .split("+")
    .map((p) => {
      if (p === "CommandOrControl") return mac ? "⌘" : "Ctrl";
      if (p === "Control") return mac ? "⌃" : "Ctrl";
      if (p === "Alt") return mac ? "⌥" : "Alt";
      if (p === "Shift") return mac ? "⇧" : "Shift";
      if (p === "Super") return "Win";
      return p;
    })
    .join(mac ? " " : " + ");
}
