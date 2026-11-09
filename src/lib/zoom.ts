import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

/** Skala czcionki treści Notatki (widok i edytor), wspólna dla wszystkich Notatek i zapamiętywana. */
const KEY = "notes-workbench.note-scale";
export const SCALE_MIN = 0.7;
export const SCALE_MAX = 2;
const STEP = 0.1;

function read(): number {
  try {
    const v = Number(localStorage.getItem(KEY));
    if (v >= SCALE_MIN && v <= SCALE_MAX) return v;
  } catch {
    /* brak pamięci przeglądarki: 100% */
  }
  return 1;
}

const clamp = (v: number) => Math.round(Math.min(SCALE_MAX, Math.max(SCALE_MIN, v)) * 100) / 100;

/**
 * Ctrl + rolka myszy (oraz Ctrl + / Ctrl − / Ctrl 0) na wskazanym elemencie zmienia skalę.
 * Zwraca skalę i informację, czy pokazać chwilowy wskaźnik procentów.
 */
export function useNoteScale(target: RefObject<HTMLElement | null>): [number, boolean, (v: number) => void] {
  const [scale, setScaleState] = useState(read);
  const [flash, setFlash] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  const setScale = useCallback((v: number) => {
    const s = clamp(v);
    setScaleState(s);
    try {
      localStorage.setItem(KEY, String(s));
    } catch {
      /* pomijamy */
    }
    setFlash(true);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setFlash(false), 1200);
  }, []);

  useEffect(() => {
    // Nasłuch na całym oknie: element Notatki pojawia się dopiero po wczytaniu treści.
    const onWheel = (e: WheelEvent) => {
      const el = target.current;
      if (!e.ctrlKey || !el || !(e.target instanceof Node) || !el.contains(e.target)) return;
      // Bez tego WebView powiększyłby całe okno aplikacji.
      e.preventDefault();
      setScaleState((cur) => {
        const next = clamp(cur + (e.deltaY < 0 ? STEP : -STEP));
        if (next !== cur) {
          try {
            localStorage.setItem(KEY, String(next));
          } catch {
            /* pomijamy */
          }
        }
        return next;
      });
      setFlash(true);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setFlash(false), 1200);
    };
    window.addEventListener("wheel", onWheel, { passive: false });
    return () => window.removeEventListener("wheel", onWheel);
  }, [target]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.shiftKey || e.altKey) return;
      if (e.key === "=" || e.key === "+") setScale(scale + STEP);
      else if (e.key === "-") setScale(scale - STEP);
      else if (e.key === "0") setScale(1);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [scale, setScale]);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  return [scale, flash, setScale];
}
