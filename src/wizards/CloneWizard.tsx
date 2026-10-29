"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Copy, ShieldAlert } from "lucide-react";
import { Button } from "@/components/motion/button";
import { Checkbox } from "@/components/motion/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/motion/select";
import {
  CenterMorphModal,
  CenterMorphModalContent,
} from "@/components/motion/center-morph-modal";
import { WizardChrome, PagedSection } from "@/components/clone/WizardChrome";
import { DriveTable } from "@/components/clone/DriveTable";
import { PartitionMappingTable } from "@/components/clone/PartitionMappingTable";
import { DriveActions, ProgressPane, SummaryPane, SummaryCard } from "@/components/clone/Shared";
import { DestinationGuard } from "@/components/clone/DestinationGuard";
import { useToasts } from "@/components/clone/Toasts";
import {
  ClonePage,
  CLONE_PAGE_TITLES,
  type CloneProgressEvent,
  type CloneRequest,
  type CloneSummary,
  type Drive,
  type PartitionMappingRow,
  type PostCloneAction,
  NOT_RESTORING_PARTITION_KEY,
} from "@/lib/clone/types";
import { describeDrive, formatBytes } from "@/lib/clone/format";
import {
  checkFitsDestination,
  useExistingDrivePartitionTable,
  useImagePartitionTable,
  validateDestination,
  validateSource,
} from "@/lib/clone/mapping";
import {
  isNativeBackend,
  listDrives,
  startClone,
  type OperationHandle,
} from "@/lib/clone/backend";

const STEPS = [
  ClonePage.Introduction,
  ClonePage.SourceDrive,
  ClonePage.DestinationDrive,
  ClonePage.PartitionsToClone,
  ClonePage.ConfirmConfiguration,
  ClonePage.Progress,
  ClonePage.Summary,
].map((p) => ({ index: p, title: CLONE_PAGE_TITLES[p] }));

