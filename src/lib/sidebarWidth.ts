import { useCallback, useEffect, useState } from "react";

const KEY = "notes-workbench.sidebar-width";
export const SIDEBAR_DEFAULT = 310;
const MIN = 230;
/** Najwyżej taki ułamek okna, żeby zostało miejsce na Notatkę. */
const MAX_FRACTION = 0.6;

export const clampSidebar = (w: number) => Math.round(Math.max(MIN, Math.min(w, window.innerWidth * MAX_FRACTION)));

function load(): number {
  try {
    const n = Number(localStorage.getItem(KEY));
    return Number.isFinite(n) && n > 0 ? n : SIDEBAR_DEFAULT;
  } catch {
    return SIDEBAR_DEFAULT;
  }
}

/** Szerokość panelu z drzewem w pikselach, zapamiętywana między uruchomieniami. */
export function useSidebarWidth(): [number, (w: number, persist?: boolean) => void] {
  const [width, setState] = useState(() => clampSidebar(load()));
  // Podczas przeciągania bez zapisu; zapis po puszczeniu separatora.
  const setWidth = useCallback((w: number, persist = true) => {
    const v = clampSidebar(w);
    setState(v);
    if (persist) {
      try {
        localStorage.setItem(KEY, String(v));
      } catch {
        /* bez zapamiętania */
      }
    }
  }, []);
  // Po zmniejszeniu okna panel nie może zabrać całego miejsca (zapamiętana szerokość zostaje).
  useEffect(() => {
    const fit = () => setState(clampSidebar(load()));
    window.addEventListener("resize", fit);
    return () => window.removeEventListener("resize", fit);
  }, []);
  return [width, setWidth];
}
