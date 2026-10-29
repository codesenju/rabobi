"use client";
// Source: beui.dev/components/motion/select — copied per beUI's MIT licence.

import { Check, ChevronDown } from "lucide-react";
import {
  animate,
  type MotionValue,
  motion,
  useMotionValue,
  useMotionValueEvent,
  useReducedMotion,
  useTransform,
  type Variants,
} from "motion/react";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { EASE_OUT } from "@/lib/ease";
import { type ListboxRow, useListbox } from "@/lib/hooks/use-listbox";
import { cn } from "@/lib/utils";

const UNFOLD_SPRING = { type: "spring", visualDuration: 0.2, bounce: 0.12 } as const;
const UNFOLD_EXIT = { duration: 0.2, ease: EASE_OUT } as const;
const RADIUS = 12;
const GAP = 8;
const LIP = 8;
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

const ITEM_VARIANTS: Variants = {
  hidden: { opacity: 0, y: -4, transition: { duration: 0.08, ease: EASE_OUT } },
  show: { opacity: 1, y: 0, transition: { duration: 0.12, ease: EASE_OUT } },
};

type Placement = "bottom" | "top";

interface SelectContextValue {
  value: string | undefined;
  open: boolean;
  setOpen: (open: boolean) => void;
  select: (value: string) => void;
  register: (value: string, label: string, disabled: boolean) => void;
  unregister: (value: string) => void;
  labelFor: (value: string | undefined) => string | undefined;
  rows: ListboxRow[];
  close: () => void;
  reduce: boolean;
  progress: MotionValue<number>;
  triggerId: string;
  listId: string;
  disabled: boolean;
  placement: Placement;
  setPlacement: (p: Placement) => void;
}

const SelectContext = createContext<SelectContextValue | null>(null);
const ActiveOptionContext = createContext<string | undefined>(undefined);

function useSelectContext(component: string) {
  const ctx = useContext(SelectContext);
  if (!ctx) throw new Error(`${component} must be used within <Select>`);
  return ctx;
}

export interface SelectProps {
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  disabled?: boolean;
  className?: string;
  children: ReactNode;
}

export function Select({
  value,
  defaultValue,
  onValueChange,
  open: openProp,
  defaultOpen = false,
  onOpenChange,
  disabled = false,
  className,
  children,
}: SelectProps) {
  const reduce = useReducedMotion() ?? false;
  const baseId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const [internalOpen, setInternalOpen] = useState(defaultOpen);
  const [internal, setInternal] = useState(defaultValue);
  const [labels, setLabels] = useState<Map<string, { label: string; disabled: boolean }>>(new Map());
  const [placement, setPlacement] = useState<Placement>("bottom");

  const controlled = value !== undefined;
  const current = controlled ? value : internal;
  const openControlled = openProp !== undefined;
  const open = openControlled ? openProp : internalOpen;
  const progress = useMotionValue(open ? 1 : 0);

  const setOpen = useCallback(
    (next: boolean) => {
      if (!openControlled) setInternalOpen(next);
      onOpenChange?.(next);
    },
    [onOpenChange, openControlled],
  );

  const close = useCallback(() => {
    const trigger = document.getElementById(`${baseId}-trigger`);
    if (rootRef.current?.contains(document.activeElement) && document.activeElement !== trigger)
      trigger?.focus();
    setOpen(false);
  }, [baseId, setOpen]);

  const select = useCallback(
    (next: string) => {
      if (!controlled) setInternal(next);
      onValueChange?.(next);
      close();
    },
    [controlled, onValueChange, close],
  );

  const register = useCallback((v: string, label: string, isDisabled: boolean) => {
    setLabels((m) => {
      const entry = m.get(v);
      if (entry?.label === label && entry.disabled === isDisabled) return m;
      return new Map(m).set(v, { label, disabled: isDisabled });
    });
  }, []);
  const unregister = useCallback((v: string) => {
    setLabels((m) => {
      if (!m.has(v)) return m;
      const next = new Map(m);
      next.delete(v);
      return next;
    });
  }, []);

  useEffect(() => {
    const animation = animate(
      progress,
      open ? 1 : 0,
      reduce ? { duration: 0.12 } : open ? UNFOLD_SPRING : UNFOLD_EXIT,
    );
    return () => animation.stop();
  }, [open, progress, reduce]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    const onPointer = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onPointer);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onPointer);
    };
  }, [open, setOpen, close]);

  const rows = useMemo(
    () => Array.from(labels, ([id, entry]) => ({ id, label: entry.label, disabled: entry.disabled })),
    [labels],
  );

  const ctx = useMemo<SelectContextValue>(
    () => ({
      value: current,
      open,
      setOpen,
      select,
      register,
      unregister,
      labelFor: (v) => (v === undefined ? undefined : labels.get(v)?.label),
      rows,
      close,
      reduce,
      progress,
      triggerId: `${baseId}-trigger`,
      listId: `${baseId}-list`,
      disabled,
      placement,
      setPlacement,
    }),
    [current, open, setOpen, select, register, unregister, labels, rows, close, reduce, progress, baseId, disabled, placement],
  );

  return (
    <SelectContext.Provider value={ctx}>
      <div ref={rootRef} className={cn("relative", className)}>
        {children}
      </div>
    </SelectContext.Provider>
  );
}

