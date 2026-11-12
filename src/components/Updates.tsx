// Aktualizacje z wydań na GitHubie (wtyczka updater Tauri; latest.json i instalator podpisany kluczem wydawcy).
import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "none" }
  | { kind: "available"; update: Update }
  | { kind: "installing"; percent: number | null }
  | { kind: "error"; text: string };

const SKIP_KEY = "notes-workbench.updates.skip-version";

/** Stan aktualizacji wspólny dla paska powiadomienia i stopki panelu. */
export function useUpdates() {
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const [dismissed, setDismissed] = useState(false);
  const [version, setVersion] = useState("");
  const busy = useRef(false);

  const runCheck = async (manual: boolean) => {
    if (busy.current || import.meta.env.DEV) {
      if (import.meta.env.DEV && manual) setState({ kind: "error", text: "w wersji deweloperskiej aktualizacje są wyłączone" });
      return;
    }
    busy.current = true;
    setState({ kind: "checking" });
    try {
      const update = await check();
      if (!update) setState({ kind: "none" });
      else {
        setState({ kind: "available", update });
        let skipped: string | null = null;
        try {
          skipped = localStorage.getItem(SKIP_KEY);
        } catch {
          /* brak pamięci przeglądarki: pokazujemy zawsze */
        }
        setDismissed(!manual && skipped === update.version);
      }
    } catch (e) {
      // Automatyczne sprawdzenie bez internetu nie powinno niepokoić: błąd tylko przy ręcznym.
      setState(manual ? { kind: "error", text: String(e) } : { kind: "idle" });
    } finally {
      busy.current = false;
    }
  };

  useEffect(() => {
    getVersion().then(setVersion, () => {});
    const id = window.setTimeout(() => void runCheck(false), 4000);
    return () => window.clearTimeout(id);
  }, []);

  const install = async () => {
    if (state.kind !== "available") return;
    const update = state.update;
    let total = 0;
    let done = 0;
    setState({ kind: "installing", percent: null });
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          done += ev.data.chunkLength;
          if (total) setState({ kind: "installing", percent: Math.min(100, Math.round((done / total) * 100)) });
        }
      });
      // Na Windows instalator sam zamyka program; na wszelki wypadek uruchamiamy go ponownie.
      await relaunch();
    } catch (e) {
      setState({ kind: "error", text: String(e) });
    }
  };

  const later = () => {
    setDismissed(true);
    if (state.kind === "available") {
      try {
        localStorage.setItem(SKIP_KEY, state.update.version);
      } catch {
        /* bez znaczenia */
      }
    }
  };

  return { state, dismissed, version, check: () => void runCheck(true), install: () => void install(), later };
}

export type Updates = ReturnType<typeof useUpdates>;

/** Pasek w prawym dolnym rogu: „Dostępna nowa wersja…”. */
export function UpdateBanner({ updates }: { updates: Updates }) {
  const { state, dismissed } = updates;
  if (state.kind === "installing")
    return (
      <div className="update-banner" role="status">
        <span>Pobieranie aktualizacji…{state.percent == null ? "" : ` ${state.percent}%`}</span>
      </div>
    );
  if (state.kind !== "available" || dismissed) return null;
  return (
    <div className="update-banner" role="status">
      <span>Dostępna nowa wersja {state.update.version}.</span>
      <button className="primary" onClick={updates.install}>
        Zainstaluj i uruchom ponownie
      </button>
      <button onClick={updates.later}>Później</button>
    </div>
  );
}

/** Wiersz w stopce panelu: wersja programu i ręczne sprawdzenie aktualizacji. */
export function UpdateFooter({ updates }: { updates: Updates }) {
  const { state, version } = updates;
  const status =
    state.kind === "checking"
      ? "Sprawdzanie…"
      : state.kind === "none"
        ? "Masz najnowszą wersję."
        : state.kind === "available"
          ? `Dostępna wersja ${state.update.version}.`
          : state.kind === "installing"
            ? "Pobieranie aktualizacji…"
            : state.kind === "error"
              ? `Nie udało się sprawdzić aktualizacji: ${state.text}`
              : "";
  return (
    <div className="update-footer">
      <span className="muted">{version ? `Wersja ${version}` : "Notes Workbench"}</span>
      {state.kind === "available" ? (
        <button className="ghost small-btn" onClick={updates.install}>
          Zainstaluj {state.update.version}
        </button>
      ) : (
        <button
          className="ghost small-btn"
          disabled={state.kind === "checking" || state.kind === "installing"}
          onClick={updates.check}
          title={status || undefined}
        >
          {state.kind === "checking" ? "Sprawdzanie…" : "Sprawdź aktualizacje"}
        </button>
      )}
      {status && state.kind !== "checking" && <div className={`update-status${state.kind === "error" ? " bad" : ""}`}>{status}</div>}
    </div>
  );
}
