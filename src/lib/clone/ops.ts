/*
 * Shared operation types for backup and restore, mirroring the Rust model
 * (src-tauri/src/model.rs). Clone types live in ./types.ts; these extend the
 * same progress/summary vocabulary to the backup and restore engines.
 */

import type { PartitionMappingRow } from "./types";

export type CompressionFormat = "zstd" | "gzip" | "uncompressed";

export interface BackupRequest {
  sourceDriveKey: string;
  /** short partition nodes; empty = all non-swap/non-extended */
  partitions: string[];
  destDir: string;
  compression: CompressionFormat;
  compressionLevel?: number;
  rescue: boolean;
  notes: string;
  /** hash the source while imaging and record the digest for self-verify (5.4) */
  verify?: boolean;
}

export interface RestoreRequest {
  imageDir: string;
  destinationDriveKey: string;
  overwritePartitionTable: boolean;
  rescue: boolean;
  mapping: PartitionMappingRow[];
  /** device the user typed to confirm overwriting a non-blank destination */
  confirmDevice?: string;
  /** verify each restored partition afterwards (5.4) */
  verify?: boolean;
}

/** Result of evaluating a destination's safety (Milestone 5.3). */
export interface DestinationSafety {
  device: string;
  /** cannot be overridden (same as source, system disk, unknown device) */
  hardBlocks: string[];
  /** overridable with type-to-confirm (mounted, has existing data) */
  warnings: string[];
  /** safe to proceed with no confirmation */
  safe: boolean;
  /** may proceed only with type-to-confirm */
  needsConfirmation: boolean;
}

export interface ImagePartitionInfo {
  key: string;
  filesystem: string | null;
  compression: string;
  sizeBytes: number;
}

export interface ImageInfo {
  path: string;
  name: string;
  disk: string;
  lastModified: string;
  sizeBytes: number;
  notes: string;
  partitions: ImagePartitionInfo[];
}

export interface DirEntryInfo {
  name: string;
  path: string;
  isDir: boolean;
}