export function SelectTrigger({ className, children }: { className?: string; children: ReactNode }) {
  const ctx = useSelectContext("SelectTrigger");
  const isTop = ctx.placement === "top";
  const nearRadius = useTransform(ctx.progress, [0, 0.2, 0.4, 1], [RADIUS, 0, 0, RADIUS]);
  const rotate = useTransform(ctx.progress, (p) =>
    ctx.reduce ? (ctx.open ? 180 : 0) : Math.min(1, Math.max(0, p)) * 180,
  );
  return (
    <motion.button
      type="button"
      id={ctx.triggerId}
      disabled={ctx.disabled}
      aria-haspopup="listbox"
      aria-expanded={ctx.open}
      aria-controls={ctx.listId}
      onClick={() => ctx.setOpen(!ctx.open)}
      onKeyDown={(event) => {
        if (!ctx.open && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
          event.preventDefault();
          ctx.setOpen(true);
        }
      }}
      style={{
        borderTopLeftRadius: ctx.reduce || !isTop ? RADIUS : nearRadius,
        borderTopRightRadius: ctx.reduce || !isTop ? RADIUS : nearRadius,
        borderBottomLeftRadius: ctx.reduce || isTop ? RADIUS : nearRadius,
        borderBottomRightRadius: ctx.reduce || isTop ? RADIUS : nearRadius,
      }}
      className={cn(
        "relative z-10 flex w-full items-center justify-between gap-2 border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-colors",
        "hover:border-muted-foreground focus-visible:ring-2 focus-visible:ring-foreground/20",
        "disabled:pointer-events-none disabled:opacity-50",
        className,
      )}
    >
      {children}
      <motion.span aria-hidden style={{ rotate }} className="text-muted-foreground">
        <ChevronDown className="h-4 w-4" />
      </motion.span>
    </motion.button>
  );
}

export function SelectValue({ placeholder, className }: { placeholder?: string; className?: string }) {
  const ctx = useSelectContext("SelectValue");
  const label = ctx.labelFor(ctx.value);
  return (
    <span className={cn(label ? "text-foreground" : "text-muted-foreground", className)}>
      {label ?? placeholder ?? "Select"}
    </span>
  );
}

