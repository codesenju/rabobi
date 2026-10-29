"use client";
// From beUI (beui.dev). Runs `start` during the render in which `open` becomes true.

import { useState } from "react";

export function useOnOpen(open: boolean, start: () => void) {
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) start();
  }
}
