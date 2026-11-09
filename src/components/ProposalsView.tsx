import { useCallback, useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { api, folderOf, type Proposal } from "../lib/api";

type Props = {
  onOpen: (path: string) => void;
  onError: (msg: string) => void;
  /** Po akceptacji zmieniły się Notatki: odśwież indeks i licznik. */
  onChanged: () => void;
};

type Filter = "all" | "relation" | "tag";

/** Propozycje powiązań i Tagów od AI (np. Claude przez serwer MCP), czekające na decyzję. */
export function ProposalsView({ onOpen, onError, onChanged }: Props) {
  const [items, setItems] = useState<Proposal[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api
      .proposals()
      .then(setItems)
      .catch((e) => onError(String(e)));
  }, [onError]);

  useEffect(() => {
    load();
    // Claude mógł dodać Propozycje w czasie, gdy okno było w tle.
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load]);

  const act = async (accept: boolean, ids: number[]) => {
    setBusy(true);
    try {
      if (accept) await api.acceptProposals(ids);
      else await api.rejectProposals(ids);
      setItems((list) => list?.filter((p) => !ids.includes(p.id)) ?? null);
      onChanged();
    } catch (e) {
      onError(String(e));
      load();
    } finally {
      setBusy(false);
    }
  };

  if (!items) return <div className="note muted pad">Wczytywanie…</div>;
  const shown = items.filter((p) => filter === "all" || p.kind === filter);
  const rel = items.filter((p) => p.kind === "relation").length;

  const bulk = async (accept: boolean) => {
    const what = accept ? "Zaakceptować" : "Odrzucić";
    const ok = await ask(`${what} wszystkie widoczne Propozycje (${shown.length})?`, { title: "Propozycje", kind: "warning" });
    if (ok) void act(accept, shown.map((p) => p.id));
  };

  const link = (path: string, title: string) => (
    <a href="#" className="wikilink" title={path} onClick={(e) => (e.preventDefault(), onOpen(path))}>
      {title}
    </a>
  );

  return (
    <div className="note">
      <div className="note-head">
        <h1 className="note-title">Propozycje</h1>
        <p className="muted">
          Powiązania i Tagi zaproponowane przez AI (np. Claude przez serwer MCP). Do Notatek trafiają dopiero po akceptacji:
          powiązanie jako <code>powiazane</code>, Tag jako <code>tags</code> we frontmatterze. Odrzucone nie zostaną
          zaproponowane ponownie.
        </p>
        <div className="proposal-bar">
          <div className="tabs">
            <button className={`tab${filter === "all" ? " on" : ""}`} onClick={() => setFilter("all")}>
              Wszystkie ({items.length})
            </button>
            <button className={`tab${filter === "relation" ? " on" : ""}`} onClick={() => setFilter("relation")}>
              Powiązania ({rel})
            </button>
            <button className={`tab${filter === "tag" ? " on" : ""}`} onClick={() => setFilter("tag")}>
              Tagi ({items.length - rel})
            </button>
          </div>
          {shown.length > 1 && (
            <span className="proposal-bulk">
              <button disabled={busy} onClick={() => void bulk(false)}>
                Odrzuć widoczne
              </button>
              <button className="primary" disabled={busy} onClick={() => void bulk(true)}>
                Akceptuj widoczne ({shown.length})
              </button>
            </span>
          )}
        </div>
      </div>
      {shown.length === 0 ? (
        <div className="muted proposal-empty">
          Brak Propozycji. Poproś Claude (z podłączonym serwerem MCP Notes Workbench), np. „przejrzyj moje notatki o
          projektach i zaproponuj powiązania”.
        </div>
      ) : (
        <ul className="proposals">
          {shown.map((p) => (
            <li key={p.id}>
              <div className="proposal-what">
                {p.kind === "relation" ? (
                  <>
                    <span className="proposal-kind">Powiązanie</span>
                    {link(p.note, p.note_title)} ↔ {link(p.target!, p.target_title ?? p.target!)}
                  </>
                ) : (
                  <>
                    <span className="proposal-kind tag-kind">Tag</span>
                    <span className="tag-name">#{p.tag}</span> dla {link(p.note, p.note_title)}
                  </>
                )}
              </div>
              <div className="proposal-reason">{p.reason}</div>
              <div className="proposal-meta muted small">
                {[folderOf(p.note), p.source, p.created].filter(Boolean).join(" · ")}
              </div>
              <div className="proposal-actions">
                <button disabled={busy} onClick={() => void act(false, [p.id])}>
                  Odrzuć
                </button>
                <button className="primary" disabled={busy} onClick={() => void act(true, [p.id])}>
                  Akceptuj
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
