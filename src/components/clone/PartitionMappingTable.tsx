"use client";
// The source -> destination partition mapping table (partitions_to_restore.py).

import { ArrowRight } from "lucide-react";
import { Checkbox } from "@/components/motion/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/motion/select";
import {
  type Drive,
  type PartitionMappingRow,
  NOT_RESTORING_PARTITION_KEY,
} from "@/lib/clone/types";
import { describePartition } from "@/lib/clone/format";

export function PartitionMappingTable({
  rows,
  destination,
  locked,
  onToggle,
  onRemap,
}: {
  rows: PartitionMappingRow[];
  destination: Drive;
  /** when true the destination mapping is fixed (overwrite-table mode) */
  locked: boolean;
  onToggle: (sourceKey: string, enabled: boolean) => void;
  onRemap: (sourceKey: string, destKey: string) => void;
}) {
  return (
    <div className="overflow-hidden rounded-xl border border-border">
      <div className="grid grid-cols-[auto_1fr_auto_1fr] items-center gap-3 border-b border-border bg-muted/50 px-4 py-2.5 text-xs font-medium uppercase tracking-wide text-muted-foreground">
        <span>Clone</span>
        <span>Source partition</span>
        <span />
        <span>Destination partition</span>
      </div>
      <div className="divide-y divide-border">
        {rows.map((row) => (
          <div
            key={row.sourceKey}
            className="grid grid-cols-[auto_1fr_auto_1fr] items-center gap-3 px-4 py-3"
          >
            <Checkbox
              checked={row.enabled}
              onCheckedChange={(v) => onToggle(row.sourceKey, v)}
              aria-label={`Clone ${row.sourceKey}`}
            />
            <span className="min-w-0 truncate text-sm text-foreground">
              {row.sourceDescription}
            </span>
            <ArrowRight className="h-4 w-4 shrink-0 text-muted-foreground" />
            {locked ? (
              <span
                className={
                  row.enabled ? "truncate text-sm text-foreground" : "truncate text-sm text-muted-foreground"
                }
              >
                {row.destDescription}
              </span>
            ) : (
              <Select
                value={row.destKey}
                onValueChange={(v) => onRemap(row.sourceKey, v)}
                disabled={!row.enabled}
              >
                <SelectTrigger className="rounded-lg">
                  <SelectValue placeholder="Choose destination" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NOT_RESTORING_PARTITION_KEY}>
                    — do not clone —
                  </SelectItem>
                  {destination.partitions
                    .filter((p) => !p.isExtended)
                    .map((p) => (
                      <SelectItem key={p.key} value={p.key}>
                        {describePartition(p)}
                      </SelectItem>
                    ))}
                </SelectContent>
              </Select>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
