"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { HardDriveDownload, Folder } from "lucide-react";
import { Button } from "@/components/motion/button";
import { Checkbox } from "@/components/motion/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/motion/select";
import { WizardChrome, PagedSection } from "@/components/clone/WizardChrome";
import { DriveTable } from "@/components/clone/DriveTable";
import { DriveActions, ProgressPane, SummaryPane, SummaryCard } from "@/components/clone/Shared";
import { useToasts } from "@/components/clone/Toasts";
import type { CloneProgressEvent, CloneSummary, Drive, Partition } from "@/lib/clone/types";
import type { BackupRequest, CompressionFormat, DirEntryInfo } from "@/lib/clone/ops";
import { describeDrive, describePartition, formatBytes } from "@/lib/clone/format";
import {
  isNativeBackend,
  listDirs,
  listDrives,
  preflight,
  startBackup,
  type OperationHandle,
} from "@/lib/clone/backend";

enum BPage {
  SourceDrive = 0,
  Partitions = 1,
  Destination = 2,
  Options = 3,
  Confirm = 4,
  Progress = 5,
  Summary = 6,
}
const STEPS = [
  { index: BPage.SourceDrive, title: "Source drive" },
  { index: BPage.Partitions, title: "Partitions" },
  { index: BPage.Destination, title: "Destination" },
  { index: BPage.Options, title: "Name & compression" },
  { index: BPage.Confirm, title: "Confirm" },
  { index: BPage.Progress, title: "Backing up" },
  { index: BPage.Summary, title: "Summary" },
];

