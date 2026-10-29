"use client";
// Shared wizard sub-components used by clone/backup/restore.

import { useEffect, useRef } from "react";
import { Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/motion/button";
import { ProgressBar } from "@/components/clone/ProgressBar";
import type { CloneProgressEvent, CloneSummary } from "@/lib/clone/types";

export function DriveActions({ loading, onRefresh }: { loading: boolean; onRefresh: () => void }) {
  return (
    <div className="mb-3 flex justify-end">
      <Button variant="outline" size="sm" onClick={onRefresh} disabled={loading}>
        {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
        Rescan
      </Button>
    </div>
  );
}

export function SummaryCard({ label, value, danger }: { label: string; value: string; danger?: boolean }) {
  return (
    <div className="rounded-xl border border-border p-4">
      <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{label}</div>
      <div className={`mt-1 text-sm font-medium ${danger ? "text-destructive" : "text-foreground"}`}>{value}</div>
    </div>
  );
}

export function ProgressPane({
  title,
  progress,
  log,
  onCancel,
}: {
  title: string;
  progress: CloneProgressEvent | null;
  log: string[];
  onCancel: () => void;
}) {
  const logRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  const overall = progress?.overallFraction ?? 0;
  return (
    <section>
      <h1 className="text-2xl font-semibold tracking-tight text-foreground">{title}</h1>
      <p className="mt-1.5 text-sm text-muted-foreground">{progress?.message ?? "Working…"}</p>
      <div className="mt-8 flex flex-col gap-6">
        <div>
          <div className="mb-2 text-sm font-medium text-foreground">Overall progress</div>
          <ProgressBar fraction={overall} />
        </div>
        {progress?.current ? (
          <div>
            <div className="mb-2 text-sm text-muted-foreground">
              {progress.current.sourceKey} → {progress.current.destKey}
            </div>
            <ProgressBar fraction={progress.current.fraction} showPercent={false} />
            <p className="mt-1.5 text-xs tabular-nums text-muted-foreground">{progress.current.statusLine}</p>
          </div>
        ) : null}
        <div>
          <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">Activity log</div>
          <div
            ref={logRef}
            className="h-40 overflow-y-auto rounded-xl border border-border bg-background p-3 font-mono text-[11px] leading-relaxed text-muted-foreground"
          >
            {log.length === 0 ? (
              <span className="text-muted-foreground/60">Waiting for output…</span>
            ) : (
              log.map((line, i) => (
                <div key={i} className="whitespace-pre-wrap break-all">
                  {line}
                </div>
              ))
            )}
          </div>
        </div>
        <div className="flex justify-end">
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </div>
    </section>
  );
}

export function SummaryPane({ summary }: { summary: CloneSummary }) {
  const ok = summary.status === "completed";
  return (
    <section>
      <div
        className={`mb-4 inline-flex items-center gap-2 rounded-full px-3 py-1 text-sm font-medium ${
          ok
            ? "bg-primary/10 text-foreground"
            : summary.status === "cancelled"
              ? "bg-muted text-muted-foreground"
              : "bg-destructive/10 text-destructive"
        }`}
      >
        {summary.title}
      </div>
      <h1 className="text-2xl font-semibold tracking-tight text-foreground">
        {ok ? "Done" : summary.status === "cancelled" ? "Stopped" : "Something went wrong"}
      </h1>
      {summary.durationSeconds > 0 ? (
        <p className="mt-1.5 text-sm text-muted-foreground">
          Completed in {Math.floor(summary.durationSeconds / 60)}m {summary.durationSeconds % 60}s.
        </p>
      ) : null}
      {summary.warnings && summary.warnings.length > 0 ? (
        <div className="mt-4 rounded-xl border border-yellow-500/40 bg-yellow-500/5 p-4">
          <div className="mb-2 text-xs font-medium uppercase tracking-wide text-yellow-600 dark:text-yellow-500">
            Warnings ({summary.warnings.length})
          </div>
          <ul className="flex flex-col gap-1 text-sm text-foreground">
            {summary.warnings.map((w, i) => (
              <li key={i} className="flex items-start gap-2">
                <span className="mt-1.5 h-1 w-1 shrink-0 rounded-full bg-yellow-500" />
                {w}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      <div className="mt-6 rounded-xl border border-border p-4">
        <ul className="flex flex-col gap-1.5 text-sm text-foreground">
          {summary.lines.map((line, i) => (
            <li key={i} className="font-mono text-[12px]">
              {line}
            </li>
          ))}
        </ul>
      </div>
      {summary.logPath ? (
        <p className="mt-3 font-mono text-[11px] text-muted-foreground">
          Log: {summary.logPath}
        </p>
      ) : null}
    </section>
  );
}
