import { useCallback, useEffect, useMemo, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { api, folderOf, type Proposal } from "../lib/api";
import { collapse, diffLines } from "../lib/diff";

type Props = {
  onOpen: (path: string) => void;
  onError: (msg: string) => void;
  /** Po akceptacji zmieniły się Notatki: odśwież indeks i licznik. */
  onChanged: () => void;
};

type Filter = "all" | "relation" | "tag" | "text";

const isText = (p: Proposal) => p.kind === "note" || p.kind === "edit";

/** Potwierdzenie w oknie systemowym; w podglądzie przeglądarki zwykłe `confirm`. */
async function confirmBox(msg: string): Promise<boolean> {
  try {
    return await ask(msg, { title: "Propozycje", kind: "warning" });
  } catch {
    return window.confirm(msg);
  }
}

/** Różnice zmiany treści, z długimi fragmentami bez zmian zwiniętymi. */
function EditDiff({ base, body }: { base: string; body: string }) {
  const [full, setFull] = useState(false);
  const lines = useMemo(() => diffLines(base, body), [base, body]);
  const rows = full ? lines : collapse(lines);
  const added = lines.filter((l) => l.kind === "add").length;
  const removed = lines.filter((l) => l.kind === "del").length;
  return (
    <div className="proposal-text">
      <div className="proposal-text-bar">
        <span className="diff-stat add">+{added}</span>
        <span className="diff-stat del">−{removed}</span>
        <button className="ghost small-btn" onClick={() => setFull(!full)}>
          {full ? "Tylko zmiany" : "Pokaż całość"}
        </button>
      </div>
      <pre className="diff">
        {rows.map((r, i) =>
          r.kind === "skip" ? (
            <div key={i} className="diff-skip">
              ⋯ {r.count} {r.count === 1 ? "linia" : "linii"} bez zmian
            </div>
          ) : (
            <div key={i} className={`diff-${r.kind}`}>
              <span className="diff-sign">{r.kind === "add" ? "+" : r.kind === "del" ? "−" : " "}</span>
              {r.text || " "}
            </div>
          ),
        )}
      </pre>
    </div>
  );
}

/** Treść proponowanej Notatki, zwinięta do kilku linii. */
function NoteBody({ body }: { body: string }) {
  const [full, setFull] = useState(false);
  const lines = body.split("\n");
  const long = lines.length > 12;
  return (
    <div className="proposal-text">
      <pre className="diff">
        {(full || !long ? lines : lines.slice(0, 12)).map((l, i) => (
          <div key={i} className="diff-add">
            <span className="diff-sign">+</span>
            {l || " "}
          </div>
        ))}
      </pre>
      {long && (
        <button className="ghost small-btn" onClick={() => setFull(!full)}>
          {full ? "Zwiń" : `Pokaż całość (${lines.length} linii)`}
        </button>
      )}
    </div>
  );
}

/** Propozycje od AI (np. Claude przez serwer MCP): powiązania, Tagi, nowe Notatki i zmiany treści. */
export function ProposalsView({ onOpen, onError, onChanged }: Props) {
  const [items, setItems] = useState<Proposal[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [failed, setFailed] = useState<Map<number, string>>(new Map());
  const [info, setInfo] = useState("");

  const load = useCallback(() => {
    api
      .proposals()
      .then((list) => {
        setItems(list);
        // Zaznaczenie tylko wśród istniejących.
        setSelected((s) => new Set([...s].filter((id) => list.some((p) => p.id === id))));
      })
      .catch((e) => onError(String(e)));
  }, [onError]);

  useEffect(() => {
    load();
    // Claude mógł dodać Propozycje w czasie, gdy okno było w tle.
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load]);

  const act = async (accept: boolean, ids: number[]) => {
    if (!ids.length) return;
    setBusy(true);
    setInfo("");
    try {
      let gone = ids;
      if (accept) {
        const r = await api.acceptProposals(ids);
        const bad = new Map(r.failed);
        gone = ids.filter((id) => !bad.has(id));
        setFailed((f) => {
          const next = new Map(f);
          ids.forEach((id) => next.delete(id));
          bad.forEach((msg, id) => next.set(id, msg));
          return next;
        });
        setInfo(
          r.failed.length
            ? `Zaakceptowano ${r.done}, nie udało się ${r.failed.length} (powód przy Propozycji).`
            : ids.length > 1
              ? `Zaakceptowano ${r.done}.`
              : "",
        );
      } else {
        await api.rejectProposals(ids);
        if (ids.length > 1) setInfo(`Odrzucono ${ids.length}.`);
      }
      setItems((list) => list?.filter((p) => !gone.includes(p.id)) ?? null);
      setSelected((s) => new Set([...s].filter((id) => !gone.includes(id))));
      onChanged();
    } catch (e) {
      onError(String(e));
      load();
    } finally {
      setBusy(false);
    }
  };

  if (!items) return <div className="note muted pad">Wczytywanie…</div>;
  const shown = items.filter(
    (p) => filter === "all" || (filter === "text" ? isText(p) : p.kind === filter),
  );
  const count = (f: (p: Proposal) => boolean) => items.filter(f).length;
  const chosen = shown.filter((p) => selected.has(p.id));
  const allChosen = shown.length > 0 && chosen.length === shown.length;
  // Bez zaznaczenia przyciski działają na wszystkie widoczne.
  const target = chosen.length ? chosen : shown;
  const targetLabel = chosen.length ? `zaznaczone (${chosen.length})` : `wszystkie widoczne (${shown.length})`;

  const toggle = (id: number) =>
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const bulk = async (accept: boolean) => {
    const what = accept ? "Zaakceptować" : "Odrzucić";
    const texts = target.filter(isText).length;
    const extra = accept && texts ? `\n\nW tym ${texts} nowych Notatek lub zmian treści: zostaną zapisane w Skarbcu.` : "";
    if (await confirmBox(`${what} ${targetLabel} Propozycje?${extra}`)) void act(accept, target.map((p) => p.id));
  };

  const link = (path: string, title: string) => (
    <a href="#" className="wikilink" title={path} onClick={(e) => (e.preventDefault(), onOpen(path))}>
      {title}
    </a>
  );

  const tab = (f: Filter, label: string, n: number) => (
    <button className={`tab${filter === f ? " on" : ""}`} onClick={() => setFilter(f)}>
      {label} ({n})
    </button>
  );

  return (
    <div className="note">
      <div className="note-head">
        <h1 className="note-title">Propozycje</h1>
        <p className="muted">
          Zmiany zaproponowane przez AI (np. Claude przez serwer MCP): powiązania, Tagi, nowe Notatki i zmiany treści.
          Do Skarbca trafiają dopiero po akceptacji. Odrzucone nie zostaną zaproponowane ponownie.
        </p>
        <div className="proposal-bar">
          <div className="tabs">
            {tab("all", "Wszystkie", items.length)}
            {tab("relation", "Powiązania", count((p) => p.kind === "relation"))}
            {tab("tag", "Tagi", count((p) => p.kind === "tag"))}
            {tab("text", "Notatki i zmiany", count(isText))}
          </div>
        </div>
        {shown.length > 0 && (
          <div className="proposal-bulk-bar">
            <label className="proposal-check-all">
              <input
                type="checkbox"
                checked={allChosen}
                ref={(el) => {
                  if (el) el.indeterminate = chosen.length > 0 && !allChosen;
                }}
                onChange={() =>
                  setSelected((s) => {
                    const next = new Set(s);
                    shown.forEach((p) => (allChosen ? next.delete(p.id) : next.add(p.id)));
                    return next;
                  })
                }
              />
              {chosen.length ? `Zaznaczono ${chosen.length} z ${shown.length}` : "Zaznacz wszystkie"}
            </label>
            <span className="proposal-bulk">
              <button disabled={busy} onClick={() => void bulk(false)}>
                Odrzuć {targetLabel}
              </button>
              <button className="primary" disabled={busy} onClick={() => void bulk(true)}>
                Akceptuj {targetLabel}
              </button>
            </span>
          </div>
        )}
        {info && <div className="proposal-info">{info}</div>}
      </div>
      {shown.length === 0 ? (
        <div className="muted proposal-empty">
          Brak Propozycji. Poproś Claude (z podłączonym serwerem MCP Notes Workbench), np. „przejrzyj moje notatki o
          projektach i zaproponuj powiązania” albo „napisz Notatkę podsumowującą projekt X”.
        </div>
      ) : (
        <ul className="proposals">
          {shown.map((p) => (
            <li key={p.id} className={`${selected.has(p.id) ? "chosen" : ""}${failed.has(p.id) ? " failed" : ""}`}>
              <input
                type="checkbox"
                className="proposal-check"
                checked={selected.has(p.id)}
                onChange={() => toggle(p.id)}
                aria-label="Zaznacz Propozycję"
              />
              <div className="proposal-main">
                <div className="proposal-what">
                  {p.kind === "relation" && (
                    <>
                      <span className="proposal-kind">Powiązanie</span>
                      {link(p.note, p.note_title)} ↔ {link(p.target!, p.target_title ?? p.target!)}
                    </>
                  )}
                  {p.kind === "tag" && (
                    <>
                      <span className="proposal-kind tag-kind">Tag</span>
                      <span className="tag-name">#{p.tag}</span> dla {link(p.note, p.note_title)}
                    </>
                  )}
                  {p.kind === "note" && (
                    <>
                      <span className="proposal-kind note-kind">Nowa Notatka</span>
                      <b>{p.title}</b>
                      <span className="muted"> w {p.note ? p.note.replace(/\//g, " / ") : "głównym Folderze"}</span>
                    </>
                  )}
                  {p.kind === "edit" && (
                    <>
                      <span className="proposal-kind edit-kind">Zmiana treści</span>
                      {link(p.note, p.note_title)}
                    </>
                  )}
                </div>
                <div className="proposal-reason">{p.reason}</div>
                {p.kind === "note" && <NoteBody body={p.body ?? ""} />}
                {p.kind === "edit" && <EditDiff base={p.base ?? ""} body={p.body ?? ""} />}
                {failed.has(p.id) && <div className="proposal-error">{failed.get(p.id)}</div>}
                <div className="proposal-meta muted small">
                  {[p.kind === "note" ? "" : folderOf(p.note), p.source, p.created].filter(Boolean).join(" · ")}
                </div>
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
