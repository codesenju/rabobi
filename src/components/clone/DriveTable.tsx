"use client";
// Selectable drive list — the source/destination drive selection TreeViews.

import { motion } from "motion/react";
import { HardDrive, Usb } from "lucide-react";
import { cn } from "@/lib/utils";
import { SPRING_LAYOUT } from "@/lib/ease";
import type { Drive } from "@/lib/clone/types";
import { formatBytes } from "@/lib/clone/format";

export function DriveTable({
  drives,
  selectedKey,
  onSelect,
  disabledKey,
  disabledReason,
}: {
  drives: Drive[];
  selectedKey: string | null;
  onSelect: (key: string) => void;
  disabledKey?: string | null;
  disabledReason?: string;
}) {
  return (
    <div className="flex flex-col gap-2" role="listbox" aria-label="Drives">
      {drives.map((drive) => {
        const selected = drive.key === selectedKey;
        const disabled = disabledKey === drive.key;
        return (
          <button
            key={drive.key}
            type="button"
            role="option"
            aria-selected={selected}
            disabled={disabled}
            onClick={() => onSelect(drive.key)}
            className={cn(
              "relative overflow-hidden rounded-xl border p-4 text-left transition-colors",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              selected ? "border-primary" : "border-border hover:border-muted-foreground",
              disabled && "cursor-not-allowed opacity-50",
            )}
          >
            {selected ? (
              <motion.span
                layoutId="drive-selected"
                transition={SPRING_LAYOUT}
                className="absolute inset-0 -z-0 bg-muted"
              />
            ) : null}
            <div className="relative z-10 flex items-start gap-3">
              <span className="mt-0.5 text-muted-foreground">
                {drive.removable ? <Usb className="h-5 w-5" /> : <HardDrive className="h-5 w-5" />}
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline justify-between gap-2">
                  <span className="truncate font-medium text-foreground">
                    {drive.key.replace(/^\/dev\//, "")}
                    {drive.model ? ` — ${drive.model}` : ""}
                  </span>
                  <span className="shrink-0 text-sm tabular-nums text-muted-foreground">
                    {formatBytes(drive.sizeBytes)}
                  </span>
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {drive.partitionTable
                    ? `${drive.partitionTable.toUpperCase()} · ${drive.partitions.length} partition${
                        drive.partitions.length === 1 ? "" : "s"
                      }`
                    : "No partition table"}
                </div>
                {disabled && disabledReason ? (
                  <div className="mt-1 text-xs text-destructive">{disabledReason}</div>
                ) : null}
                {drive.partitions.length > 0 ? (
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    {drive.partitions.map((p) => (
                      <span
                        key={p.key}
                        className="rounded-md bg-background/60 px-2 py-0.5 text-[11px] text-muted-foreground ring-1 ring-border"
                      >
                        {p.key.replace(/^\/dev\//, "")} · {p.filesystem ?? "raw"} ·{" "}
                        {formatBytes(p.sizeBytes)}
                      </span>
                    ))}
                  </div>
                ) : null}
              </div>
            </div>
          </button>
        );
      })}
    </div>
  );
}
