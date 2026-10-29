"use client";
// Minimal toast stack in the beUI idiom (stacked, spring-in, auto-dismiss).

import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, Info, X } from "lucide-react";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { SPRING_PANEL } from "@/lib/ease";

export type ToastKind = "info" | "warning" | "error";
export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

interface ToastContextValue {
  push: (kind: ToastKind, message: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function useToasts() {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToasts must be used within <ToastProvider>");
  return ctx;
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const push = useCallback((kind: ToastKind, message: string) => {
    const id = Date.now() + Math.random();
    setToasts((prev) => [...prev, { id, kind, message }]);
    setTimeout(() => setToasts((prev) => prev.filter((t) => t.id !== id)), 5200);
  }, []);

  const dismiss = useCallback(
    (id: number) => setToasts((prev) => prev.filter((t) => t.id !== id)),
    [],
  );

  const value = useMemo(() => ({ push }), [push]);

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div className="pointer-events-none fixed bottom-6 right-6 z-[200] flex w-80 flex-col gap-2">
        <AnimatePresence>
          {toasts.map((t) => (
            <motion.div
              key={t.id}
              layout
              initial={{ opacity: 0, y: 16, scale: 0.96 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 8, scale: 0.96 }}
              transition={SPRING_PANEL}
              className="pointer-events-auto flex items-start gap-3 rounded-xl border border-border bg-card p-3 shadow-lg"
            >
              <span className="mt-0.5">
                {t.kind === "error" || t.kind === "warning" ? (
                  <AlertTriangle
                    className={
                      t.kind === "error" ? "h-4 w-4 text-destructive" : "h-4 w-4 text-yellow-500"
                    }
                  />
                ) : (
                  <Info className="h-4 w-4 text-muted-foreground" />
                )}
              </span>
              <p className="flex-1 text-sm text-foreground">{t.message}</p>
              <button
                type="button"
                aria-label="Dismiss"
                onClick={() => dismiss(t.id)}
                className="text-muted-foreground hover:text-foreground"
              >
                <X className="h-4 w-4" />
              </button>
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </ToastContext.Provider>
  );
}
