import { useCallback, useState } from "react";

/** Gdzie jest edytor w trybie edycji: sam albo obok podglądu (z lewej, z prawej, u góry, na dole). */
export type SplitLayout = "single" | "left" | "right" | "top" | "bottom";

export const LAYOUTS: { id: SplitLayout; label: string }[] = [
  { id: "single", label: "Tylko edytor" },
  { id: "left", label: "Edytor z lewej, podgląd z prawej" },
  { id: "right", label: "Podgląd z lewej, edytor z prawej" },
  { id: "top", label: "Edytor u góry, podgląd na dole" },
  { id: "bottom", label: "Podgląd u góry, edytor na dole" },
];

const LAYOUT_KEY = "notes-workbench.split-layout";
const RATIO_KEY = "notes-workbench.split-ratio";
export const RATIO_MIN = 0.15;
export const RATIO_MAX = 0.85;

export const clampRatio = (v: number) => Math.min(RATIO_MAX, Math.max(RATIO_MIN, v));

function load<T>(key: string, parse: (s: string) => T | null, fallback: T): T {
  try {
    const s = localStorage.getItem(key);
    return (s === null ? null : parse(s)) ?? fallback;
  } catch {
    return fallback;
  }
}

function store(key: string, v: string) {
  try {
    localStorage.setItem(key, v);
  } catch {
    /* bez zapamiętania */
  }
}

/** Układ i proporcja (udział edytora, 0–1), wspólne dla wszystkich Notatek i zapamiętywane. */
export function useSplit(): [SplitLayout, (l: SplitLayout) => void, number, (r: number, persist?: boolean) => void] {
  const [layout, setLayoutState] = useState<SplitLayout>(() =>
    load(LAYOUT_KEY, (s) => (LAYOUTS.some((l) => l.id === s) ? (s as SplitLayout) : null), "single"),
  );
  const [ratio, setRatioState] = useState(() =>
    load(RATIO_KEY, (s) => (Number.isFinite(Number(s)) ? clampRatio(Number(s)) : null), 0.5),
  );
  const setLayout = useCallback((l: SplitLayout) => {
    setLayoutState(l);
    store(LAYOUT_KEY, l);
  }, []);
  // Podczas przeciągania bez zapisu; zapis po puszczeniu separatora.
  const setRatio = useCallback((r: number, persist = true) => {
    const v = clampRatio(r);
    setRatioState(v);
    if (persist) store(RATIO_KEY, String(v));
  }, []);
  return [layout, setLayout, ratio, setRatio];
}

const MODE_KEY = "notes-workbench.editor-mode";

/** Rodzaj edytora: Markdown (CodeMirror) albo wizualny (WYSIWYG); zapamiętywany. */
export function useEditorMode(): [boolean, (visual: boolean) => void] {
  const [visual, setVisualState] = useState<boolean>(() => load(MODE_KEY, (s) => s === "visual", false));
  const setVisual = useCallback((v: boolean) => {
    setVisualState(v);
    store(MODE_KEY, v ? "visual" : "markdown");
  }, []);
  return [visual, setVisual];
}