export function SelectContent({ className, children }: { className?: string; children: ReactNode }) {
  const ctx = useSelectContext("SelectContent");
  const panelRef = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(0);
  const { open, progress, reduce, setPlacement } = ctx;
  const { activeId, onKeyDown } = useListbox({ open, rows: ctx.rows, selected: ctx.value, list: panelRef });

  const [exiting, setExiting] = useState(false);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (!open) setExiting(true);
  }
  const present = open || exiting;
  useMotionValueEvent(progress, "animationComplete", () => {
    if (progress.get() === 0) setExiting(false);
  });

  useLayoutEffect(() => {
    const node = panelRef.current;
    if (!node || !present) return;
    const measure = () => setHeight(node.offsetHeight);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [present]);

  useLayoutEffect(() => {
    if (!open) return;
    const trigger = document.getElementById(ctx.triggerId);
    const node = panelRef.current;
    if (!trigger || !node) return;
    const rect = trigger.getBoundingClientRect();
    const h = node.offsetHeight;
    const below = window.innerHeight - rect.bottom;
    const above = rect.top;
    setPlacement(below < h + 16 && above > below ? "top" : "bottom");
  }, [open, ctx.triggerId, setPlacement]);

  const isTop = ctx.placement === "top";
  const ready = height > 0 && !reduce;
  const fromY = ready ? (isTop ? GAP : -GAP) : 0;
  const detach = useTransform(progress, [0.4, 1], [0, 1]);
  const y = useTransform(detach, (d) => lerp(fromY, 0, d));
  const nearRadius = useTransform(detach, (d) => (ready ? RADIUS * d : RADIUS));
  const borderRadius = useTransform(nearRadius, (near) =>
    isTop ? `${RADIUS}px ${RADIUS}px ${near}px ${near}px` : `${near}px ${near}px ${RADIUS}px ${RADIUS}px`,
  );
  const clipPath = useTransform([progress, nearRadius], ([p, near]: number[]) => {
    if (!ready) return "none";
    const reveal = Math.min(1, Math.max(0, p));
    if (reveal === 1) return "none";
    const hidden = Math.max(0, (height - LIP) * (1 - reveal));
    const inset = isTop ? `${hidden}px 0 0 0` : `0 0 ${hidden}px 0`;
    const corners = isTop
      ? `${RADIUS}px ${RADIUS}px ${near}px ${near}px`
      : `${near}px ${near}px ${RADIUS}px ${RADIUS}px`;
    return `inset(${inset} round ${corners})`;
  });
  const opacity = useTransform(progress, reduce ? [0, 1] : [0, 0.12], [0, 1]);

  return (
    <motion.div
      ref={panelRef}
      id={ctx.listId}
      role="listbox"
      aria-labelledby={ctx.triggerId}
      aria-hidden={!open}
      // @ts-expect-error inert attribute
      inert={!open ? "" : undefined}
      onKeyDown={(event) => {
        if (event.key === "Tab") ctx.close();
        else onKeyDown(event);
      }}
      style={{
        visibility: present ? undefined : "hidden",
        maxHeight: present ? undefined : 0,
        borderWidth: present ? undefined : 0,
        y,
        clipPath,
        borderRadius,
        opacity,
        pointerEvents: open ? "auto" : "none",
        marginTop: isTop ? 0 : GAP,
        marginBottom: isTop ? GAP : 0,
      }}
      className={cn(
        "absolute left-0 right-0 z-20 overflow-hidden border border-border bg-background shadow-lg",
        isTop ? "bottom-full" : "top-full",
        className,
      )}
    >
      <ActiveOptionContext.Provider value={activeId}>
        <motion.div initial={false} animate={open ? "show" : "hidden"} className="p-1">
          {children}
        </motion.div>
      </ActiveOptionContext.Provider>
    </motion.div>
  );
}

export function SelectItem({
  value,
  disabled = false,
  className,
  children,
}: {
  value: string;
  disabled?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const ctx = useSelectContext("SelectItem");
  const activeId = useContext(ActiveOptionContext);
  const selected = ctx.value === value;
  const label = typeof children === "string" ? children : value;

  useLayoutEffect(() => {
    ctx.register(value, label, disabled);
  }, [ctx.register, value, label, disabled]);
  useLayoutEffect(() => () => ctx.unregister(value), [ctx.unregister, value]);

  return (
    <motion.div variants={ctx.reduce ? undefined : ITEM_VARIANTS}>
      <button
        type="button"
        role="option"
        aria-selected={selected}
        disabled={disabled}
        tabIndex={activeId === value ? 0 : -1}
        data-listbox-id={value}
        onClick={() => ctx.select(value)}
        className={cn(
          "flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-1.5 text-left text-sm outline-none transition-colors",
          selected
            ? "bg-muted text-foreground"
            : "text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:bg-muted",
          "disabled:pointer-events-none disabled:opacity-50",
          className,
        )}
      >
        {children}
        {selected ? <Check className="h-3.5 w-3.5 shrink-0" /> : null}
      </button>
    </motion.div>
  );
}
