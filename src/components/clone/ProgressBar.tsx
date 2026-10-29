"use client";
// beUI-idiom progress bar: an animated fill plus a rolling number ticker.

import { animate, motion, useMotionValue, useTransform } from "motion/react";
import { useEffect } from "react";
import { EASE_OUT } from "@/lib/ease";
import { cn } from "@/lib/utils";

export function ProgressBar({
  fraction,
  className,
  showPercent = true,
}: {
  fraction: number;
  className?: string;
  showPercent?: boolean;
}) {
  const value = useMotionValue(0);
  const width = useTransform(value, (v) => `${Math.round(v * 100)}%`);
  const percentText = useTransform(value, (v) => `${Math.round(v * 100)}%`);

  useEffect(() => {
    const controls = animate(value, Math.max(0, Math.min(1, fraction)), {
      duration: 0.4,
      ease: EASE_OUT,
    });
    return () => controls.stop();
  }, [fraction, value]);

  return (
    <div className={cn("flex items-center gap-3", className)}>
      <div className="relative h-2.5 flex-1 overflow-hidden rounded-full bg-muted">
        <motion.div
          style={{ width }}
          className="absolute inset-y-0 left-0 rounded-full bg-primary"
        />
      </div>
      {showPercent ? (
        <motion.span className="w-11 text-right text-sm font-semibold tabular-nums text-foreground">
          {percentText}
        </motion.span>
      ) : null}
    </div>
  );
}
