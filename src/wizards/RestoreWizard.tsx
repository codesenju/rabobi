"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { HardDriveUpload, Folder, ShieldAlert, Archive } from "lucide-react";
import { motion } from "motion/react";
import { Button } from "@/components/motion/button";
import { Checkbox } from "@/components/motion/checkbox";
import {
  CenterMorphModal,
  CenterMorphModalContent,
} from "@/components/motion/center-morph-modal";
import { WizardChrome, PagedSection } from "@/components/clone/WizardChrome";
import { DriveTable } from "@/components/clone/DriveTable";
import { DriveActions, ProgressPane, SummaryPane, SummaryCard } from "@/components/clone/Shared";
import { DestinationGuard } from "@/components/clone/DestinationGuard";
import { useToasts } from "@/components/clone/Toasts";
import { SPRING_LAYOUT } from "@/lib/ease";
import { cn } from "@/lib/utils";
import type { CloneProgressEvent, CloneSummary, Drive, PartitionMappingRow } from "@/lib/clone/types";
import type { ImageInfo, RestoreRequest } from "@/lib/clone/ops";
import { describeDrive, formatBytes } from "@/lib/clone/format";
import {
  isNativeBackend,
  listDirs,
  listDrives,
  scanImages,
  startRestore,
  type OperationHandle,
} from "@/lib/clone/backend";
import type { DirEntryInfo } from "@/lib/clone/ops";

enum RPage {
  SourceFolder = 0,
  Image = 1,
  Destination = 2,
  Confirm = 3,
  Progress = 4,
  Summary = 5,
}
const STEPS = [
  { index: RPage.SourceFolder, title: "Image folder" },
  { index: RPage.Image, title: "Select image" },
  { index: RPage.Destination, title: "Destination" },
  { index: RPage.Confirm, title: "Confirm" },
  { index: RPage.Progress, title: "Restoring" },
  { index: RPage.Summary, title: "Summary" },
];

