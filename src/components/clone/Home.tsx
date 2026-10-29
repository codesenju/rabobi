"use client";

import { useEffect, useState } from "react";
import { motion } from "motion/react";
import {
  Copy,
  HardDriveDownload,
  HardDriveUpload,
  AlertTriangle,
  CheckCircle2,
  XCircle,
} from "lucide-react";
import { SPRING_PRESS } from "@/lib/ease";
import { cn } from "@/lib/utils";
import { preflight, type Preflight } from "@/lib/clone/backend";

export type AppMode = "home" | "clone" | "backup" | "restore";

const CARDS: {
  mode: Exclude<AppMode, "home">;
  title: string;
  desc: string;
  icon: typeof Copy;
}[] = [
  {
    mode: "clone",
    title: "Clone a disk",
    desc: "Point one disk at another and make them identical — drive to drive, no image file.",
    icon: Copy,
  },
  {
    mode: "backup",
    title: "Back up a disk",
    desc: "Capture a disk into a compressed image folder you can keep and point back later.",
    icon: HardDriveDownload,
  },
  {
    mode: "restore",
    title: "Restore an image",
    desc: "Point a saved image back at a disk and bring its twin back to life.",
    icon: HardDriveUpload,
  },
];

export function Home({
  native,
  onPick,
}: {
  native: boolean | null;
  onPick: (mode: Exclude<AppMode, "home">) => void;
}) {
  const [pf, setPf] = useState<Preflight | null>(null);
  useEffect(() => {
    preflight().then(setPf).catch(() => undefined);
  }, []);

  return (
    <div className="flex h-full w-full flex-col items-center justify-center px-8">
      <div className="w-full max-w-3xl">
        <div className="mb-10 text-center">
          <img
            src="/rabobi.svg"
            alt="Rabobi"
            width={96}
            height={96}
            className="mx-auto mb-4 rounded-[22px] shadow-lg"
          />
          <h1 className="text-4xl font-bold tracking-tight text-foreground">Rabobi</h1>
          <p className="mt-2 text-sm text-muted-foreground">
            Clone, back up, and restore disks — three identical drives, pointing at each other.
          </p>
          <p className="mt-1 text-xs text-muted-foreground/70">
            Named after the Xhosa-dubbed 1980s Spider-Man. You know the meme.
          </p>
        </div>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          {CARDS.map((card) => {
            const Icon = card.icon;
            return (
              <motion.button
                key={card.mode}
                type="button"
                whileTap={{ scale: 0.97 }}
                whileHover={{ y: -3 }}
                transition={SPRING_PRESS}
                onClick={() => onPick(card.mode)}
                className={cn(
                  "flex flex-col items-start gap-3 rounded-2xl border border-border bg-card/40 p-5 text-left",
                  "transition-colors hover:border-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                )}
              >
                <span className="grid h-11 w-11 place-items-center rounded-xl bg-primary text-primary-foreground">
                  <Icon className="h-6 w-6" />
                </span>
                <span className="text-base font-medium text-foreground">{card.title}</span>
                <span className="text-sm leading-relaxed text-muted-foreground">{card.desc}</span>
              </motion.button>
            );
          })}
        </div>

        {native === false ? (
          <div className="mt-8 flex items-start gap-2 rounded-xl border border-yellow-500/30 bg-yellow-500/5 p-4 text-sm">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-yellow-500" />
            <p className="text-muted-foreground">
              Running in <span className="font-medium text-foreground">simulation mode</span> — the
              Linux tool-chain isn't present, so drives and operations are simulated. On a Linux host
              with partclone/sfdisk installed and run as root, operations execute for real.
            </p>
          </div>
        ) : pf ? (
          <div
            className={cn(
              "mt-8 rounded-xl border p-4 text-sm",
              pf.ready
                ? "border-emerald-500/30 bg-emerald-500/5"
                : "border-yellow-500/30 bg-yellow-500/5",
            )}
          >
            <div className="flex items-center gap-2">
              {pf.ready ? (
                <CheckCircle2 className="h-4 w-4 text-emerald-500" />
              ) : (
                <AlertTriangle className="h-4 w-4 text-yellow-500" />
              )}
              <span className="font-medium text-foreground">
                {pf.ready ? "Ready for real disk operations" : "Not ready for real operations"}
              </span>
            </div>
            {pf.blockers.length > 0 ? (
              <ul className="mt-2 flex flex-col gap-1 text-muted-foreground">
                {pf.blockers.map((b, i) => (
                  <li key={i} className="flex items-start gap-2">
                    <span className="mt-1 h-1 w-1 shrink-0 rounded-full bg-yellow-500" />
                    {b}
                  </li>
                ))}
              </ul>
            ) : null}
            {pf.tools.length > 0 ? (
              <div className="mt-3 flex flex-wrap gap-1.5">
                {pf.tools.map((t) => (
                  <span
                    key={t.name}
                    className={cn(
                      "inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] ring-1",
                      t.present
                        ? "text-foreground ring-border"
                        : "text-destructive ring-destructive/40",
                    )}
                  >
                    {t.present ? (
                      <CheckCircle2 className="h-3 w-3 text-emerald-500" />
                    ) : (
                      <XCircle className="h-3 w-3 text-destructive" />
                    )}
                    {t.name}
                  </span>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
