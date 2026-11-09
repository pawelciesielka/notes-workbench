import { useState } from "react";
import type { Report } from "../lib/api";

const COUNT_LABELS: [string, string][] = [
  ["notes_in_export", "Notatki w eksporcie"],
  ["notes_written", "Notatki zapisane"],
  ["notes_in_trash", "w tym w Koszu"],
  ["folders_in_export", "Foldery w eksporcie"],
  ["folders_written", "Foldery zapisane"],
  ["images", "Obrazy"],
  ["attachments", "Załączniki (inne pliki)"],
  ["files_in_export", "Pliki w katalogu eksportu"],
  ["files_copied", "Pliki skopiowane"],
  ["files_unreferenced", "Pliki nieprzypisane"],
];

const LEVELS = ["błąd", "ostrzeżenie", "zmiana"] as const;

type Props = {
  report: Report;
  onOpen: (notePath: string) => void;
};

export function ReportView({ report, onOpen }: Props) {
  const [level, setLevel] = useState<(typeof LEVELS)[number]>(report.errors > 0 ? "błąd" : report.warnings > 0 ? "ostrzeżenie" : "zmiana");
  const issues = report.issues.filter((i) => i.level === level);
  const count = (l: string) => report.issues.filter((i) => i.level === l).length;

  return (
    <div className="note">
      <div className="note-head">
        <div className="crumbs">{report.source}</div>
        <h1 className="note-title">Raport importu</h1>
      </div>
      <div className={`banner ${report.ok ? "ok" : "bad"}`}>
        {report.ok
          ? "Import bez błędów: tekst każdej Notatki zgadza się z eksportem, a pliki z sumami kontrolnymi."
          : `Import zakończony z błędami (${report.errors}). Obejrzyj wskazane Notatki.`}
      </div>
      <table className="counts">
        <tbody>
          {COUNT_LABELS.map(([k, label]) => (
            <tr key={k}>
              <td>{label}</td>
              <td className="num">{report.counts[k] ?? 0}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="tabs">
        {LEVELS.map((l) => (
          <button key={l} className={`tab${l === level ? " on" : ""}`} onClick={() => setLevel(l)}>
            {l === "błąd" ? "Błędy" : l === "ostrzeżenie" ? "Ostrzeżenia" : "Zmiany"} ({count(l)})
          </button>
        ))}
      </div>
      {issues.length === 0 ? (
        <div className="muted pad">Brak.</div>
      ) : (
        <ul className="issues">
          {issues.map((i, n) => (
            <li key={n}>
              <span className="issue-kind">{i.kind}</span>
              {i.note && (
                <a
                  href="#"
                  onClick={(e) => {
                    e.preventDefault();
                    onOpen(i.note!);
                  }}
                >
                  {i.note}
                </a>
              )}
              <div className="issue-msg">{i.message}</div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
