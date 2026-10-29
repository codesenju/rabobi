"use client";
// Destination safety gate (Milestone 5.3). Shows hard blocks (cannot proceed)
// and warnings (proceed only after typing the device name to confirm), and
// reports back whether the operation may start.

import { useEffect, useState } from "react";
import { AlertTriangle, ShieldAlert, ShieldCheck } from "lucide-react";
import { cn } from "@/lib/utils";
import type { DestinationSafety } from "@/lib/clone/ops";
import { checkDestination } from "@/lib/clone/backend";

export function DestinationGuard({
  destination,
  source,
  confirmValue,
  onConfirmChange,
  onReadyChange,
}: {
  /** destination device node, e.g. "/dev/sdb" */
  destination: string;
  /** source device (clone) or null (restore) */
  source: string | null;
  /** current type-to-confirm text */
  confirmValue: string;
  onConfirmChange: (v: string) => void;
  /** reports whether the operation may start, and the device string to send */
  onReadyChange: (ready: boolean, confirmDevice: string | undefined) => void;
}) {
  const [safety, setSafety] = useState<DestinationSafety | null>(null);

  useEffect(() => {
    let active = true;
    checkDestination(destination, source).then((s) => {
      if (active) setSafety(s);
    });
    return () => {
      active = false;
    };
  }, [destination, source]);

  const short = destination.replace(/^\/dev\//, "");
  const confirmed =
    confirmValue.trim() === destination || confirmValue.trim() === short;

  useEffect(() => {
    if (!safety) {
      onReadyChange(false, undefined);
      return;
    }
    if (safety.hardBlocks.length > 0) {
      onReadyChange(false, undefined);
    } else if (safety.needsConfirmation) {
      onReadyChange(confirmed, confirmed ? destination : undefined);
    } else {
      onReadyChange(true, undefined);
    }
  }, [safety, confirmed, destination, onReadyChange]);

  if (!safety) {
    return (
      <div className="rounded-xl border border-border p-4 text-sm text-muted-foreground">
        Checking destination safety…
      </div>
    );
  }

  if (safety.hardBlocks.length > 0) {
    return (
      <div className="rounded-xl border border-destructive/40 bg-destructive/5 p-4">
        <div className="mb-2 flex items-center gap-2 text-destructive">
          <ShieldAlert className="h-4 w-4" />
          <span className="text-sm font-medium">This destination cannot be used</span>
        </div>
        <ul className="flex flex-col gap-1 text-sm text-foreground">
          {safety.hardBlocks.map((b, i) => (
            <li key={i} className="flex items-start gap-2">
              <span className="mt-1.5 h-1 w-1 shrink-0 rounded-full bg-destructive" />
              {b}
            </li>
          ))}
        </ul>
      </div>
    );
  }

  if (safety.safe) {
    return (
      <div className="flex items-center gap-2 rounded-xl border border-emerald-500/30 bg-emerald-500/5 p-4 text-sm">
        <ShieldCheck className="h-4 w-4 text-emerald-500" />
        <span className="text-foreground">Destination looks empty and safe to use.</span>
      </div>
    );
  }

  // needs confirmation
  return (
    <div className="rounded-xl border border-yellow-500/40 bg-yellow-500/5 p-4">
      <div className="mb-2 flex items-center gap-2 text-yellow-600 dark:text-yellow-500">
        <AlertTriangle className="h-4 w-4" />
        <span className="text-sm font-medium">This will erase existing data</span>
      </div>
      <ul className="mb-3 flex flex-col gap-1 text-sm text-foreground">
        {safety.warnings.map((w, i) => (
          <li key={i} className="flex items-start gap-2">
            <span className="mt-1.5 h-1 w-1 shrink-0 rounded-full bg-yellow-500" />
            {w}
          </li>
        ))}
      </ul>
      <label className="flex flex-col gap-1.5">
        <span className="text-sm text-muted-foreground">
          Type <span className="font-mono font-medium text-foreground">{short}</span> to confirm you
          want to overwrite this disk:
        </span>
        <input
          value={confirmValue}
          onChange={(e) => onConfirmChange(e.target.value)}
          placeholder={short}
          spellCheck={false}
          autoComplete="off"
          className={cn(
            "w-48 rounded-lg border bg-background px-3 py-2 font-mono text-sm outline-none transition-colors",
            confirmed
              ? "border-emerald-500/60 text-foreground focus-visible:ring-2 focus-visible:ring-emerald-500/40"
              : "border-border text-foreground focus-visible:ring-2 focus-visible:ring-ring",
          )}
        />
      </label>
    </div>
  );
}