export function CloneWizard({ onHome }: { onHome: () => void }) {
  const toasts = useToasts();
  const [page, setPage] = useState<ClonePage>(ClonePage.Introduction);
  const [native, setNative] = useState<boolean | null>(null);
  const [drives, setDrives] = useState<Drive[]>([]);
  const [loadingDrives, setLoadingDrives] = useState(false);
  const [sourceKey, setSourceKey] = useState<string | null>(null);
  const [destKey, setDestKey] = useState<string | null>(null);
  const [overwriteTable, setOverwriteTable] = useState(true);
  const [rescue, setRescue] = useState(false);
  const [verify, setVerify] = useState(false);
  const [postAction, setPostAction] = useState<PostCloneAction>("nothing");
  const [mapping, setMapping] = useState<PartitionMappingRow[]>([]);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [destConfirmText, setDestConfirmText] = useState("");
  const [destReady, setDestReady] = useState(false);
  const [destConfirmDevice, setDestConfirmDevice] = useState<string | undefined>(undefined);
  const [progress, setProgress] = useState<CloneProgressEvent | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [summary, setSummary] = useState<CloneSummary | null>(null);
  const handleRef = useRef<OperationHandle | null>(null);

  const source = useMemo(() => drives.find((d) => d.key === sourceKey) ?? null, [drives, sourceKey]);
  const destination = useMemo(() => drives.find((d) => d.key === destKey) ?? null, [drives, destKey]);

  useEffect(() => {
    isNativeBackend().then(setNative);
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

  const rebuildMapping = useCallback(
    (ov: boolean) => {
      if (!source || !destination) return;
      setMapping(ov ? useImagePartitionTable(source, destination) : useExistingDrivePartitionTable(source));
    },
    [source, destination],
  );

  const goNext = useCallback(async () => {
    switch (page) {
      case ClonePage.Introduction:
        await refreshDrives();
        setPage(ClonePage.SourceDrive);
        break;
      case ClonePage.SourceDrive: {
        if (!source) return toasts.push("warning", "Select a source drive to continue.");
        const err = validateSource(source);
        if (err) return toasts.push("error", err);
        setPage(ClonePage.DestinationDrive);
        break;
      }
      case ClonePage.DestinationDrive: {
        if (!destination) return toasts.push("warning", "Select a destination drive to continue.");
        const err = validateDestination(source!.key, destination.key);
        if (err) return toasts.push("error", err);
        rebuildMapping(overwriteTable);
        setPage(ClonePage.PartitionsToClone);
        break;
      }
      case ClonePage.PartitionsToClone: {
        const anySelected = mapping.some((m) => m.enabled && m.destKey !== NOT_RESTORING_PARTITION_KEY);
        if (!anySelected) return toasts.push("warning", "Select at least one partition to clone.");
        const fitErr = checkFitsDestination(source!, destination!, mapping, overwriteTable);
        if (fitErr) return toasts.push("error", fitErr);
        setPage(ClonePage.ConfirmConfiguration);
        break;
      }
      case ClonePage.ConfirmConfiguration:
        if (!destReady) {
          toasts.push("warning", "Confirm the destination before continuing.");
          return;
        }
        setConfirmOpen(true);
        break;
      case ClonePage.Summary:
        reset();
        break;
      default:
        break;
    }
  }, [page, source, destination, overwriteTable, mapping, destReady, refreshDrives, rebuildMapping, toasts]);

  const goBack = useCallback(() => {
    setPage((p) =>
      p === ClonePage.SourceDrive
        ? ClonePage.Introduction
        : p === ClonePage.DestinationDrive
          ? ClonePage.SourceDrive
          : p === ClonePage.PartitionsToClone
            ? ClonePage.DestinationDrive
            : p === ClonePage.ConfirmConfiguration
              ? ClonePage.PartitionsToClone
              : p,
    );
  }, []);

  function reset() {
    setPage(ClonePage.Introduction);
    setSourceKey(null);
    setDestKey(null);
    setOverwriteTable(true);
    setRescue(false);
    setVerify(false);
    setPostAction("nothing");
    setMapping([]);
    setProgress(null);
    setLog([]);
    setSummary(null);
    setDestConfirmText("");
    setDestReady(false);
    setDestConfirmDevice(undefined);
    handleRef.current = null;
  }

  const beginClone = useCallback(async () => {
    setConfirmOpen(false);
    setPage(ClonePage.Progress);
    setProgress({ status: "preparing", overallFraction: 0, current: null, message: "Starting…" });
    setLog([]);
    const request: CloneRequest = {
      sourceDriveKey: source!.key,
      destinationDriveKey: destination!.key,
      overwritePartitionTable: overwriteTable,
      rescue,
      postAction,
      mapping,
      confirmDevice: destConfirmDevice,
      verify,
    };
    try {
      const handle = await startClone(request, (event) => {
        setProgress(event);
        if (event.logLine) setLog((prev) => [...prev, event.logLine!]);
      });
      handleRef.current = handle;
      const result = await handle.done;
      setSummary(result);
      setPage(ClonePage.Summary);
      if (result.status === "completed") toasts.push("info", "Clone completed.");
      else if (result.status === "failed") toasts.push("error", "Clone failed. See summary.");
    } catch (err) {
      setSummary({ status: "failed", title: "Clone failed", lines: [String(err)], durationSeconds: 0 });
      setPage(ClonePage.Summary);
      toasts.push("error", String(err));
    }
  }, [source, destination, overwriteTable, rescue, postAction, mapping, destConfirmDevice, verify, toasts]);

  const toggleRow = useCallback((key: string, enabled: boolean) => {
    setMapping((prev) => prev.map((m) => (m.sourceKey === key ? { ...m, enabled } : m)));
  }, []);

  const remapRow = useCallback(
    (key: string, newDestKey: string) => {
      setMapping((prev) =>
        prev.map((m) => {
          if (m.sourceKey !== key) return m;
          const dp = destination?.partitions.find((p) => p.key === newDestKey);
          return {
            ...m,
            destKey: newDestKey,
            destDescription:
              newDestKey === NOT_RESTORING_PARTITION_KEY || !dp
                ? "— do not clone —"
                : `${dp.key}: ${formatBytes(dp.sizeBytes)} ${dp.filesystem ?? "raw"}`,
            enabled: newDestKey !== NOT_RESTORING_PARTITION_KEY ? m.enabled : false,
          };
        }),
      );
    },
    [destination],
  );

  const isRunning = page === ClonePage.Progress;
  const canGoBack = page > ClonePage.Introduction && page < ClonePage.Progress;

  const footer = isRunning ? null : (
    <>
      <Button variant="ghost" onClick={goBack} disabled={!canGoBack} className={canGoBack ? "" : "invisible"}>
        Back
      </Button>
      <Button
        variant={page === ClonePage.ConfirmConfiguration ? "destructive" : "primary"}
        onClick={goNext}
        ripple
        disabled={page === ClonePage.ConfirmConfiguration && !destReady}
      >
        {page === ClonePage.Introduction && "Get started"}
        {(page === ClonePage.SourceDrive || page === ClonePage.DestinationDrive || page === ClonePage.PartitionsToClone) && "Next"}
        {page === ClonePage.ConfirmConfiguration && "Clone now"}
        {page === ClonePage.Summary && "Clone another disk"}
      </Button>
    </>
  );

  return (
    <>
      <WizardChrome
        brand={{ icon: <Copy className="h-5 w-5" />, title: "Clone", subtitle: "Disk → disk" }}
        steps={STEPS}
        current={page}
        native={native}
        pageKey={page}
        footer={footer}
        onHome={onHome}
      >
        {page === ClonePage.Introduction && (
          <PagedSection
            title="Clone a disk, drive to drive"
            subtitle="Copy a whole disk — partition table and filesystems — directly onto another, with no intermediate image file."
          >
            <ul className="flex flex-col gap-2 text-sm text-muted-foreground">
              {[
                "Copies only used filesystem space, not empty blocks",
                "Optionally recreates the source partition table on the destination",
                "Rescue mode to continue past bad sectors",
              ].map((l) => (
                <li key={l} className="flex items-center gap-2">
                  <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                  {l}
                </li>
              ))}
            </ul>
          </PagedSection>
        )}

        {page === ClonePage.SourceDrive && (
          <PagedSection title="Select the source drive" subtitle="The disk to copy from. Read, not modified.">
            <DriveActions loading={loadingDrives} onRefresh={refreshDrives} />
            <DriveTable drives={drives} selectedKey={sourceKey} onSelect={setSourceKey} />
          </PagedSection>
        )}

        {page === ClonePage.DestinationDrive && (
          <PagedSection title="Select the destination drive" subtitle="Everything on this disk will be permanently overwritten.">
            <DriveActions loading={loadingDrives} onRefresh={refreshDrives} />
            <DriveTable
              drives={drives}
              selectedKey={destKey}
              onSelect={setDestKey}
              disabledKey={sourceKey}
              disabledReason="This is the source drive."
            />
          </PagedSection>
        )}

        {page === ClonePage.PartitionsToClone && source && destination && (
          <PagedSection title="Choose what to clone" subtitle={`${describeDrive(source)}  →  ${describeDrive(destination)}`}>
            <div className="mb-4 flex flex-col gap-3 rounded-xl border border-border p-4">
              <Checkbox
                checked={overwriteTable}
                onCheckedChange={(v) => {
                  setOverwriteTable(v);
                  rebuildMapping(v);
                }}
                label="Overwrite the destination partition table"
                description="Recreate the source layout on the destination (recommended for a full disk clone). Turn off to map into the destination's existing partitions."
              />
            </div>
            <PartitionMappingTable rows={mapping} destination={destination} locked={overwriteTable} onToggle={toggleRow} onRemap={remapRow} />
          </PagedSection>
        )}

        {page === ClonePage.ConfirmConfiguration && source && destination && (
          <PagedSection title="Confirm the clone" subtitle="Review the operation before any data is written.">
            <div className="flex flex-col gap-4">
              <div className="grid grid-cols-2 gap-3">
                <SummaryCard label="Source" value={describeDrive(source)} />
                <SummaryCard label="Destination" value={describeDrive(destination)} danger />
              </div>
              <div className="rounded-xl border border-border p-4">
                <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  Partitions to clone ({mapping.filter((m) => m.enabled && m.destKey !== NOT_RESTORING_PARTITION_KEY).length})
                </div>
                <ul className="flex flex-col gap-1.5 text-sm text-foreground">
                  {mapping
                    .filter((m) => m.enabled && m.destKey !== NOT_RESTORING_PARTITION_KEY)
                    .map((m) => (
                      <li key={m.sourceKey} className="flex items-center gap-2">
                        <span className="truncate">{m.sourceKey.replace(/^\/dev\//, "")}</span>
                        <span className="text-muted-foreground">→</span>
                        <span className="truncate">{m.destKey.replace(/^\/dev\//, "")}</span>
                      </li>
                    ))}
                </ul>
                <div className="mt-3 text-xs text-muted-foreground">
                  Partition table: {overwriteTable ? "overwrite destination with source layout" : "keep destination layout"}
                </div>
              </div>
              <div className="flex flex-col gap-4 rounded-xl border border-border p-4">
                <Checkbox
                  checked={rescue}
                  onCheckedChange={setRescue}
                  label="Enable rescue mode"
                  description="Continue past read errors (bad sectors) instead of aborting. Slower."
                />
                <Checkbox
                  checked={verify}
                  onCheckedChange={setVerify}
                  label="Verify after cloning"
                  description="Re-read the destination and compare it against the source (SHA-256). Adds time but confirms an exact copy."
                />
                <div className="flex items-center justify-between gap-4">
                  <div className="text-sm">
                    <div className="text-foreground">When finished</div>
                    <div className="text-xs text-muted-foreground">Action after the clone completes.</div>
                  </div>
                  <div className="w-44">
                    <Select value={postAction} onValueChange={(v) => setPostAction(v as PostCloneAction)}>
                      <SelectTrigger className="rounded-lg">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="nothing">Do nothing</SelectItem>
                        <SelectItem value="reboot">Reboot</SelectItem>
                        <SelectItem value="shutdown">Shut down</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>
              </div>
              <DestinationGuard
                destination={destination.key}
                source={source.key}
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

        {page === ClonePage.Progress && (
          <ProgressPane title="Cloning" progress={progress} log={log} onCancel={() => handleRef.current?.cancel()} />
        )}

        {page === ClonePage.Summary && summary && <SummaryPane summary={summary} />}
      </WizardChrome>

      <CenterMorphModal open={confirmOpen} onOpenChange={setConfirmOpen}>
        <CenterMorphModalContent ariaLabel="Confirm clone" showCloseButton={false}>
          <div className="p-6">
            <div className="mb-3 flex items-center gap-2 text-destructive">
              <ShieldAlert className="h-5 w-5" />
              <h2 className="text-lg font-semibold">Permanently overwrite destination?</h2>
            </div>
            <p className="text-sm text-muted-foreground">
              This will erase all data on <span className="font-medium text-foreground">{destination ? describeDrive(destination) : ""}</span> and
              replace it with a clone of <span className="font-medium text-foreground">{source ? describeDrive(source) : ""}</span>. This cannot be undone.
            </p>
            <div className="mt-6 flex justify-end gap-2">
              <Button variant="outline" onClick={() => setConfirmOpen(false)}>Cancel</Button>
              <Button variant="destructive" onClick={beginClone} ripple>Yes, clone and overwrite</Button>
            </div>
          </div>
        </CenterMorphModalContent>
      </CenterMorphModal>
    </>
  );
}
