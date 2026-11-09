import { useEffect, useState } from "react";

/** Motyw: zgodny z Windows (`auto`), zawsze jasny albo zawsze ciemny. */
export type ThemeMode = "auto" | "light" | "dark";
const KEY = "notes-workbench.theme";

function systemDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

function readMode(): ThemeMode {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "auto") return v;
  } catch {
    /* brak dostępu do pamięci przeglądarki: zostaje auto */
  }
  return "auto";
}

/** Tryb wybrany przez użytkownika i motyw faktycznie wyświetlany (dla grafu, który rysuje na canvasie). */
export function useTheme(): [ThemeMode, (m: ThemeMode) => void, "light" | "dark"] {
  const [mode, setMode] = useState<ThemeMode>(readMode);
  const [sysDark, setSysDark] = useState(systemDark);

  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    const on = () => setSysDark(mq.matches);
    mq?.addEventListener("change", on);
    return () => mq?.removeEventListener("change", on);
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    if (mode === "auto") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", mode);
    try {
      localStorage.setItem(KEY, mode);
    } catch {
      /* pomijamy */
    }
  }, [mode]);

  const effective = mode === "auto" ? (sysDark ? "dark" : "light") : mode;
  return [mode, setMode, effective];
}

/** Ustawia motyw przed pierwszym renderem, żeby nie mignął jasny ekran w trybie ciemnym. */
export function applyStoredTheme() {
  const m = readMode();
  if (m !== "auto") document.documentElement.setAttribute("data-theme", m);
}
