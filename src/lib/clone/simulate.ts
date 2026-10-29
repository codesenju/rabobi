/*
 * Browser-side simulation of the clone engine. Used when the Linux tools are
 * unavailable (macOS, or plain `vite dev`). It reproduces Rescuezilla's
 * byte-weighted progress model: each partition contributes to the overall bar
 * in proportion to its used size, and partclone-style status lines are emitted.
 */

import type {
  CloneProgressEvent,
  CloneRequest,
  CloneSummary,
  Drive,
} from "./types";
import { NOT_RESTORING_PARTITION_KEY } from "./types";
import type {
  BackupRequest,
  DestinationSafety,
  DirEntryInfo,
  ImageInfo,
  RestoreRequest,
} from "./ops";

const SIM_DRIVES: Drive[] = [
  {
    key: "/dev/sda",
    model: "Samsung SSD 970 EVO 500GB",
    sizeBytes: 500_107_862_016,
    partitionTable: "gpt",
    removable: false,
    partitions: [
      {
        key: "/dev/sda1",
        number: 1,
        sizeBytes: 536_870_912,
        filesystem: "fat32",
        label: "EFI",
        isExtended: false,
        isRaidMember: false,
        usedBytes: 120_000_000,
      },
      {
        key: "/dev/sda2",
        number: 2,
        sizeBytes: 272_730_423_296,
        filesystem: "ntfs",
        label: "Windows",
        isExtended: false,
        isRaidMember: false,
        usedBytes: 98_000_000_000,
      },
      {
        key: "/dev/sda3",
        number: 3,
        sizeBytes: 226_839_767_808,
        filesystem: "ext4",
        label: "Linux",
        isExtended: false,
        isRaidMember: false,
        usedBytes: 41_000_000_000,
      },
    ],
  },
  {
    key: "/dev/sdb",
    model: "WD Blue 1TB HDD",
    sizeBytes: 1_000_204_886_016,
    partitionTable: "gpt",
    removable: false,
    partitions: [
      {
        key: "/dev/sdb1",
        number: 1,
        sizeBytes: 1_000_204_886_016 - 1_048_576,
        filesystem: "ext4",
        label: "Backup",
        isExtended: false,
        isRaidMember: false,
        usedBytes: 12_000_000_000,
      },
    ],
  },
  {
    key: "/dev/sdc",
    model: "SanDisk Ultra USB 3.0",
    sizeBytes: 61_530_439_680,
    partitionTable: "msdos",
    removable: true,
    partitions: [
      {
        key: "/dev/sdc1",
        number: 1,
        sizeBytes: 61_530_439_680 - 1_048_576,
        filesystem: "fat32",
        label: "USB",
        isExtended: false,
        isRaidMember: false,
        usedBytes: 2_400_000_000,
      },
    ],
  },
];