function defaultImageName(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}-img-rabobi`;
}

export function BackupWizard({ onHome }: { onHome: () => void }) {
  const toasts = useToasts();
  const [page, setPage] = useState<BPage>(BPage.SourceDrive);
  const [native, setNative] = useState<boolean | null>(null);
  const [os, setOs] = useState<string>("other");
  const [drives, setDrives] = useState<Drive[]>([]);
  const [loadingDrives, setLoadingDrives] = useState(false);
  const [sourceKey, setSourceKey] = useState<string | null>(null);
  const [selectedParts, setSelectedParts] = useState<Set<string>>(new Set());
  const [destPath, setDestPath] = useState<string>("/mnt/backup");
  const [dirs, setDirs] = useState<DirEntryInfo[]>([]);
  const [imageName, setImageName] = useState<string>(defaultImageName());
  const [compression, setCompression] = useState<CompressionFormat>("zstd");
  const [rescue, setRescue] = useState(false);
  const [verify, setVerify] = useState(false);
  const [notes, setNotes] = useState("");
  const [progress, setProgress] = useState<CloneProgressEvent | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [summary, setSummary] = useState<CloneSummary | null>(null);
  const handleRef = useRef<OperationHandle | null>(null);

  const source = useMemo(() => drives.find((d) => d.key === sourceKey) ?? null, [drives, sourceKey]);
  const backupable = useMemo(
    () => (source?.partitions ?? []).filter((p) => !p.isExtended && p.filesystem !== "swap"),
    [source],
  );

  useEffect(() => {
    isNativeBackend().then(setNative);
    preflight()
      .then((pf) => {
        setOs(pf.os);
        if (pf.os === "windows") {
          // Windows has no zstd; default to gzip and a Windows-style path.
          setCompression("gzip");
          setDestPath("D:\\");
        }
      })
      .catch(() => undefined);
    listDrives().then(setDrives).catch(() => undefined);
  }, []);

  const refreshDrives = useCallback(async () => {
    setLoadingDrives(true);
    try {
      setDrives(await listDrives());
    } catch (err) {
      toasts.push("error", `Could not enumerate drives: ${String(err)}`);
    } finally {
      setLoadingDrives(false);
    }
  }, [toasts]);

  const browse = useCallback(
    async (path: string) => {
      try {
        setDestPath(path);
        setDirs(await listDirs(path));
      } catch (err) {
        toasts.push("error", `Cannot open ${path}: ${String(err)}`);
      }
    },
    [toasts],
  );

  const togglePart = (key: string, on: boolean) =>
    setSelectedParts((prev) => {
      const next = new Set(prev);
      if (on) next.add(key);
      else next.delete(key);
      return next;
    });

  const destDir = `${destPath.replace(/\/$/, "")}/${imageName}`;

  const goNext = useCallback(async () => {
    switch (page) {
      case BPage.SourceDrive:
        if (!source) return toasts.push("warning", "Select a source drive.");
        setSelectedParts(new Set(backupable.map((p) => p.key)));
        setPage(BPage.Partitions);
        break;
      case BPage.Partitions:
        if (selectedParts.size === 0) return toasts.push("warning", "Select at least one partition.");
        await browse(destPath);
        setPage(BPage.Destination);
        break;
      case BPage.Destination:
        setPage(BPage.Options);
        break;
      case BPage.Options:
        if (!imageName.trim()) return toasts.push("warning", "Enter an image name.");
        setPage(BPage.Confirm);
        break;
      case BPage.Confirm:
        await begin();
        break;
      case BPage.Summary:
        reset();
        break;
      default:
        break;
    }
  }, [page, source, backupable, selectedParts, destPath, imageName, browse, toasts]);

  const goBack = useCallback(() => {
    setPage((p) => (p > BPage.SourceDrive && p < BPage.Progress ? p - 1 : p));
  }, []);

  function reset() {
    setPage(BPage.SourceDrive);
    setSourceKey(null);
    setSelectedParts(new Set());
    setImageName(defaultImageName());
    setCompression("zstd");
    setRescue(false);
    setVerify(false);
    setNotes("");
    setProgress(null);
    setLog([]);
    setSummary(null);
    handleRef.current = null;
  }

  const begin = useCallback(async () => {
    setPage(BPage.Progress);
    setProgress({ status: "preparing", overallFraction: 0, current: null, message: "Starting…" });
    setLog([]);
    const request: BackupRequest = {
      sourceDriveKey: source!.key,
      partitions: Array.from(selectedParts).map((k) => k.replace(/^\/dev\//, "")),
      destDir,
      compression,
      rescue,
      notes,
      verify,
    };
    try {
      const handle = await startBackup(request, (event) => {
        setProgress(event);
        if (event.logLine) setLog((prev) => [...prev, event.logLine!]);
      });
      handleRef.current = handle;
      const result = await handle.done;
      setSummary(result);
      setPage(BPage.Summary);
      toasts.push(result.status === "completed" ? "info" : "error", result.title);
    } catch (err) {
      setSummary({ status: "failed", title: "Backup failed", lines: [String(err)], durationSeconds: 0 });
      setPage(BPage.Summary);
      toasts.push("error", String(err));
    }
  }, [source, selectedParts, destDir, compression, rescue, notes, verify, toasts]);

  const isRunning = page === BPage.Progress;
  const canGoBack = page > BPage.SourceDrive && page < BPage.Progress;

  const footer = isRunning ? null : (
    <>
      <Button variant="ghost" onClick={goBack} disabled={!canGoBack} className={canGoBack ? "" : "invisible"}>
        Back
      </Button>
      <Button variant={page === BPage.Confirm ? "primary" : "primary"} onClick={goNext} ripple>
        {page === BPage.Confirm ? "Start backup" : page === BPage.Summary ? "Back up another disk" : "Next"}
      </Button>
    </>
  );

  return (
    <WizardChrome
      brand={{ icon: <HardDriveDownload className="h-5 w-5" />, title: "Backup", subtitle: "Disk → image" }}
      steps={STEPS}
      current={page}
      native={native}
      pageKey={page}
      footer={footer}
      onHome={onHome}
    >
      {page === BPage.SourceDrive && (
        <PagedSection title="Select the drive to back up" subtitle="A compressed image of this disk will be written to a folder.">
          <DriveActions loading={loadingDrives} onRefresh={refreshDrives} />
          <DriveTable drives={drives} selectedKey={sourceKey} onSelect={setSourceKey} />
        </PagedSection>
      )}

      {page === BPage.Partitions && source && (
        <PagedSection title="Choose partitions to back up" subtitle={describeDrive(source)}>
          {os === "windows" ? (
            <div className="mb-4 rounded-xl border border-yellow-500/30 bg-yellow-500/5 p-4 text-sm text-muted-foreground">
              On Windows, Rabobi writes a <span className="font-medium text-foreground">full-disk raw image</span> —
              the whole disk is captured regardless of the selection below.
            </div>
          ) : null}
          <div className="flex flex-col gap-2 rounded-xl border border-border p-4">
            {backupable.length === 0 ? (
              <p className="text-sm text-muted-foreground">No backable partitions found on this disk.</p>
            ) : (
              backupable.map((p: Partition) => (
                <Checkbox
                  key={p.key}
                  checked={selectedParts.has(p.key)}
                  onCheckedChange={(v) => togglePart(p.key, v)}
                  label={describePartition(p)}
                />
              ))
            )}
          </div>
        </PagedSection>
      )}

      {page === BPage.Destination && (
        <PagedSection title="Choose the destination folder" subtitle="The image sub-folder will be created inside this location.">
          <div className="mb-3 flex items-center gap-2 text-sm text-muted-foreground">
            <Folder className="h-4 w-4" />
            <span className="font-mono">{destPath}</span>
          </div>
          <div className="overflow-hidden rounded-xl border border-border">
            <button
              type="button"
              onClick={() => browse(destPath.split("/").slice(0, -1).join("/") || "/")}
              className="w-full border-b border-border px-4 py-2.5 text-left text-sm text-muted-foreground hover:bg-muted/50"
            >
              ../ (up one level)
            </button>
            <div className="max-h-72 divide-y divide-border overflow-y-auto">
              {dirs.length === 0 ? (
                <div className="px-4 py-3 text-sm text-muted-foreground">(no subfolders)</div>
              ) : (
                dirs.map((d) => (
                  <button
                    key={d.path}
                    type="button"
                    onClick={() => browse(d.path)}
                    className="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm text-foreground hover:bg-muted/50"
                  >
                    <Folder className="h-4 w-4 text-muted-foreground" />
                    {d.name}
                  </button>
                ))
              )}
            </div>
          </div>
        </PagedSection>
      )}

      {page === BPage.Options && (
        <PagedSection title="Name & compression" subtitle="Name the image and choose how it's compressed.">
          <div className="flex flex-col gap-4">
            <label className="flex flex-col gap-1.5">
              <span className="text-sm font-medium text-foreground">Image name</span>
              <input
                value={imageName}
                onChange={(e) => setImageName(e.target.value)}
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
              />
              <span className="font-mono text-xs text-muted-foreground">{destDir}</span>
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="text-sm font-medium text-foreground">Notes (optional)</span>
              <textarea
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                rows={2}
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
              />
            </label>
            <div className="flex items-center justify-between gap-4">
              <div className="text-sm">
                <div className="text-foreground">Compression</div>
                <div className="text-xs text-muted-foreground">zstd is fast and compresses well.</div>
              </div>
              <div className="w-44">
                <Select value={compression} onValueChange={(v) => setCompression(v as CompressionFormat)}>
                  <SelectTrigger className="rounded-lg">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {os !== "windows" ? (
                      <SelectItem value="zstd">zstd (recommended)</SelectItem>
                    ) : null}
                    <SelectItem value="gzip">
                      {os === "windows" ? "gzip (recommended)" : "gzip"}
                    </SelectItem>
                    <SelectItem value="uncompressed">uncompressed</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <Checkbox
              checked={rescue}
              onCheckedChange={setRescue}
              label="Enable rescue mode"
              description="Continue past read errors (bad sectors). Slower."
            />
            <Checkbox
              checked={verify}
              onCheckedChange={setVerify}
              label="Verify (record integrity digest)"
              description="Hash the source while imaging and store a SHA-256 in the image, so a later restore can self-check it."
            />
          </div>
        </PagedSection>
      )}

      {page === BPage.Confirm && source && (
        <PagedSection title="Confirm the backup" subtitle="Review before imaging begins.">
          <div className="flex flex-col gap-4">
            <div className="grid grid-cols-2 gap-3">
              <SummaryCard label="Source" value={describeDrive(source)} />
              <SummaryCard label="Compression" value={compression} />
            </div>
            <SummaryCard label="Destination" value={destDir} />
            <div className="rounded-xl border border-border p-4">
              <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Partitions ({selectedParts.size})
              </div>
              <ul className="flex flex-col gap-1.5 text-sm text-foreground">
                {Array.from(selectedParts).map((k) => {
                  const p = backupable.find((bp) => bp.key === k);
                  return (
                    <li key={k} className="font-mono text-[12px]">
                      {k.replace(/^\/dev\//, "")} {p ? `· ${p.filesystem ?? "raw"} · ${formatBytes(p.usedBytes ?? p.sizeBytes)}` : ""}
                    </li>
                  );
                })}
              </ul>
            </div>
          </div>
        </PagedSection>
      )}

      {page === BPage.Progress && (
        <ProgressPane title="Backing up" progress={progress} log={log} onCancel={() => handleRef.current?.cancel()} />
      )}

      {page === BPage.Summary && summary && <SummaryPane summary={summary} />}
    </WizardChrome>
  );
}
