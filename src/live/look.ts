import { N_ } from "../lib/i18n";

/** How the caption bar looks; kept on this computer for the Live window. */
export type Look = {
  /** Caption size, as a multiple of the normal size. */
  scale: number;
  /** Background opacity in percent: 100 is the normal bar, 0 leaves only the text over the screen. */
  bg: number;
  /** The translation's colour: "auto", or a `#rrggbb` colour. */
  color: string;
  /** Only the latest captions, centred at the bottom; the controls show while the mouse is over the bar. */
  subtitle: boolean;
  /** The original text above each translation. */
  original: boolean;
};

export const SCALES = [0.8, 0.9, 1, 1.15, 1.3, 1.5, 1.75, 2, 2.4, 2.8, 3.2];

/** Colours that read well as subtitles, besides any the user picks. */
export const COLORS: { id: string; label: string }[] = [
  { id: "auto", label: N_("自動") },
  { id: "#ffffff", label: N_("白") },
  { id: "#ffd400", label: N_("黃") },
  { id: "#5ee7ff", label: N_("青") },
  { id: "#7dff8a", label: N_("綠") },
];

export const DEFAULT_LOOK: Look = { scale: 1, bg: 100, color: "auto", subtitle: false, original: true };

const KEY = "lv-look";
/** Where earlier versions kept only the caption size. */
const OLD_SCALE_KEY = "lv-scale";

/** A saved look, with anything missing or out of range back at its default. */
export function parseLook(saved: string | null, oldScale: string | null = null): Look {
  let raw: Partial<Look> = {};
  try {
    raw = saved ? JSON.parse(saved) : {};
  } catch {
    // A damaged value: start over.
  }
  const scale = Number(raw.scale ?? oldScale);
  const bg = Number(raw.bg);
  return {
    scale: SCALES.includes(scale) ? scale : DEFAULT_LOOK.scale,
    bg: Number.isFinite(bg) ? Math.min(100, Math.max(0, Math.round(bg))) : DEFAULT_LOOK.bg,
    color: typeof raw.color === "string" && /^(auto|#[0-9a-f]{6})$/i.test(raw.color) ? raw.color : DEFAULT_LOOK.color,
    subtitle: typeof raw.subtitle === "boolean" ? raw.subtitle : DEFAULT_LOOK.subtitle,
    original: typeof raw.original === "boolean" ? raw.original : DEFAULT_LOOK.original,
  };
}

export function loadLook(): Look {
  try {
    return parseLook(localStorage.getItem(KEY), localStorage.getItem(OLD_SCALE_KEY));
  } catch {
    return DEFAULT_LOOK;
  }
}

export function saveLook(look: Look) {
  try {
    localStorage.setItem(KEY, JSON.stringify(look));
  } catch {
    // The look still applies until the window closes.
  }
}

/** Over a mostly see-through background the text gets a dark outline, like subtitles on a video. */
export const outlined = (look: Look) => look.bg < 60;

/** The translation's colour: the one picked, white over a see-through background, else the theme's. */
export function textColor(look: Look): string | null {
  if (look.color !== "auto") return look.color;
  return outlined(look) ? "#ffffff" : null;
}

export function nextScale(scale: number, step: number): number {
  const i = SCALES.indexOf(scale);
  return SCALES[Math.min(SCALES.length - 1, Math.max(0, (i < 0 ? SCALES.indexOf(1) : i) + step))];
}