export function simulateListDrives(): Drive[] {
  // Deep clone so callers can't mutate the fixtures.
  return JSON.parse(JSON.stringify(SIM_DRIVES));
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

function clock(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${pad(m)}:${pad(s)}`;
}

export class SimulatedOperation {
  private cancelled = false;
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private readonly kind: "clone" | "backup" | "restore",
    private readonly request: CloneRequest | BackupRequest | RestoreRequest,
    private readonly onProgress: (e: CloneProgressEvent) => void,
  ) {}

  cancel(): Promise<void> {
    this.cancelled = true;
    if (this.timer) clearTimeout(this.timer);
    return Promise.resolve();
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => {
      this.timer = setTimeout(resolve, ms);
    });
  }

  /** Resolve the per-partition work items for the chosen operation. */
  private items(): { source: string; dest: string; weight: number }[] {
    if (this.kind === "clone") {
      const req = this.request as CloneRequest;
      const drives = simulateListDrives();
      const src = drives.find((d) => d.key === req.sourceDriveKey);
      return req.mapping
        .filter((m) => m.enabled && m.destKey !== NOT_RESTORING_PARTITION_KEY)
        .map((m) => ({
          source: m.sourceKey,
          dest: m.destKey,
          weight: Math.max(
            1,
            src?.partitions.find((p) => p.key === m.sourceKey)?.usedBytes ??
              src?.partitions.find((p) => p.key === m.sourceKey)?.sizeBytes ??
              1,
          ),
        }));
    }
    if (this.kind === "backup") {
      const req = this.request as BackupRequest;
      const drives = simulateListDrives();
      const src = drives.find((d) => d.key === req.sourceDriveKey);
      const parts = (src?.partitions ?? []).filter(
        (p) => !p.isExtended && p.filesystem !== "swap",
      );
      const chosen = parts.filter(
        (p) =>
          req.partitions.length === 0 ||
          req.partitions.includes(p.key) ||
          req.partitions.includes(p.key.replace(/^\/dev\//, "")),
      );
      return chosen.map((p) => ({
        source: p.key,
        dest: `${p.key.replace(/^\/dev\//, "")}.${p.filesystem ?? "dd"}-ptcl-img`,
        weight: Math.max(1, p.usedBytes ?? p.sizeBytes),
      }));
    }
    // restore
    const req = this.request as RestoreRequest;
    const img = simulateScanImages("").find((i) => i.path === req.imageDir) ??
      simulateScanImages("")[0];
    return (img?.partitions ?? []).map((p, idx) => ({
      source: p.key,
      dest:
        req.mapping.find((m) => m.sourceKey.endsWith(p.key) && m.enabled)?.destKey ??
        `${req.destinationDriveKey}${idx + 1}`,
      weight: Math.max(1, p.sizeBytes),
    }));
  }

  private verbs() {
    switch (this.kind) {
      case "backup":
        return { ing: "Imaging", done: "imaged", noun: "Backup" };
      case "restore":
        return { ing: "Restoring", done: "restored", noun: "Restore" };
      default:
        return { ing: "Cloning", done: "cloned", noun: "Clone" };
    }
  }

  async run(): Promise<CloneSummary> {
    const started = Date.now();
    const items = this.items();
    const v = this.verbs();
    if (items.length === 0) {
      return this.summary("failed", started, ["Nothing selected to process."]);
    }
    const totalWeight = items.reduce((a, b) => a + b.weight, 0) || 1;

    const prep =
      this.kind === "backup"
        ? "Writing image metadata…"
        : this.kind === "restore"
          ? "Writing destination partition table…"
          : "Preparing destination…";
    this.onProgress({
      status: "preparing",
      overallFraction: 0,
      current: null,
      message: prep,
      logLine:
        this.kind === "backup"
          ? `mkdir ${(this.request as BackupRequest).destDir}`
          : this.kind === "restore"
            ? `cat *-pt.sf | sfdisk ${(this.request as RestoreRequest).destinationDriveKey}`
            : `sfdisk --dump → sfdisk`,
    });
    await this.sleep(600);
    if (this.cancelled) return this.summary("cancelled", started, []);

    const lines: string[] = [];
    let completedWeight = 0;

    for (const item of items) {
      const steps = 20;
      for (let s = 1; s <= steps; s++) {
        if (this.cancelled) return this.summary("cancelled", started, lines);
        const frac = s / steps;
        const elapsed = Math.round((Date.now() - started) / 1000);
        const remaining = Math.round((elapsed / frac) * (1 - frac));
        const overall = (completedWeight + item.weight * frac) / totalWeight;
        this.onProgress({
          status: "copying",
          overallFraction: overall,
          current: {
            sourceKey: item.source,
            destKey: item.dest,
            fraction: frac,
            statusLine: `Elapsed: ${clock(elapsed)}, Remaining: ${clock(remaining)}, Completed: ${(frac * 100).toFixed(1)}%`,
          },
          message: `${v.ing} ${item.source} → ${item.dest}`,
          logLine: s === 1 ? this.cmdFor(item) : undefined,
        });
        await this.sleep(110);
      }
      completedWeight += item.weight;
      lines.push(`${item.source} → ${item.dest}: ${v.done}`);
    }

    this.onProgress({
      status: "finalizing",
      overallFraction: 1,
      current: null,
      message: "Flushing buffers…",
      logLine: "sync",
    });
    await this.sleep(400);
    return this.summary("completed", started, lines);
  }

  private cmdFor(item: { source: string; dest: string }): string {
    switch (this.kind) {
      case "backup":
        return `partclone --clone --source ${item.source} --output - | ${(this.request as BackupRequest).compression} | split -b 4GB - ${item.dest}`;
      case "restore":
        return `cat ${item.source}.* | decompress | partclone.restore --overwrite ${item.dest}`;
      default:
        return `partclone --dev-to-dev --source ${item.source} --overwrite ${item.dest}`;
    }
  }

  private summary(
    status: CloneSummary["status"],
    started: number,
    lines: string[],
  ): CloneSummary {
    const duration = Math.round((Date.now() - started) / 1000);
    const v = this.verbs();
    const title =
      status === "completed"
        ? `${v.noun} completed successfully`
        : status === "cancelled"
          ? `${v.noun} cancelled`
          : `${v.noun} failed`;
    const finalLines =
      status === "cancelled" ? [...lines, "Operation cancelled by user."] : lines;

    // Simulated verification + log path so the UI surfaces them in demo mode.
    const warnings: string[] = [];
    const verify = (this.request as { verify?: boolean }).verify;
    if (status === "completed" && verify) {
      finalLines.push("verification: OK (hashes match) [simulated]");
    }
    const logPath = `/tmp/rabobi/rabobi-${this.kind}-simulated.log`;

    this.onProgress({
      status,
      overallFraction: status === "completed" ? 1 : 0,
      current: null,
      message: title,
    });
    return {
      status,
      title,
      lines: finalLines,
      durationSeconds: duration,
      warnings,
      logPath,
    };
  }
}