export function RestoreWizard({ onHome }: { onHome: () => void }) {
  const toasts = useToasts();
  const [page, setPage] = useState<RPage>(RPage.SourceFolder);
  const [native, setNative] = useState<boolean | null>(null);
  const [path, setPath] = useState("/mnt/backup");
  const [dirs, setDirs] = useState<DirEntryInfo[]>([]);
  const [images, setImages] = useState<ImageInfo[]>([]);
  const [imagePath, setImagePath] = useState<string | null>(null);
  const [drives, setDrives] = useState<Drive[]>([]);
  const [loadingDrives, setLoadingDrives] = useState(false);
  const [destKey, setDestKey] = useState<string | null>(null);
  const [overwriteTable, setOverwriteTable] = useState(true);
  const [rescue, setRescue] = useState(false);
  const [verify, setVerify] = useState(false);
  const [destConfirmText, setDestConfirmText] = useState("");
  const [destReady, setDestReady] = useState(false);
  const [destConfirmDevice, setDestConfirmDevice] = useState<string | undefined>(undefined);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [progress, setProgress] = useState<CloneProgressEvent | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [summary, setSummary] = useState<CloneSummary | null>(null);
  const handleRef = useRef<OperationHandle | null>(null);

  const image = useMemo(() => images.find((i) => i.path === imagePath) ?? null, [images, imagePath]);
  const destination = useMemo(() => drives.find((d) => d.key === destKey) ?? null, [drives, destKey]);

  useEffect(() => {
    isNativeBackend().then(setNative);
  }, []);

  const browse = useCallback(
    async (p: string) => {
      try {
        setPath(p);
        const [d, imgs] = await Promise.all([listDirs(p), scanImages(p)]);
        setDirs(d);
        setImages(imgs);
      } catch (err) {
        toasts.push("error", `Cannot open ${p}: ${String(err)}`);
      }
    },
    [toasts],
  );

  useEffect(() => {
    browse(path);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refreshDrives = useCallback(async () => {
    setLoadingDrives(true);
    try {
      setDrives(await listDrives());
    } catch (err) {
      toasts.push("error", String(err));
    } finally {
      setLoadingDrives(false);
    }
  }, [toasts]);

  const goNext = useCallback(async () => {
    switch (page) {
      case RPage.SourceFolder:
        if (images.length === 0) return toasts.push("warning", "No images found here. Browse to a folder that contains a backup.");
        setPage(RPage.Image);
        break;
      case RPage.Image:
        if (!image) return toasts.push("warning", "Select an image to restore.");
        await refreshDrives();
        setPage(RPage.Destination);
        break;
      case RPage.Destination:
        if (!destination) return toasts.push("warning", "Select a destination drive.");
        setPage(RPage.Confirm);
        break;
      case RPage.Confirm:
        if (!destReady) {
          toasts.push("warning", "Confirm the destination before continuing.");
          return;
        }
        setConfirmOpen(true);
        break;
      case RPage.Summary:
        reset();
        break;
      default:
        break;
    }
  }, [page, images, image, destination, destReady, refreshDrives, toasts]);

  const goBack = useCallback(() => {
    setPage((p) => (p > RPage.SourceFolder && p < RPage.Progress ? p - 1 : p));
  }, []);

  function reset() {
    setPage(RPage.SourceFolder);
    setImagePath(null);
    setDestKey(null);
    setOverwriteTable(true);
    setRescue(false);
    setVerify(false);
    setDestConfirmText("");
    setDestReady(false);
    setDestConfirmDevice(undefined);
    setProgress(null);
    setLog([]);
    setSummary(null);
    handleRef.current = null;
  }

  const begin = useCallback(async () => {
    setConfirmOpen(false);
    setPage(RPage.Progress);
    setProgress({ status: "preparing", overallFraction: 0, current: null, message: "Starting…" });
    setLog([]);
    const mapping: PartitionMappingRow[] = (image?.partitions ?? []).map((p, idx) => ({
      sourceKey: p.key,
      enabled: true,
      sourceDescription: `${p.key} ${p.filesystem ?? "raw"}`,
      destKey: `${destination!.key}${idx + 1}`,
      destDescription: "",
    }));
    const request: RestoreRequest = {
      imageDir: image!.path,
      destinationDriveKey: destination!.key,
      overwritePartitionTable: overwriteTable,
      rescue,
      mapping,
      confirmDevice: destConfirmDevice,
      verify,
    };
    try {
      const handle = await startRestore(request, (event) => {
        setProgress(event);
        if (event.logLine) setLog((prev) => [...prev, event.logLine!]);
      });
      handleRef.current = handle;
      const result = await handle.done;
      setSummary(result);
      setPage(RPage.Summary);
      toasts.push(result.status === "completed" ? "info" : "error", result.title);
    } catch (err) {
      setSummary({ status: "failed", title: "Restore failed", lines: [String(err)], durationSeconds: 0 });
      setPage(RPage.Summary);
      toasts.push("error", String(err));
    }
  }, [image, destination, overwriteTable, rescue, destConfirmDevice, verify, toasts]);

  const isRunning = page === RPage.Progress;
  const canGoBack = page > RPage.SourceFolder && page < RPage.Progress;

  const footer = isRunning ? null : (
    <>
      <Button variant="ghost" onClick={goBack} disabled={!canGoBack} className={canGoBack ? "" : "invisible"}>
        Back
      </Button>
      <Button
        variant={page === RPage.Confirm ? "destructive" : "primary"}
        onClick={goNext}
        ripple
        disabled={page === RPage.Confirm && !destReady}
      >
        {page === RPage.Confirm ? "Restore now" : page === RPage.Summary ? "Restore another image" : "Next"}
      </Button>
    </>
  );

  return (
    <>
      <WizardChrome
        brand={{ icon: <HardDriveUpload className="h-5 w-5" />, title: "Restore", subtitle: "Image → disk" }}
        steps={STEPS}
        current={page}
        native={native}
        pageKey={page}
        footer={footer}
        onHome={onHome}
      >
        {page === RPage.SourceFolder && (
          <PagedSection title="Find the image folder" subtitle="Browse to the location that contains your backup images.">
            <div className="mb-3 flex items-center gap-2 text-sm text-muted-foreground">
              <Folder className="h-4 w-4" />
              <span className="font-mono">{path}</span>
            </div>
            <div className="overflow-hidden rounded-xl border border-border">
              <button
                type="button"
                onClick={() => browse(path.split("/").slice(0, -1).join("/") || "/")}
                className="w-full border-b border-border px-4 py-2.5 text-left text-sm text-muted-foreground hover:bg-muted/50"
              >
                ../ (up one level)
              </button>
              <div className="max-h-56 divide-y divide-border overflow-y-auto">
                {dirs.map((d) => (
                  <button
                    key={d.path}
                    type="button"
                    onClick={() => browse(d.path)}
                    className="flex w-full items-center gap-2 px-4 py-2.5 text-left text-sm text-foreground hover:bg-muted/50"
                  >
                    <Folder className="h-4 w-4 text-muted-foreground" />
                    {d.name}
                  </button>
                ))}
              </div>
            </div>
            {images.length > 0 ? (
              <p className="mt-3 text-sm text-muted-foreground">
                Found {images.length} image{images.length === 1 ? "" : "s"} here.
              </p>
            ) : null}
          </PagedSection>
        )}

        {page === RPage.Image && (
          <PagedSection title="Select an image" subtitle={`Images found in ${path}`}>
            <div className="flex flex-col gap-2">
              {images.map((img) => {
                const selected = img.path === imagePath;
                return (
                  <button
                    key={img.path}
                    type="button"
                    onClick={() => setImagePath(img.path)}
                    className={cn(
                      "relative overflow-hidden rounded-xl border p-4 text-left transition-colors",
                      selected ? "border-primary" : "border-border hover:border-muted-foreground",
                    )}
                  >
                    {selected ? (
                      <motion.span layoutId="image-selected" transition={SPRING_LAYOUT} className="absolute inset-0 bg-muted" />
                    ) : null}
                    <div className="relative z-10 flex items-start gap-3">
                      <Archive className="mt-0.5 h-5 w-5 text-muted-foreground" />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-baseline justify-between gap-2">
                          <span className="truncate font-medium text-foreground">{img.name}</span>
                          <span className="shrink-0 text-sm tabular-nums text-muted-foreground">{formatBytes(img.sizeBytes)}</span>
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">
                          disk {img.disk} · {img.lastModified} · {img.partitions.length} partitions
                        </div>
                        {img.notes ? <div className="mt-1 text-xs text-foreground/70">{img.notes}</div> : null}
                        <div className="mt-2 flex flex-wrap gap-1.5">
                          {img.partitions.map((p) => (
                            <span key={p.key} className="rounded-md bg-background/60 px-2 py-0.5 text-[11px] text-muted-foreground ring-1 ring-border">
                              {p.key} · {p.filesystem ?? "raw"} · {p.compression} · {formatBytes(p.sizeBytes)}
                            </span>
                          ))}
                        </div>
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          </PagedSection>
        )}

        {page === RPage.Destination && (
          <PagedSection title="Select the destination drive" subtitle="Everything on this disk will be permanently overwritten.">
            <DriveActions loading={loadingDrives} onRefresh={refreshDrives} />
            <DriveTable drives={drives} selectedKey={destKey} onSelect={setDestKey} />
          </PagedSection>
        )}

        {page === RPage.Confirm && image && destination && (
          <PagedSection title="Confirm the restore" subtitle="Review before any data is written.">
            <div className="flex flex-col gap-4">
              <div className="grid grid-cols-2 gap-3">
                <SummaryCard label="Image" value={image.name} />
                <SummaryCard label="Destination" value={describeDrive(destination)} danger />
              </div>
              <div className="rounded-xl border border-border p-4">
                <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  Partitions to restore ({image.partitions.length})
                </div>
                <ul className="flex flex-col gap-1.5 font-mono text-[12px] text-foreground">
                  {image.partitions.map((p, idx) => (
                    <li key={p.key}>
                      {p.key} → {destination.key.replace(/^\/dev\//, "")}
                      {idx + 1} · {p.filesystem ?? "raw"} · {formatBytes(p.sizeBytes)}
                    </li>
                  ))}
                </ul>
              </div>
              <div className="flex flex-col gap-4 rounded-xl border border-border p-4">
                <Checkbox
                  checked={overwriteTable}
                  onCheckedChange={setOverwriteTable}
                  label="Overwrite the destination partition table"
                  description="Recreate the image's partition layout on the destination (recommended for a full disk restore)."
                />
                <Checkbox
                  checked={rescue}
                  onCheckedChange={setRescue}
                  label="Enable rescue mode"
                  description="Continue past errors instead of aborting."
                />
                <Checkbox
                  checked={verify}
                  onCheckedChange={setVerify}
                  label="Verify after restoring"
                  description="Hash each restored partition and record it in the operation log."
                />
              </div>
              <DestinationGuard
                destination={destination.key}
                source={null}
                confirmValue={destConfirmText}
                onConfirmChange={setDestConfirmText}
                onReadyChange={(ready, dev) => {
                  setDestReady(ready);
                  setDestConfirmDevice(dev);
                }}
              />
            </div>
          </PagedSection>
        )}

        {page === RPage.Progress && (
          <ProgressPane title="Restoring" progress={progress} log={log} onCancel={() => handleRef.current?.cancel()} />
        )}

        {page === RPage.Summary && summary && <SummaryPane summary={summary} />}
      </WizardChrome>

      <CenterMorphModal open={confirmOpen} onOpenChange={setConfirmOpen}>
        <CenterMorphModalContent ariaLabel="Confirm restore" showCloseButton={false}>
          <div className="p-6">
            <div className="mb-3 flex items-center gap-2 text-destructive">
              <ShieldAlert className="h-5 w-5" />
              <h2 className="text-lg font-semibold">Permanently overwrite destination?</h2>
            </div>
            <p className="text-sm text-muted-foreground">
              This will erase all data on{" "}
              <span className="font-medium text-foreground">{destination ? describeDrive(destination) : ""}</span> and
              replace it with the image{" "}
              <span className="font-medium text-foreground">{image?.name}</span>. This cannot be undone.
            </p>
            <div className="mt-6 flex justify-end gap-2">
              <Button variant="outline" onClick={() => setConfirmOpen(false)}>Cancel</Button>
              <Button variant="destructive" onClick={begin} ripple>Yes, restore and overwrite</Button>
            </div>
          </div>
        </CenterMorphModalContent>
      </CenterMorphModal>
    </>
  );
}
