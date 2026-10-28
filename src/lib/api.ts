import { Channel, invoke } from "@tauri-apps/api/core";

export type Provider = "gemini" | "openai" | "anthropic";
export type Source = "selection" | "screen" | "clipboard_image" | "live";
export type Task = "translate" | "explain" | "search";

export const TASKS: { id: Task; label: string; hint: string }[] = [
  { id: "translate", label: "翻譯", hint: "辨識文字並翻譯" },
  { id: "explain", label: "解釋", hint: "說明內容：圖表、示意圖、迷因、程式碼、題目" },
  { id: "search", label: "搜尋", hint: "辨識這是什麼並上網查資料" },
];
export type LiveSource = "mic" | "system" | "screen";

export interface Hotkey {
  id: string;
  accelerator: string;
  source: Source;
  task: Task;
  tutor: boolean;
  /** Live hotkeys: what to start with; empty means what was used last. */
  live_sources?: LiveSource[];
}

export interface Settings {
  target_language: string;
  provider: Provider;
  models: Record<Provider, string> & { gemini_live: string; openai_realtime: string; gemini_translate: string;
    gemini_explain: string; openai_explain: string; anthropic_explain: string };
  live_idle_minutes: number;
  live_provider: Provider | null;
  hotkeys: Hotkey[];
  save_history: boolean;
  popup_size: [number, number] | null;
  popup_pinned: boolean;
  popup_pos: [number, number] | null;
  close_to_tray: boolean;
  minimize_to_tray: boolean;
  start_in_tray: boolean;
  audio_only_models: string[];
  live_sources: LiveSource[];
  own_language_to_english: boolean;
  welcome_seen: boolean;
  live_hotkey_added: boolean;
  own_language_upgraded: boolean;
}

export interface Media {
  mime: string;
  data_b64: string;
  name?: string | null;
}

export interface Capture {
  text: string;
  media: Media | null;
  task: Task;
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
  sources?: [string, string][];
  /** Follow-up questions and answers, in order. */
  thread?: [string, string][];
  task?: Task | null;
}

export interface WinRect { x: number; y: number; w: number; h: number; title: string }
export interface SnipView {
  preview: string; width: number; height: number; windows: WinRect[];
  purpose: "translate" | "live_target";
}
export interface Rect { x: number; y: number; w: number; h: number }

export interface Caps { image: boolean; audio: boolean; video: boolean; live: boolean }
export interface ProviderInfo {
  provider: Provider; model: string; caps: Caps; has_key: boolean;
  /** Can search the web (Search task). */
  web: boolean;
  /** The real-time model, for providers that have a real-time API. */
  live_model: string | null;
}
export const providerInfo = () => invoke<ProviderInfo[]>("provider_info");

export type StreamEvent =
  | { kind: "delta"; text: string }
  | { kind: "source"; title: string; url: string }
  | { kind: "conversation"; id: number }
  | { kind: "question"; text: string }
  | { kind: "offline"; reason: string }
  | { kind: "done" }
  | { kind: "error"; message: string };

export const PROVIDERS: { id: Provider; name: string; vendor: string; media: string }[] = [
  { id: "gemini", name: "Gemini", vendor: "Google", media: "文字、圖片、音訊、影片" },
  { id: "openai", name: "ChatGPT", vendor: "OpenAI", media: "文字、圖片" },
  { id: "anthropic", name: "Claude", vendor: "Anthropic", media: "文字、圖片" },
];

export interface PlatformInfo { os: string; wayland: boolean; exe: string }
export const platformInfo = () => invoke<PlatformInfo>("platform_info");
export const autostartGet = () => invoke<boolean>("autostart_get");
export const autostartSet = (enabled: boolean) => invoke<void>("autostart_set", { enabled });
/** The model a task uses: Explain and Search may use a stronger one (empty field: the main model). */
export function modelForTask(s: Settings, p: Provider, task: Task): string {
  const explain = s.models[`${p}_explain` as const]?.trim();
  return task !== "translate" && explain ? explain : s.models[p];
}

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const setApiKey = (provider: Provider, key: string) =>
  invoke<void>("set_api_key", { args: { provider }, key });
export const deleteApiKey = (provider: Provider) => invoke<void>("delete_api_key", { args: { provider } });
export const testLive = (provider: Provider) => invoke<string>("test_live", { args: { provider } });
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
  task: Task,
  tutor: boolean,
  onEvent: (e: StreamEvent) => void,
  provider: Provider | null = null,
) {
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  return invoke<void>("run_translation", { input, task, tutor, provider, onEvent: channel });
}

export const dataUrl = (m: Media) => `data:${m.mime};base64,${m.data_b64}`;

export interface LiveOption { provider: Provider; audio: boolean; screen: boolean }

export interface LiveInfo {
  running: boolean;
  sources: LiveSource[];
  tutor: boolean;
  /** The running session's provider and model. */
  label: string | null;
  /** Chosen in the caption bar; null means automatic. */
  chosen: Provider | null;
  /** Providers with a key, and what they can do in Live. */
  options: LiveOption[];
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
export const liveSetProvider = (provider: Provider | null) => invoke<void>("live_set_provider", { provider });

/** Opens a web search for `query` in the default browser. */
export async function searchWeb(query: string) {
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(`https://www.google.com/search?q=${encodeURIComponent(query)}`);
}

export async function openLink(url: string) {
  if (!/^https?:\/\//.test(url)) return;
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

/** One capture and every follow-up about it. `id` is also its history id when history is on. */
export interface Conversation {
  id: number;
  task: Task;
  tutor: boolean;
  text: string;
  media: Media | null;
  label: string;
  provider: Provider;
  model: string;
  answer: string;
  sources: [string, string][];
  thread: [string, string][];
  media_lost: boolean;
}

/** What a selected phrase in an answer can be asked about. */
export type PhraseAction = "explain" | "examples" | "rephrase";

export const convoGet = (id: number) => invoke<Conversation | null>("convo_get", { id });
export function followUp(
  id: number,
  ask: { question: string } | { action: PhraseAction; phrase: string },
  onEvent: (e: StreamEvent) => void,
) {
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  const args = "question" in ask
    ? { question: ask.question, action: null, phrase: null }
    : { question: null, action: ask.action, phrase: ask.phrase };
  return invoke<void>("follow_up", { id, ...args, onEvent: channel });
}
export const openChat = (id: number) => invoke<void>("open_chat", { id });
export const chatPending = () => invoke<number | null>("chat_pending");
export const liveFollowUp = (input: string, output: string) => invoke<void>("live_follow_up", { input, output });
export const liveStar = (input: string, output: string) => invoke<number>("live_star", { input, output });
