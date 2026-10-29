/*
 * Source -> destination partition mapping, translated from Rescuezilla's
 * partitions_to_restore.py.
 *
 * Two modes:
 *  - overwrite partition table ON  (_use_image_partition_table):
 *      every source partition /dev/sdaN maps to destination /dev/sdbN,
 *      and the mapping is locked (not user-editable).
 *  - overwrite partition table OFF (_use_existing_drive_partition_table):
 *      mappings start blank (DISABLED) so the user picks each destination
 *      partition manually on the target's existing table.
 */

import {
  type Drive,
  type Partition,
  type PartitionMappingRow,
  NOT_RESTORING_PARTITION_KEY,
} from "./types";
import { describePartition } from "./format";

/** Split "/dev/sda12" -> { node: "/dev/sda", number: 12 }. */
export function splitDeviceString(key: string): { node: string; number: number } {
  const match = key.match(/^(.*?)(\d+)$/);
  if (!match) return { node: key, number: 0 };
  return { node: match[1], number: Number(match[2]) };
}

/** Join a disk node and a partition number into a partition key. */
export function joinDeviceString(node: string, number: number): string {
  // nvme/mmc style nodes end in a digit and use a 'p' separator.
  const needsP = /\d$/.test(node);
  return `${node}${needsP ? "p" : ""}${number}`;
}

function describeDestForNumber(dest: Drive, number: number): string {
  const existing = dest.partitions.find((p) => p.number === number);
  if (existing) return describePartition(existing);
  return `${joinDeviceString(dest.key, number)}: (will be created)`;
}

/** Build mapping rows when overwriting the destination partition table. */
export function useImagePartitionTable(
  source: Drive,
  dest: Drive,
): PartitionMappingRow[] {
  return source.partitions
    .filter((p) => !p.isExtended)
    .map((p: Partition): PartitionMappingRow => {
      const { number } = splitDeviceString(p.key);
      const destKey = joinDeviceString(dest.key, number);
      return {
        sourceKey: p.key,
        enabled: true,
        sourceDescription: describePartition(p),
        destKey,
        destDescription: describeDestForNumber(dest, number),
      };
    });
}

/** Build mapping rows when keeping the destination's existing table. */
export function useExistingDrivePartitionTable(
  source: Drive,
): PartitionMappingRow[] {
  return source.partitions
    .filter((p) => !p.isExtended)
    .map(
      (p: Partition): PartitionMappingRow => ({
        sourceKey: p.key,
        enabled: false,
        sourceDescription: describePartition(p),
        destKey: NOT_RESTORING_PARTITION_KEY,
        destDescription: "— choose a destination —",
      }),
    );
}

/**
 * Restore-to-smaller-disk guard (Sfdisk.get_highest_offset_partition vs
 * destination capacity). Returns an error string, or null when it fits.
 */
export function checkFitsDestination(
  source: Drive,
  dest: Drive,
  mapping: PartitionMappingRow[],
  overwritePartitionTable: boolean,
): string | null {
  if (!overwritePartitionTable) return null;
  const enabledSource = source.partitions.filter(
    (p) => !p.isExtended && mapping.some((m) => m.sourceKey === p.key && m.enabled),
  );
  const required = enabledSource.reduce((sum, p) => sum + p.sizeBytes, 0);
  if (required > dest.sizeBytes) {
    return (
      "The selected partitions require more space than the destination disk " +
      "provides. Rescuezilla-style clone cannot yet shrink partitions to fit a " +
      "smaller disk."
    );
  }
  return null;
}

/** Validate a source drive selection (mirrors handler.py CLONE_SOURCE checks). */
export function validateSource(drive: Drive): string | null {
  if (drive.partitionTable === null && drive.partitions.length === 0) {
    return "The selected source drive has no partition table and no partitions.";
  }
  if (drive.partitions.some((p) => p.isRaidMember)) {
    return "The selected source drive contains a Linux RAID member, which clone cannot read directly.";
  }
  return null;
}

/** Validate destination selection (must differ from source). */
export function validateDestination(
  sourceKey: string,
  destKey: string,
): string | null {
  if (sourceKey === destKey) {
    return "The destination drive must be different from the source drive.";
  }
  return null;
}
