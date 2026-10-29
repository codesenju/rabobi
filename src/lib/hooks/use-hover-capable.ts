"use client";

import { useEffect, useState } from "react";

/**
 * From beUI (beui.dev). Returns true only when the primary pointer can hover,
 * so touch devices skip hover-only affordances.
 */
export function useHoverCapable() {
  const [canHover, setCanHover] = useState(true);

  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const query = window.matchMedia("(hover: hover) and (pointer: fine)");
    const update = () => setCanHover(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);

  return canHover;
}
