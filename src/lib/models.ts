import type { Provider } from "./api";
import { N_ } from "./i18n";

export interface ModelOption {
  id: string;
  label: string;
  /** One line on why you would pick it. */
  note: string;
}

/**
 * Current models per provider (checked October 2026), first entry is the default.
 * The settings page also takes any other model name typed in by hand.
 */
export const MODEL_PRESETS: Record<Provider, ModelOption[]> = {
  gemini: [
    { id: "gemini-3.5-flash-lite", label: "Gemini 3.5 Flash-Lite", note: N_("最快、最便宜") },
    { id: "gemini-3.6-flash", label: "Gemini 3.6 Flash", note: N_("速度與品質平衡") },
    { id: "gemini-3.8-flash", label: "Gemini 3.8 Flash", note: N_("最聰明的 Flash") },
    { id: "gemini-4-argon", label: "Gemini 4 Argon", note: N_("最強，較慢較貴") },
  ],
  openai: [
    { id: "gpt-6-luna", label: "GPT-6 Luna", note: N_("快、低成本") },
    { id: "gpt-6.1-terra", label: "GPT-6.1 Terra", note: N_("平衡，擅長看圖") },
    { id: "gpt-6.1-sol", label: "GPT-6.1 Sol", note: N_("複雜工作") },
    { id: "gpt-6-astra", label: "GPT-6 Astra", note: N_("最強，較慢較貴") },
  ],
  anthropic: [
    { id: "claude-opus-5-5", label: "Claude Opus 5.5", note: N_("品質最好") },
    { id: "claude-sonnet-5-5", label: "Claude Sonnet 5.5", note: N_("快、平衡") },
    { id: "claude-haiku-5-5", label: "Claude Haiku 5.5", note: N_("最快、最便宜") },
    { id: "claude-fable-5-1", label: "Claude Fable 5.1", note: N_("最強，較慢較貴") },
  ],
};

/** Real-time models for Live, for providers that have a real-time API. */
export const LIVE_PRESETS: Partial<Record<Provider, ModelOption[]>> = {
  gemini: [
    { id: "gemini-3.8-live", label: "Gemini 3.8 Live", note: N_("低延遲") },
    { id: "gemini-3.8-live-extended-thinking", label: "Gemini 3.8 Live Extended Thinking", note: N_("推理較深、較慢") },
  ],
  openai: [
    { id: "gpt-realtime-2.1-mini", label: "GPT Realtime 2.1 mini", note: N_("快、便宜") },
    { id: "gpt-realtime-2.1", label: "GPT Realtime 2.1", note: N_("品質較好") },
  ],
};

/** Gemini's interpreter model, used when Live only listens and adds no notes. */
export const TRANSLATE_PRESETS: ModelOption[] = [
  { id: "gemini-3.5-live-translate-preview", label: "Gemini 3.5 Live Translate", note: N_("專門口譯、邊講邊翻") },
  { id: "", label: N_("不使用"), note: N_("一律用即時模型") },
];
