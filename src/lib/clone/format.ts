import type { Drive, Partition } from "./types";

/** Human-readable byte size, e.g. "8.00 GB". Mirrors Rescuezilla's size text. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exp;
  const digits = exp === 0 ? 0 : 2;
  return `${value.toFixed(digits)} ${units[exp]}`;
}

/** "sdc: 8.00 GB (SanDisk Ultra)" — the clone destination description format. */
export function describeDrive(drive: Drive): string {
  const shortNode = drive.key.replace(/^\/dev\//, "");
  const model = drive.model ? ` (${drive.model})` : "";
  return `${shortNode}: ${formatBytes(drive.sizeBytes)}${model}`;
}

/** "/dev/sda1: 50.00 GB ntfs (Windows)" — the partition description format. */
export function describePartition(part: Partition): string {
  const fs = part.filesystem ? ` ${part.filesystem}` : "";
  const label = part.label ? ` (${part.label})` : "";
  const used =
    part.usedBytes != null ? `, ${formatBytes(part.usedBytes)} used` : "";
  return `${part.key}: ${formatBytes(part.sizeBytes)}${fs}${used}${label}`;
}

export function formatPercent(fraction: number): string {
  return `${Math.round(Math.max(0, Math.min(1, fraction)) * 100)}%`;
}
