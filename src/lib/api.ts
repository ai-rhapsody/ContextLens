import { Channel, invoke } from "@tauri-apps/api/core";

export type Provider = "gemini" | "openai" | "anthropic";
export type ActionId = "translate_selection" | "tutor_selection";

export interface Hotkey {
  id: string;
  accelerator: string;
  action: ActionId;
}

export interface Settings {
  target_language: string;
  provider: Provider;
  models: Record<Provider, string>;
  tutor_mode: boolean;
  hotkeys: Hotkey[];
}

export interface Capture {
  text: string;
  tutor: boolean;
  error: string | null;
}

export type StreamEvent =
  | { kind: "delta"; text: string }
  | { kind: "done" }
  | { kind: "error"; message: string };

export const PROVIDERS: { id: Provider; name: string; vendor: string }[] = [
  { id: "gemini", name: "Gemini", vendor: "Google" },
  { id: "openai", name: "ChatGPT", vendor: "OpenAI" },
  { id: "anthropic", name: "Claude", vendor: "Anthropic" },
];

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const setApiKey = (provider: Provider, key: string) =>
  invoke<void>("set_api_key", { args: { provider }, key });
export const deleteApiKey = (provider: Provider) => invoke<void>("delete_api_key", { args: { provider } });
export const testProvider = (provider: Provider) => invoke<string>("test_provider", { args: { provider } });
export const takePendingCapture = () => invoke<Capture | null>("take_pending_capture");
export const hidePopup = () => invoke<void>("hide_popup");
export const showSettings = () => invoke<void>("show_settings");

export async function keyStatus(): Promise<Record<Provider, boolean>> {
  const rows = await invoke<[Provider, boolean][]>("key_status");
  return Object.fromEntries(rows) as Record<Provider, boolean>;
}

/** Starts a streaming translation. Events from a superseded run must be ignored by the caller. */
export function runTranslation(text: string, tutor: boolean, onEvent: (e: StreamEvent) => void) {
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  return invoke<void>("run_translation", { text, tutor, onEvent: channel });
}
