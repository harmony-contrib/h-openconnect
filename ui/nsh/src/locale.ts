import { locale } from "nsh";
import type { LanguagePreference } from "./model";

export function resolveLanguage(
  preference: LanguagePreference | undefined,
  systemLocale?: string,
): "zh" | "en" {
  if (preference === "zh-CN") return "zh";
  if (preference === "en") return "en";
  const tag = (systemLocale || locale() || "zh-CN").toLowerCase();
  return tag.startsWith("zh") ? "zh" : "en";
}

export function sanitizeDisplayText(text: string): string {
  if (!text) return "";
  return text.replace(/vpn/gi, "connection");
}

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let num = value;
  let unit = 0;
  while (num >= 1024 && unit + 1 < units.length) {
    num /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${Math.round(num)} B` : `${num.toFixed(1)} ${units[unit]}`;
}

export function formatRate(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`;
}

export function formatDuration(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(Number.isFinite(totalSeconds) ? totalSeconds : 0));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const secs = safe % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return hours > 0
    ? `${pad(hours)}:${pad(minutes)}:${pad(secs)}`
    : `${pad(minutes)}:${pad(secs)}`;
}

export function formatTimestamp(raw?: string): string {
  if (!raw) return "—";
  const trimmed = raw.trim();
  if (!trimmed) return "—";
  if (/^\d{10,13}$/.test(trimmed)) {
    const ms = trimmed.length === 10 ? Number(trimmed) * 1000 : Number(trimmed);
    const date = new Date(ms);
    if (!Number.isNaN(date.getTime())) {
      const pad = (n: number) => String(n).padStart(2, "0");
      return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
    }
  }
  const parsed = new Date(trimmed);
  if (!Number.isNaN(parsed.getTime())) {
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${parsed.getFullYear()}-${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())} ${pad(parsed.getHours())}:${pad(parsed.getMinutes())}:${pad(parsed.getSeconds())}`;
  }
  return trimmed;
}

export function formatLogTime(raw?: string): string {
  const full = formatTimestamp(raw);
  if (full === "—") return "—";
  const parts = full.split(" ");
  return parts[parts.length - 1] || full;
}

export function fileBasename(path?: string): string {
  if (!path) return "";
  const cleaned = path.replace(/\\/g, "/");
  const idx = cleaned.lastIndexOf("/");
  return idx >= 0 ? cleaned.slice(idx + 1) : cleaned;
}