/** Simulated image library for the restore picker. */
export function simulateScanImages(_dir: string): ImageInfo[] {
  void _dir;
  return [
    {
      path: "/mnt/backup/2026-03-11-img-rabobi",
      name: "2026-03-11-img-rabobi",
      disk: "sda",
      lastModified: "2026-03-11 14:22:05 UTC",
      sizeBytes: 139_000_000_000,
      notes: "Pre-upgrade full backup of the Samsung SSD",
      partitions: [
        { key: "sda1", filesystem: "fat32", compression: "zstd", sizeBytes: 90_000_000 },
        { key: "sda2", filesystem: "ntfs", compression: "zstd", sizeBytes: 96_000_000_000 },
        { key: "sda3", filesystem: "ext4", compression: "zstd", sizeBytes: 42_900_000_000 },
      ],
    },
    {
      path: "/mnt/backup/2026-01-02-img-laptop",
      name: "2026-01-02-img-laptop",
      disk: "nvme0n1",
      lastModified: "2026-01-02 09:10:44 UTC",
      sizeBytes: 54_000_000_000,
      notes: "",
      partitions: [
        { key: "nvme0n1p1", filesystem: "fat32", compression: "gzip", sizeBytes: 100_000_000 },
        { key: "nvme0n1p2", filesystem: "ext4", compression: "gzip", sizeBytes: 53_900_000_000 },
      ],
    },
  ];
}

/** Simulated directory browser for the destination folder picker. */
export function simulateListDirs(path: string): DirEntryInfo[] {
  const base = path && path !== "/" ? path.replace(/\/$/, "") : "";
  return [
    { name: "mnt", path: `${base}/mnt`, isDir: true },
    { name: "media", path: `${base}/media`, isDir: true },
    { name: "backup", path: `${base}/backup`, isDir: true },
  ];
}

/** Simulated destination-safety evaluation, mirroring Rust `safety.rs`. */
export function simulateCheckDestination(
  destination: string,
  source: string | null,
): DestinationSafety {
  const drive = simulateListDrives().find((d) => d.key === destination);
  const hardBlocks: string[] = [];
  const warnings: string[] = [];

  if (!drive) {
    hardBlocks.push(`Destination ${destination} is not a recognised disk.`);
  } else {
    if (source && source === destination) {
      hardBlocks.push("Destination is the same as the source.");
    }
    if (drive.isSystem) {
      hardBlocks.push("Destination hosts the running operating system (/, /boot, or swap).");
    }
    if (drive.isMounted && !drive.isSystem) {
      warnings.push("Destination has mounted filesystems — they will be unmounted and erased.");
    }
    if (drive.partitions.length > 0) {
      warnings.push(
        `Destination already contains ${drive.partitions.length} partition(s); all data will be erased.`,
      );
    }
  }

  const safe = hardBlocks.length === 0 && warnings.length === 0;
  const needsConfirmation = hardBlocks.length === 0 && warnings.length > 0;
  return { device: destination, hardBlocks, warnings, safe, needsConfirmation };
}
