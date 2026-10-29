"use client";
// From beUI (beui.dev). Bridges Motion's usePresence with a render-prop so a
// component can hold its exit animation until Motion signals it is safe to
// unmount, and expose `inert` while leaving.

import { usePresence } from "motion/react";
import type { ReactNode } from "react";

export interface PresenceGateRenderProps {
  isPresent: boolean;
  gate: {
    onAnimationComplete?: () => void;
  };
}

export interface PresenceGateProps {
  children: (props: PresenceGateRenderProps) => ReactNode;
}

export function PresenceGate({ children }: PresenceGateProps) {
  const [isPresent, safeToRemove] = usePresence();
  const gate = {
    onAnimationComplete: () => {
      if (!isPresent) safeToRemove?.();
    },
  };
  return <>{children({ isPresent, gate })}</>;
}
