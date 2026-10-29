"use client";
// Shared wizard chrome: sidebar (brand + step rail + sim notice) and a
// footer nav. Used by the clone, backup and restore wizards so they share
// one consistent frame.

import { AnimatePresence, motion } from "motion/react";
import { Check } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/motion/button";
import { EASE_OUT, SPRING_LAYOUT } from "@/lib/ease";
import { cn } from "@/lib/utils";

export interface WizardStep {
  index: number;
  title: string;
}

export function WizardChrome({
  brand,
  steps,
  current,
  native,
  pageKey,
  children,
  footer,
  onHome,
}: {
  brand: { icon: ReactNode; title: string; subtitle: string };
  steps: WizardStep[];
  current: number;
  native: boolean | null;
  pageKey: string | number;
  children: ReactNode;
  footer: ReactNode;
  onHome: () => void;
}) {
  return (
    <div className="flex h-full w-full overflow-hidden">
      <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-card/40 p-5">
        <button
          type="button"
          onClick={onHome}
          className="mb-6 flex items-center gap-2 rounded-lg p-1 text-left transition-colors hover:bg-muted/50"
        >
          <span className="grid h-9 w-9 place-items-center rounded-xl bg-primary text-primary-foreground">
            {brand.icon}
          </span>
          <div>
            <div className="text-sm font-semibold leading-tight text-foreground">{brand.title}</div>
            <div className="text-[11px] text-muted-foreground">{brand.subtitle}</div>
          </div>
        </button>

        <nav aria-label="Steps" className="flex flex-col gap-1">
          {steps.map((step) => {
            const active = step.index === current;
            const done = step.index < current;
            return (
              <div key={step.index} className="relative flex items-center gap-3 py-1">
                {active ? (
                  <motion.span
                    layoutId="wizard-step-active"
                    transition={SPRING_LAYOUT}
                    className="absolute inset-0 rounded-xl bg-muted"
                  />
                ) : null}
                <span
                  className={cn(
                    "relative z-10 grid h-7 w-7 place-items-center rounded-full border text-xs font-semibold transition-colors",
                    done
                      ? "border-primary bg-primary text-primary-foreground"
                      : active
                        ? "border-foreground text-foreground"
                        : "border-border text-muted-foreground",
                  )}
                >
                  {done ? <Check className="h-4 w-4" /> : step.index + 1}
                </span>
                <span
                  className={cn(
                    "relative z-10 text-sm transition-colors",
                    active
                      ? "font-medium text-foreground"
                      : done
                        ? "text-foreground/70"
                        : "text-muted-foreground",
                  )}
                >
                  {step.title}
                </span>
              </div>
            );
          })}
        </nav>

        <div className="mt-auto pt-6">
          <Button variant="ghost" size="sm" onClick={onHome}>
            ← Back to home
          </Button>
          {native === false ? (
            <div className="mt-4 rounded-lg border border-border bg-muted/40 p-3 text-[11px] leading-relaxed text-muted-foreground">
              <span className="font-medium text-foreground">Simulation mode.</span> The Linux
              tool-chain (partclone/sfdisk) is not present, so drives and progress are simulated.
            </div>
          ) : null}
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        <div className="flex-1 overflow-y-auto px-10 py-8">
          <AnimatePresence mode="wait">
            <motion.div
              key={pageKey}
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={{ duration: 0.25, ease: EASE_OUT }}
              className="mx-auto max-w-2xl"
            >
              {children}
            </motion.div>
          </AnimatePresence>
        </div>
        {footer ? (
          <footer className="flex items-center justify-between border-t border-border px-10 py-4">
            {footer}
          </footer>
        ) : null}
      </main>
    </div>
  );
}

export function PagedSection({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  return (
    <section>
      <h1 className="text-2xl font-semibold tracking-tight text-foreground">{title}</h1>
      {subtitle ? <p className="mt-1.5 text-sm text-muted-foreground">{subtitle}</p> : null}
      <div className="mt-6">{children}</div>
    </section>
  );
}
