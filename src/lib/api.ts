import { Channel, invoke } from "@tauri-apps/api/core";

export type Provider = "gemini" | "openai" | "anthropic";
export type Source = "selection" | "screen" | "clipboard_image" | "live";
export type LiveSource = "mic" | "system" | "screen";

export interface Hotkey {
  id: string;
  accelerator: string;
  source: Source;
  tutor: boolean;
}

export interface Settings {
  target_language: string;
  provider: Provider;
  models: Record<Provider, string> & { gemini_live: string };
  live_idle_minutes: number;
  hotkeys: Hotkey[];
  save_history: boolean;
  popup_size: [number, number] | null;
}

export interface Media {
  mime: string;
  data_b64: string;
  name?: string | null;
}

export interface Capture {
  text: string;
  media: Media | null;
  tutor: boolean;
  error: string | null;
}

export interface HistoryEntry {
  at_ms: number;
  kind: "text" | "image" | "audio" | "video" | "live";
  source: string;
  thumb: string | null;
  result: string;
  tutor: boolean;
  provider: Provider;
  model: string;
}

export interface WinRect { x: number; y: number; w: number; h: number; title: string }
export interface SnipView {
  preview: string; width: number; height: number; windows: WinRect[];
  purpose: "translate" | "live_target";
}
export interface Rect { x: number; y: number; w: number; h: number }

export interface Caps { image: boolean; audio: boolean; video: boolean; live: boolean }
export interface ProviderInfo { provider: Provider; model: string; caps: Caps; has_key: boolean }
export const providerInfo = () => invoke<ProviderInfo[]>("provider_info");

export type StreamEvent =
  | { kind: "delta"; text: string }
  | { kind: "done" }
  | { kind: "error"; message: string };

export const PROVIDERS: { id: Provider; name: string; vendor: string; media: string }[] = [
  { id: "gemini", name: "Gemini", vendor: "Google", media: "文字、圖片、音訊、影片" },
  { id: "openai", name: "ChatGPT", vendor: "OpenAI", media: "文字、圖片" },
  { id: "anthropic", name: "Claude", vendor: "Anthropic", media: "文字、圖片" },
];

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const setApiKey = (provider: Provider, key: string) =>
  invoke<void>("set_api_key", { args: { provider }, key });
export const deleteApiKey = (provider: Provider) => invoke<void>("delete_api_key", { args: { provider } });
export const testProvider = (provider: Provider) => invoke<string>("test_provider", { args: { provider } });
export const takePendingCapture = () => invoke<Capture | null>("take_pending_capture");
export const hidePopup = () => invoke<void>("hide_popup");
export const setPopupPinned = (pinned: boolean) => invoke<void>("set_popup_pinned", { pinned });
export const showSettings = () => invoke<void>("show_settings");
export const snipView = () => invoke<SnipView>("snip_view");
export const finishSnip = (rect: Rect, label: string | null = null) => invoke<void>("finish_snip", { rect, label });
export const cancelSnip = () => invoke<void>("cancel_snip");
export const historyList = (query: string) => invoke<HistoryEntry[]>("history_list", { query });
export const historyDelete = (atMs: number) => invoke<void>("history_delete", { atMs });
export const historyClear = () => invoke<void>("history_clear");

export async function keyStatus(): Promise<Record<Provider, boolean>> {
  const rows = await invoke<[Provider, boolean][]>("key_status");
  return Object.fromEntries(rows) as Record<Provider, boolean>;
}

/** Starts a streaming translation. Events from a superseded run must be ignored by the caller. */
export function runTranslation(
  input: { text: string; media: Media | null },
  tutor: boolean,
  onEvent: (e: StreamEvent) => void,
  provider: Provider | null = null,
) {
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  return invoke<void>("run_translation", { input, tutor, provider, onEvent: channel });
}

export const dataUrl = (m: Media) => `data:${m.mime};base64,${m.data_b64}`;

export interface LiveInfo {
  running: boolean;
  sources: LiveSource[];
  tutor: boolean;
  model: string;
  model_ok: boolean;
  system_audio: boolean;
  /** What the screen source watches, if one was picked. */
  screen: string | null;
}

export type LiveEvent =
  | { kind: "status"; state: "connecting" | "listening" | "reconnecting" | "paused" | "stopped" | "error"; message: string | null }
  | { kind: "input"; text: string }
  | { kind: "output"; text: string }
  | { kind: "turn_complete" }
  | { kind: "level"; value: number };

export const liveInfo = () => invoke<LiveInfo>("live_info");
export const liveStart = (sources: LiveSource[], tutor: boolean) => invoke<void>("live_start", { sources, tutor });
export const liveStop = () => invoke<void>("live_stop");
export const openLive = () => invoke<void>("open_live");
export const closeLive = () => invoke<void>("close_live");
export const livePickScreen = () => invoke<void>("live_pick_screen");
