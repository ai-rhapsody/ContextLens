import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type UpdateState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "current" }
  | { phase: "available"; version: string; notes: string | null }
  | { phase: "downloading"; version: string; received: number; total: number | null }
  | { phase: "ready"; version: string }
  | { phase: "error"; message: string };

export const currentVersion = () => getVersion().catch(() => "dev");

let pending: Update | null = null;

export async function checkForUpdate(): Promise<UpdateState> {
  try {
    pending = await check();
    if (!pending) return { phase: "current" };
    return { phase: "available", version: pending.version, notes: pending.body ?? null };
  } catch (e) {
    return { phase: "error", message: String(e) };
  }
}

/** Downloads, verifies the signature and installs; reports progress through `onState`. */
export async function installUpdate(onState: (s: UpdateState) => void): Promise<void> {
  if (!pending) return;
  const version = pending.version;
  let received = 0;
  let total: number | null = null;
  try {
    await pending.downloadAndInstall((e) => {
      if (e.event === "Started") total = e.data.contentLength ?? null;
      if (e.event === "Progress") received += e.data.chunkLength;
      onState({ phase: "downloading", version, received, total });
    });
    onState({ phase: "ready", version });
  } catch (e) {
    onState({ phase: "error", message: String(e) });
  }
}

export const restartApp = () => relaunch();
