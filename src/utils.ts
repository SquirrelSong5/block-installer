import type { VersionEntry } from "./api";

/** 按版本号估计需要的 Java（实际以版本文件为准，安装时会重新判断）。 */
export function javaFor(v: VersionEntry | { id: string; releaseTime?: string; category?: string }): number {
  const id = v.id;
  const year = /^(\d{2})\.(\d+)/.exec(id);
  if (year && Number(year[1]) >= 26) return 25;
  const m = /^1\.(\d+)(?:\.(\d+))?$/.exec(id);
  if (m) {
    const a = Number(m[1]);
    const b = Number(m[2] ?? 0);
    if (a >= 21 || (a === 20 && b >= 5)) return 21;
    if (a >= 18) return 17;
    if (a === 17) return 16;
    return 8;
  }
  // 快照等按发布时间估计
  const t = v.releaseTime ?? "";
  if (!t) return 8;
  if (t < "2021-05-12") return 8;
  if (t < "2021-09-15") return 16;
  if (t < "2024-04-01") return 17;
  if (t < "2025-12-01") return 21;
  return 25;
}

/** Java 主版本能运行的 MC 范围说明。 */
export function javaCoverage(major: number): string {
  if (major === 8) return "可运行 1.16.5 及更早的版本";
  if (major < 17) return "可运行 1.17";
  if (major < 21) return "可运行 1.17 – 1.20.4";
  if (major < 25) return "可运行 1.17 – 1.21.x";
  return "可运行 1.17 及以后的所有版本";
}

export function javaFits(major: number, required: number): boolean {
  return required <= 8 ? major === 8 : major >= required;
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}/${p(d.getMonth() + 1)}/${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function formatSpeed(bps: number): string {
  if (!bps || bps < 1) return "0 B/s";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let i = 0;
  let v = bps;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function iconForVersion(v: { id: string; category?: string }): string {
  switch (v.category) {
    case "snapshot":
      return "cmd";
    case "old":
      return "cobble";
    case "aprilFools":
      return "cake";
    default:
      return "grass";
  }
}

export const LANGUAGES: { value: string; label: string }[] = [
  { value: "zh_cn", label: "简体中文" },
  { value: "zh_tw", label: "繁體中文（台灣）" },
  { value: "zh_hk", label: "繁體中文（香港）" },
  { value: "en_us", label: "English" },
  { value: "", label: "不修改" },
];
