import { useEffect, useState, type ReactNode } from "react";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { api, type ScriptTool } from "../lib/api";
import { GlobeIcon, PencilIcon, PlusIcon, ToolsIcon, TrashIcon } from "./Icons";

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

/** Narzędzia wbudowane. Nowa akcja systemowa to kolejna pozycja na tej liście. */
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

type Draft = { id: string | null; name: string; path: string; admin: boolean };
const EMPTY: Draft = { id: null, name: "", path: "", admin: false };

/** Strona Narzędzi w głównym obszarze. */
export function ToolsView({ onError }: Props) {
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<Record<string, string>>({});
  const [scripts, setScripts] = useState<ScriptTool[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);

  useEffect(() => {
    api.toolScripts().then(setScripts, (e) => onError(String(e)));
  }, [onError]);

  const exec = async (id: string, fn: () => Promise<string>) => {
    setBusy(id);
    setDone((d) => ({ ...d, [id]: "" }));
    try {
      const msg = await fn();
      setDone((d) => ({ ...d, [id]: msg }));
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const saveList = async (list: ScriptTool[]) => {
    try {
      await api.setToolScripts(list);
      setScripts(list);
      return true;
    } catch (e) {
      onError(String(e));
      return false;
    }
  };

  const pickScript = async () => {
    if (!draft) return;
    const p = await open({
      title: "Skrypt PowerShell (.ps1)",
      filters: [{ name: "PowerShell", extensions: ["ps1"] }],
      defaultPath: draft.path || undefined,
    });
    if (typeof p === "string") {
      const base = p.split(/[\\/]/).pop()!.replace(/\.ps1$/i, "");
      setDraft({ ...draft, path: p, name: draft.name || base });
    }
  };

  const saveDraft = async () => {
    if (!draft) return;
    const item: ScriptTool = {
      id: draft.id ?? `script-${Date.now().toString(36)}`,
      name: draft.name.trim(),
      path: draft.path.trim().replace(/^"|"$/g, ""),
      admin: draft.admin,
    };
    const list = draft.id ? scripts.map((s) => (s.id === draft.id ? item : s)) : [...scripts, item];
    if (await saveList(list)) setDraft(null);
  };

  const remove = async (s: ScriptTool) => {
    const ok = await ask(`Usunąć narzędzie „${s.name}”? Sam plik skryptu zostaje na dysku.`, {
      title: "Narzędzia",
      kind: "warning",
    });
    if (ok) void saveList(scripts.filter((x) => x.id !== s.id));
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
              <button
                className="primary"
                disabled={busy !== null}
                onClick={() => void exec(t.id, () => t.run().then(() => t.after))}
              >
                {busy === t.id ? "Otwieranie…" : t.button}
              </button>
            </div>
          </div>
        ))}

        {scripts.map((s) => (
          <div key={s.id} className="mcp-card">
            <div className="tool-icon">
              <ToolsIcon size={20} />
            </div>
            <div className="mcp-body">
              <div className="mcp-name">{s.name}</div>
              <div className="tool-about">
                Skrypt PowerShell{s.admin ? ", uruchamiany jako administrator (Windows zapyta o zgodę)" : ""}.
              </div>
              <div className="mcp-config" title={s.path}>
                {s.path}
              </div>
              {done[s.id] && <div className="mcp-after tool-output">{done[s.id]}</div>}
            </div>
            <div className="mcp-actions">
              <button className="ghost icon-only" disabled={busy !== null} title="Edytuj" onClick={() => setDraft({ ...s })}>
                <PencilIcon size={16} />
              </button>
              <button className="ghost icon-only" disabled={busy !== null} title="Usuń z listy" onClick={() => void remove(s)}>
                <TrashIcon size={16} />
              </button>
              <button
                className="primary"
                disabled={busy !== null}
                onClick={() => void exec(s.id, () => api.toolRunScript(s.id).then((out) => out || "Gotowe."))}
              >
                {busy === s.id ? "Działa…" : "Uruchom"}
              </button>
            </div>
          </div>
        ))}

        {draft ? (
          <div className="mcp-card tool-form">
            <div className="mcp-body">
              <div className="mcp-name">{draft.id ? "Edytuj narzędzie" : "Nowe narzędzie: skrypt .ps1"}</div>
              <div className="field">
                <label>Skrypt</label>
                <div className="pick">
                  <input
                    value={draft.path}
                    placeholder={String.raw`C:\tools\reset-displaylink.ps1`}
                    onChange={(e) => setDraft({ ...draft, path: e.target.value })}
                  />
                  <button onClick={() => void pickScript()}>Wybierz…</button>
                </div>
              </div>
              <div className="field">
                <label>Nazwa</label>
                <input
                  value={draft.name}
                  placeholder="Restart monitorów"
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                />
              </div>
              <label className="tool-check">
                <input
                  type="checkbox"
                  checked={draft.admin}
                  onChange={(e) => setDraft({ ...draft, admin: e.target.checked })}
                />{" "}
                Uruchamiaj jako administrator (np. restart sterowników)
              </label>
              <div className="actions">
                <button className="ghost" onClick={() => setDraft(null)}>
                  Anuluj
                </button>
                <button
                  className="primary"
                  disabled={!draft.name.trim() || !draft.path.trim()}
                  onClick={() => void saveDraft()}
                >
                  Zapisz
                </button>
              </div>
            </div>
          </div>
        ) : (
          <button className="ghost tool-add" onClick={() => setDraft(EMPTY)}>
            <PlusIcon size={16} /> Dodaj skrypt .ps1
          </button>
        )}
      </div>
    </div>
  );
}
