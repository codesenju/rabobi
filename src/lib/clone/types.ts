/*
 * Domain model for the clone feature, reverse-engineered from Rescuezilla
 * (src/apps/rescuezilla/.../drive_query.py, parser/metadata_only_image.py,
 * partitions_to_restore.py, clone_manager.py).
 *
 * Rescuezilla clone = device-to-device copy. The source disk's live partition
 * table (from sfdisk/parted/lsblk/blkid) is read into a "MetadataOnlyImage",
 * each selected source partition is mapped to a destination partition, and the
 * data is copied with partclone --dev-to-dev (or partclone.dd for raw).
 */

/** A single partition on a physical disk. */
export interface Partition {
  /** e.g. "/dev/sda1" */
  key: string;
  /** partition number within the parent disk, e.g. 1 */
  number: number;
  /** size in bytes */
  sizeBytes: number;
  /** detected filesystem, e.g. "ntfs", "ext4", "fat32", "swap", or null */
  filesystem: string | null;
  /** volume label, if any */
  label: string | null;
  /** true if this is an extended partition (skipped during copy; EBR-handled) */
  isExtended: boolean;
  /** true if part of a Linux md RAID member (clone rejects these as source) */
  isRaidMember: boolean;
  /** bytes actually used by the filesystem (partclone copies only used space) */
  usedBytes: number | null;
}

/** A physical disk / drive. Mirrors drive_query.py's combined drive state. */
export interface Drive {
  /** device node, e.g. "/dev/sda" */
  key: string;
  /** vendor/model string, e.g. "Samsung SSD 970 EVO" */
  model: string;
  /** total capacity in bytes */
  sizeBytes: number;
  /** "gpt" | "msdos" (MBR) | null when there is no partition table */
  partitionTable: "gpt" | "msdos" | null;
  /** removable media flag (USB sticks etc.) */
  removable: boolean;
  partitions: Partition[];
  /** true if any partition is currently mounted */
  isMounted?: boolean;
  /** true if this disk hosts the running OS (/, /boot, swap, …) */
  isSystem?: boolean;
}

/** Marker used by Rescuezilla for a partition that is not being restored. */
export const NOT_RESTORING_PARTITION_KEY = "DISABLED";

/**
 * One row of the partition-mapping table (partitions_to_restore.py).
 * Maps a source partition to a destination partition on the target disk.
 */
export interface PartitionMappingRow {
  /** source partition key, e.g. "/dev/sda1" */
  sourceKey: string;
  /** whether this partition will be cloned */
  enabled: boolean;
  /** human description of the source partition */
  sourceDescription: string;
  /** destination partition key or NOT_RESTORING_PARTITION_KEY */
  destKey: string;
  /** human description of the destination partition */
  destDescription: string;
}

/** Post-operation action, mirrors clone_step6_perform_action_combobox. */
export type PostCloneAction = "nothing" | "shutdown" | "reboot";

/** Everything the engine needs to run a clone. */
export interface CloneRequest {
  sourceDriveKey: string;
  destinationDriveKey: string;
  /** when true, the source partition table is written to the destination */
  overwritePartitionTable: boolean;
  /** partclone "rescue" mode: continue past read errors */
  rescue: boolean;
  postAction: PostCloneAction;
  mapping: PartitionMappingRow[];
  /** device the user typed to confirm overwriting a non-blank destination */
  confirmDevice?: string;
  /** verify each copied partition afterwards by hashing source vs dest (5.4) */
  verify?: boolean;
}

/** Progress for a single partition being copied. */
export interface PartitionProgress {
  sourceKey: string;
  destKey: string;
  /** 0..1 for this partition */
  fraction: number;
  /** partclone status line, e.g. "Elapsed: 00:12, Remaining: 00:48, 23.4%" */
  statusLine: string;
}

export type CloneStatus =
  | "idle"
  | "preparing"
  | "copying"
  | "finalizing"
  | "completed"
  | "failed"
  | "cancelled";

/** Overall progress event emitted from the backend during a clone. */
export interface CloneProgressEvent {
  status: CloneStatus;
  /** byte-weighted overall progress across all selected partitions, 0..1 */
  overallFraction: number;
  /** currently-copying partition, if any */
  current: PartitionProgress | null;
  /** short human message for the status bar */
  message: string;
  /** appended to the on-screen log */
  logLine?: string;
}

/** Final summary, mirrors clone_step7_summary. */
export interface CloneSummary {
  status: Extract<CloneStatus, "completed" | "failed" | "cancelled">;
  title: string;
  /** per-partition result lines */
  lines: string[];
  durationSeconds: number;
  /** non-fatal problems (recoverable read errors, skipped steps) — 5.7 */
  warnings?: string[];
  /** path to the per-operation log file on the host — 5.5 */
  logPath?: string;
}

/** Wizard pages, mirroring Rescuezilla's 7-page clone_tabs notebook. */
export enum ClonePage {
  Introduction = 0,
  SourceDrive = 1,
  DestinationDrive = 2,
  PartitionsToClone = 3,
  ConfirmConfiguration = 4,
  Progress = 5,
  Summary = 6,
}

export const CLONE_PAGE_TITLES: Record<ClonePage, string> = {
  [ClonePage.Introduction]: "Welcome",
  [ClonePage.SourceDrive]: "Source drive",
  [ClonePage.DestinationDrive]: "Destination drive",
  [ClonePage.PartitionsToClone]: "Partitions",
  [ClonePage.ConfirmConfiguration]: "Confirm",
  [ClonePage.Progress]: "Cloning",
  [ClonePage.Summary]: "Summary",
};
