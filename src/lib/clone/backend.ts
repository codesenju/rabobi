/*
 * Backend bridge. Inside the Tauri shell it invokes Rust commands that run the
 * real Rescuezilla-equivalent pipeline (lsblk/sfdisk/parted/blkid to enumerate,
 * partclone to copy/image/restore). Outside Tauri (plain `vite dev`, or macOS
 * where the Linux tools are absent) it uses an in-browser simulation so the
 * whole UX is exercisable.
 */

import type {
  CloneProgressEvent,
  CloneRequest,
  CloneSummary,
  Drive,
} from "./types";
import type {
  BackupRequest,
  DestinationSafety,
  DirEntryInfo,
  ImageInfo,
  RestoreRequest,
} from "./ops";
import {
  simulateListDrives,
  SimulatedOperation,
  simulateScanImages,
  simulateListDirs,
  simulateCheckDestination,
} from "./simulate";

type ProgressCallback = (event: CloneProgressEvent) => void;

interface TauriApi {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  listen: (
    event: string,
    handler: (e: { payload: unknown }) => void,
  ) => Promise<() => void>;
}

async function getTauri(): Promise<TauriApi | null> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return null;
  }
  const core = await import("@tauri-apps/api/core");
  const event = await import("@tauri-apps/api/event");
  return {
    invoke: core.invoke as TauriApi["invoke"],
    listen: event.listen as unknown as TauriApi["listen"],
  };
}

export async function isNativeBackend(): Promise<boolean> {
  return (await getTauri()) !== null;
}

export interface ToolStatus {
  name: string;
  present: boolean;
}
export interface Preflight {
  os: "linux" | "windows" | "macos" | "other";
  supportedPlatform: boolean;
  isRoot: boolean;
  tools: ToolStatus[];
  allToolsPresent: boolean;
  ready: boolean;
  blockers: string[];
}

export async function preflight(): Promise<Preflight> {
  const tauri = await getTauri();
  if (!tauri) {
    // Simulation mode: not a real platform, but the UI is fully usable.
    return {
      os: "other",
      supportedPlatform: false,
      isRoot: false,
      tools: [],
      allToolsPresent: false,
      ready: false,
      blockers: ["Simulation mode — real disk operations run on a Linux host as root, or a Windows host as Administrator."],
    };
  }
  return tauri.invoke<Preflight>("preflight");
}

export async function listDrives(): Promise<Drive[]> {
  const tauri = await getTauri();
  if (!tauri) return simulateListDrives();
  return tauri.invoke<Drive[]>("list_drives");
}

export async function scanImages(dir: string): Promise<ImageInfo[]> {
  const tauri = await getTauri();
  if (!tauri) return simulateScanImages(dir);
  return tauri.invoke<ImageInfo[]>("scan_images", { dir });
}

export async function checkDestination(
  destination: string,
  source: string | null,
): Promise<DestinationSafety> {
  const tauri = await getTauri();
  if (!tauri) return simulateCheckDestination(destination, source);
  return tauri.invoke<DestinationSafety>("check_destination", {
    destination,
    source: source ?? undefined,
  });
}

export async function listDirs(path: string): Promise<DirEntryInfo[]> {
  const tauri = await getTauri();
  if (!tauri) return simulateListDirs(path);
  return tauri.invoke<DirEntryInfo[]>("list_dirs", { path });
}

/** A handle to a running operation so callers can cancel it. */
export interface OperationHandle {
  cancel: () => Promise<void>;
  done: Promise<CloneSummary>;
}

type OpKind = "clone" | "backup" | "restore";

async function startOperation(
  kind: OpKind,
  request: CloneRequest | BackupRequest | RestoreRequest,
  onProgress: ProgressCallback,
): Promise<OperationHandle> {
  const tauri = await getTauri();

  if (!tauri) {
    const sim = new SimulatedOperation(kind, request, onProgress);
    return { cancel: () => sim.cancel(), done: sim.run() };
  }

  const unlisten = await tauri.listen("clone://progress", (e) => {
    onProgress(e.payload as CloneProgressEvent);
  });

  const cmd =
    kind === "clone" ? "start_clone" : kind === "backup" ? "start_backup" : "start_restore";

  const done = tauri
    .invoke<CloneSummary>(cmd, { request })
    .finally(() => unlisten());

  return { cancel: () => tauri.invoke<void>("cancel_operation"), done };
}

export function startClone(
  request: CloneRequest,
  onProgress: ProgressCallback,
): Promise<OperationHandle> {
  return startOperation("clone", request, onProgress);
}

export function startBackup(
  request: BackupRequest,
  onProgress: ProgressCallback,
): Promise<OperationHandle> {
  return startOperation("backup", request, onProgress);
}

export function startRestore(
  request: RestoreRequest,
  onProgress: ProgressCallback,
): Promise<OperationHandle> {
  return startOperation("restore", request, onProgress);
}
