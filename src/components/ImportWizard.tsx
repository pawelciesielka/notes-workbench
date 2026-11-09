import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { api, type Report } from "../lib/api";

type Props = {
  onDone: (report: Report) => void;
  onCancel?: () => void;
};

export function ImportWizard({ onDone, onCancel }: Props) {
  const [exportPath, setExportPath] = useState("");
  const [vault, setVault] = useState("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    const un = listen<{ done: number; total: number }>("import-progress", (e) => setProgress(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);

  const pickExport = async () => {
    const p = await open({ filters: [{ name: "Eksport AMNO (HTML)", extensions: ["htm", "html"] }] });
    if (typeof p === "string") setExportPath(p);
  };
  const pickVault = async () => {
    const p = await open({ directory: true, title: "Wskaż pusty katalog na Skarbiec" });
    if (typeof p === "string") setVault(p);
  };
  const run = async () => {
    setBusy(true);
    setError("");
    setProgress(null);
    try {
      onDone(await api.importAmno(exportPath, vault));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="wizard">
      <h1>Import z All My Notes Organizer</h1>
      <p className="muted">
        W AMNO wybierz Plik → Eksport, typ „HTML Document”. Powstanie plik <code>.htm</code> i obok niego katalog{" "}
        <code>… - files</code> z obrazami i załącznikami. Import zapisze wszystko jako Notatki Markdown w pustym katalogu
        Skarbca i sprawdzi, czy nic nie zginęło.
      </p>
      <div className="field">
        <label>1. Eksport AMNO</label>
        <div className="pick">
          <input readOnly value={exportPath} placeholder="My Data.htm" />
          <button onClick={pickExport} disabled={busy}>
            Wybierz…
          </button>
        </div>
      </div>
      <div className="field">
        <label>2. Katalog Skarbca (pusty)</label>
        <div className="pick">
          <input readOnly value={vault} placeholder="np. C:\Notatki" />
          <button onClick={pickVault} disabled={busy}>
            Wybierz…
          </button>
        </div>
      </div>
      {error && <div className="banner bad">{error}</div>}
      {busy && (
        <div className="progress">
          <div
            className="progress-bar"
            style={{ width: progress ? `${(100 * progress.done) / Math.max(1, progress.total)}%` : "3%" }}
          />
          <span>{progress ? `Notatki: ${progress.done} / ${progress.total}` : "Wczytywanie eksportu…"}</span>
        </div>
      )}
      <div className="actions">
        {onCancel && (
          <button onClick={onCancel} disabled={busy}>
            Anuluj
          </button>
        )}
        <button className="primary" onClick={run} disabled={busy || !exportPath || !vault}>
          Importuj
        </button>
      </div>
    </div>
  );
}
