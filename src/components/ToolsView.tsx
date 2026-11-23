import { useState, type ReactNode } from "react";
import { api } from "../lib/api";
import { GlobeIcon } from "./Icons";

type Props = { onError: (msg: string) => void };

type Tool = {
  id: string;
  name: string;
  icon: ReactNode;
  /** Co robi akcja, jednym zdaniem. */
  about: string;
  /** Szczegół techniczny pod opisem (ścieżka, polecenie). */
  detail?: string;
  button: string;
  /** Komunikat po udanym uruchomieniu. */
  after: string;
  run: () => Promise<void>;
};

/** Narzędzia: często używane akcje systemowe. Nowa akcja to kolejna pozycja na tej liście. */
const TOOLS: Tool[] = [
  {
    id: "hosts",
    name: "Plik hosts",
    icon: <GlobeIcon size={20} />,
    about: "Otwiera plik hosts w Notatniku jako administrator, żeby można go było zapisać. Windows zapyta o zgodę (UAC).",
    detail: String.raw`C:\Windows\System32\drivers\etc\hosts`,
    button: "Edytuj",
    after: "Otwarto w Notatniku.",
    run: api.toolEditHosts,
  },
];

/** Strona Narzędzi w głównym obszarze. */
export function ToolsView({ onError }: Props) {
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<Record<string, string>>({});

  const run = async (t: Tool) => {
    setBusy(t.id);
    setDone((d) => ({ ...d, [t.id]: "" }));
    try {
      await t.run();
      setDone((d) => ({ ...d, [t.id]: t.after }));
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="note">
      <div className="note-head">
        <h1 className="note-title">Narzędzia</h1>
        <p className="muted mcp-lead">Akcje, których używasz co jakiś czas, w jednym miejscu.</p>
      </div>

      <div className="mcp-clients">
        {TOOLS.map((t) => (
          <div key={t.id} className="mcp-card">
            <div className="tool-icon">{t.icon}</div>
            <div className="mcp-body">
              <div className="mcp-name">{t.name}</div>
              <div className="tool-about">{t.about}</div>
              {t.detail && (
                <div className="mcp-config" title={t.detail}>
                  {t.detail}
                </div>
              )}
              {done[t.id] && <div className="mcp-after">{done[t.id]}</div>}
            </div>
            <div className="mcp-actions">
              <button className="primary" disabled={busy !== null} onClick={() => void run(t)}>
                {busy === t.id ? "Otwieranie…" : t.button}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
