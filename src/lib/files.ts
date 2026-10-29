import type { Media } from "./api";
import { N_, t } from "./i18n";

/** Hard cap before anything crosses the IPC bridge; providers have their own, lower limits. */
export const MAX_FILE_BYTES = 20 * 1024 * 1024;

const BY_EXTENSION: Record<string, string> = {
  png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", webp: "image/webp", gif: "image/gif",
  heic: "image/heic", heif: "image/heif",
  mp3: "audio/mpeg", wav: "audio/wav", m4a: "audio/mp4", aac: "audio/aac", ogg: "audio/ogg",
  flac: "audio/flac", aiff: "audio/aiff", opus: "audio/ogg",
  mp4: "video/mp4", mov: "video/quicktime", webm: "video/webm", avi: "video/x-msvideo",
  mkv: "video/x-matroska", wmv: "video/x-ms-wmv", mpeg: "video/mpeg", mpg: "video/mpeg", "3gp": "video/3gpp",
};

/** Browsers leave `type` empty or vague for some files; fall back to the extension. */
export function mimeFor(name: string, type: string): string {
  if (type && type !== "application/octet-stream") return type;
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return BY_EXTENSION[ext] ?? type ?? "";
}

export function kindOf(mime: string): "image" | "audio" | "video" | null {
  const k = mime.split("/")[0];
  return k === "image" || k === "audio" || k === "video" ? k : null;
}

export const KIND_LABEL = { image: N_("圖片"), audio: N_("音訊"), video: N_("影片"), live: "Live" } as const;

export function fileToMedia(file: File): Promise<Media> {
  const mime = mimeFor(file.name, file.type);
  if (!kindOf(mime)) return Promise.reject(new Error(t("不支援這種檔案：{name}", { name: file.name })));
  if (file.size > MAX_FILE_BYTES) {
    return Promise.reject(new Error(t("檔案太大（{size} MB），上限 20 MB。", { size: (file.size / 1048576).toFixed(1) })));
  }
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onerror = () => reject(r.error ?? new Error(t("無法讀取檔案")));
    r.onload = () => {
      const url = String(r.result);
      resolve({ mime, data_b64: url.slice(url.indexOf(",") + 1), name: file.name });
    };
    r.readAsDataURL(file);
  });
}
