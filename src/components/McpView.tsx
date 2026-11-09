import { useCallback, useEffect, useState } from "react";
import { api, type McpClient, type McpStatus } from "../lib/api";
import { CheckIcon, LockIcon, PlugIcon, SearchIcon, SparkIcon, FileIcon } from "./Icons";

type Props = { onError: (msg: string) => void; onOpenProposals: () => void };

const LOGO: Record<McpClient["id"], { letter: string; color: string }> = {
  "claude-desktop": { letter: "C", color: "#d97757" },
  "claude-code": { letter: ">_", color: "#c4623f" },
  codex: { letter: "X", color: "#10a37f" },
};

/** Podłączanie serwera MCP Notes Workbench do Claude Desktop, Claude Code i Codex. */
export function McpView({ onError, onOpenProposals }: Props) {
  const [st, setSt] = useState<McpStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<Record<string, string>>({});
  const [copied, setCopied] = useState(false);

  const load = useCallback(() => {
    api.mcpStatus().then(setSt).catch((e) => onError(String(e)));
  }, [onError]);

  useEffect(() => {
    load();
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load]);

  const act = async (c: McpClient, connect: boolean) => {
    setBusy(c.id);
    try {
      const s = connect ? await api.mcpConnect(c.id) : await api.mcpDisconnect(c.id);
      setSt(s);
      setDone((d) => ({ ...d, [c.id]: connect ? c.after : "Odłączono. Uruchom program ponownie, żeby zniknął serwer." }));
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const copy = async () => {
    if (!st) return;
    try {
      await navigator.clipboard.writeText(st.snippet);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="note">
      <div className="note-head">
        <h1 className="note-title">Połącz z AI</h1>
        <p className="muted mcp-lead">
          Serwer MCP pozwala Claude albo Codex szukać i czytać Twoje Notatki oraz proponować powiązania i Tagi. Notatek nie
          zmienia: Propozycje zatwierdzasz w aplikacji. Działa lokalnie, na tym komputerze.
        </p>
        {st && (
          <div className="mcp-vault">
            Udostępniany Skarbiec: <b>{st.vault}</b>
          </div>
        )}
      </div>

      <div className="mcp-clients">
        {!st && <div className="muted">Sprawdzanie…</div>}
        {st?.clients.map((c) => (
          <div key={c.id} className={`mcp-card${c.connected ? " on" : ""}`}>
            <div className="mcp-logo" style={{ background: LOGO[c.id].color }}>
              {LOGO[c.id].letter}
            </div>
            <div className="mcp-body">
              <div className="mcp-name">
                {c.name}
                {c.connected ? (
                  <span className="pill ok">
                    <CheckIcon size={12} /> Połączony
                  </span>
                ) : c.detected ? (
                  <span className="pill">Niepołączony</span>
                ) : (
                  <span className="pill muted-pill">Nie znaleziono programu</span>
                )}
              </div>
              {c.config && (
                <div className="mcp-config" title={c.config}>
                  {c.config}
                </div>
              )}
              {done[c.id] && <div className="mcp-after">{done[c.id]}</div>}
            </div>
            <div className="mcp-actions">
              {c.connected ? (
                <>
                  <button disabled={busy !== null} onClick={() => void act(c, true)} title="Zapisz wpis ponownie (np. po zmianie Skarbca)">
                    Odśwież
                  </button>
                  <button className="danger" disabled={busy !== null} onClick={() => void act(c, false)}>
                    Rozłącz
                  </button>
                </>
              ) : (
                <button
                  className="primary"
                  disabled={busy !== null}
                  onClick={() => void act(c, true)}
                  title={c.detected ? undefined : "Programu nie wykryto, ale wpis można dodać zawczasu."}
                >
                  <PlugIcon size={15} /> {busy === c.id ? "Łączenie…" : "Połącz"}
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      <section className="mcp-section">
        <h2>Co AI może zrobić</h2>
        <ul className="mcp-tools">
          <li>
            <SearchIcon size={15} /> Szukać po słowach i po znaczeniu, przeglądać Foldery i Tagi
          </li>
          <li>
            <FileIcon size={15} /> Czytać Notatki z Linkami, Backlinkami i nazwami Załączników
          </li>
          <li>
            <SparkIcon size={15} /> Proponować powiązania i Tagi, które zatwierdzasz w widoku{" "}
            <a href="#" onClick={(e) => (e.preventDefault(), onOpenProposals())}>
              Propozycje
            </a>
          </li>
          <li>
            <LockIcon size={15} /> Nie widzi Notatek oznaczonych „Ukryj przed AI” (#noai) ani Kosza
          </li>
        </ul>
        <p className="muted small">
          Przykład: „Przejrzyj moje notatki z folderu Projects i zaproponuj powiązania oraz Tagi”.
        </p>
      </section>

      <section className="mcp-section">
        <h2>Inne programy (Cursor, Windsurf…)</h2>
        <p className="muted small">
          Wklej ten wpis do sekcji <code>mcpServers</code> w ich konfiguracji. Najpierw kliknij „Połącz” przy dowolnym
          programie powyżej, żeby serwer został zainstalowany w tym miejscu.
        </p>
        <div className="mcp-snippet">
          <pre>{st?.snippet ?? ""}</pre>
          <button className="ghost" onClick={() => void copy()}>
            {copied ? <CheckIcon size={14} /> : null} {copied ? "Skopiowano" : "Kopiuj"}
          </button>
        </div>
      </section>
    </div>
  );
}
