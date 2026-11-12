import { Fragment, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type SearchHit, type SemanticInfo } from "../lib/api";
import { SearchIcon, XIcon } from "./Icons";

type Props = {
  /** Panel jest widoczny: dopiero wtedy wczytujemy model wyszukiwania po znaczeniu. */
  active: boolean;
  activeNote: string | null;
  /** Zmienia się, gdy trzeba ustawić kursor w polu (skrót Ctrl+Shift+F). */
  focusKey: number;
  onOpen: (path: string) => void;
  onError: (msg: string) => void;
};

type Mode = "words" | "meaning";

/** Fragment z trafieniami oznaczonymi znakami \u0001 … \u0002. */
function Highlighted({ text }: { text: string }) {
  const parts = text.split(/(\u0001[^\u0002]*\u0002)/);
  return (
    <>
      {parts.map((p, i) =>
        p.startsWith("\u0001") ? <mark key={i}>{p.slice(1, -1)}</mark> : <Fragment key={i}>{p}</Fragment>,
      )}
    </>
  );
}

/** Stan wyszukiwania semantycznego: włączanie, pobieranie modelu, postęp liczenia wektorów. */
function SemanticState({ info, onEnable }: { info: SemanticInfo | null; onEnable: () => void }) {
  if (!info) return null;
  if (!info.enabled)
    return (
      <div className="pad semantic-box">
        <p>
          Wyszukiwanie po znaczeniu znajduje Notatki o podobnej treści, nawet gdy nie mają tych samych słów (np.{" "}
          <i>problemy z siecią</i> znajdzie notatkę o VPN).
        </p>
        <p className="muted">
          Używa lokalnego modelu {info.model}. Model pobiera się jednorazowo (ok. {info.model_mb} MB), potem działa bez
          internetu, a treść Notatek nie opuszcza komputera.
        </p>
        <button className="primary" onClick={onEnable}>
          Włącz i pobierz model
        </button>
      </div>
    );
  if (info.error) return <div className="banner bad">{info.error}</div>;
  if (info.running && info.phase) {
    // Wczytywanie modelu nie zgłasza postępu: pasek „w toku” zamiast procentów.
    const loading = !info.model_ready || !info.total;
    return (
      <div className="pad">
        <div className="progress">
          {loading ? (
            <div className="progress-bar indeterminate" />
          ) : (
            <div className="progress-bar" style={{ width: `${(100 * info.done) / info.total}%` }} />
          )}
          <span>{info.total && info.phase.startsWith("Liczenie") ? `Wektory: ${info.done} / ${info.total}` : info.phase}</span>
        </div>
        {info.model_ready && info.notes > 0 && <div className="muted small">Wyniki obejmują na razie {info.notes} Notatek.</div>}
      </div>
    );
  }
  return null;
}

export function SearchPanel({ active, activeNote, focusKey, onOpen, onError }: Props) {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<Mode>("words");
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [partial, setPartial] = useState(false);
  const [busy, setBusy] = useState(false);
  const [sem, setSem] = useState<SemanticInfo | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [focusKey]);

  useEffect(() => {
    api.semanticStatus().then(setSem).catch(() => {});
    const un = listen<Omit<SemanticInfo, "model" | "model_mb" | "enabled">>("semantic-status", (e) =>
      setSem((s) => (s ? { ...s, ...e.payload, enabled: true } : s)),
    );
    return () => {
      un.then((f) => f());
    };
  }, []);

  const semEnabled = !!sem?.enabled;
  useEffect(() => {
    if (active && semEnabled) api.semanticActivate().catch((e) => onError(String(e)));
  }, [active, semEnabled, onError]);

  // Zapytanie zamienia na wektor model, więc szukanie po znaczeniu czeka na jego wczytanie.
  const semReady = semEnabled && !!sem?.model_ready;

  useEffect(() => {
    const q = query.trim();
    if (!q) return setHits(null);
    if (mode === "meaning" && !semReady) return setHits(null);
    let alive = true;
    // Wektor zapytania liczy model, więc przy szukaniu po znaczeniu czekamy dłużej na koniec pisania.
    const t = window.setTimeout(
      () => {
        setBusy(true);
        const p =
          mode === "words"
            ? api.search(q).then((r) => alive && (setHits(r.hits), setPartial(r.partial)))
            : api.semanticSearch(q).then((r) => alive && (setHits(r), setPartial(false)));
        p.catch((e) => onError(String(e))).finally(() => alive && setBusy(false));
      },
      mode === "words" ? 150 : 400,
    );
    return () => {
      alive = false;
      window.clearTimeout(t);
    };
  }, [query, mode, semReady, onError]);

  const enable = () => {
    setSem((s) => (s ? { ...s, enabled: true, running: true, phase: "Przygotowanie…" } : s));
    api.semanticEnable().catch((e) => onError(String(e)));
  };

  return (
    <div className="tree">
      <div className="search-top">
        <label className="search-field">
          <SearchIcon size={14} />
          <input
            ref={input}
            placeholder={mode === "words" ? "Szukaj w treści…" : "Opisz, czego szukasz…"}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && hits?.[0]) onOpen(hits[0].path);
              if (e.key === "Escape") setQuery("");
            }}
          />
          {query && (
            <button className="icon-btn tiny" onClick={() => setQuery("")} aria-label="Wyczyść">
              <XIcon size={12} />
            </button>
          )}
        </label>
      </div>
      <div className="tabs search-modes">
        <button className={`tab${mode === "words" ? " on" : ""}`} onClick={() => setMode("words")}>
          Słowa
        </button>
        <button className={`tab${mode === "meaning" ? " on" : ""}`} onClick={() => setMode("meaning")}>
          Znaczenie
        </button>
        {busy && <span className="muted small">szukam…</span>}
      </div>
      <div className="tree-list">
        {mode === "meaning" && <SemanticState info={sem} onEnable={enable} />}
        {!hits && mode === "words" && (
          <div className="muted pad search-help">
            Szuka w tytułach, Folderach, Tagach i treści. Wielkość liter i polskie znaki nie mają znaczenia, a słowa
            pasują też jako początek wyrazu (<i>serwis</i> znajdzie <i>serwisu</i>).
            <br />
            <code>"dwa słowa"</code> szuka frazy, <code>#tag</code> zawęża do Tagu.
          </div>
        )}
        {!hits && mode === "meaning" && semReady && (
          <div className="muted pad search-help">
            Opisz własnymi słowami, czego szukasz. Wyniki są posortowane od najbardziej podobnych; to nie musi być
            dokładne dopasowanie.
          </div>
        )}
        {hits && hits.length === 0 && <div className="muted pad">Nic nie znaleziono.</div>}
        {partial && hits && hits.length > 0 && (
          <div className="muted pad">Żadna Notatka nie ma wszystkich słów. Pokazuję te z częścią słów.</div>
        )}
        {hits?.map((h) => (
          <div
            key={h.path}
            className={`search-hit${activeNote === h.path ? " active" : ""}`}
            onClick={() => onOpen(h.path)}
            title={h.path}
          >
            <div className="search-title">{h.title}</div>
            {h.folder && <div className="search-folder">{h.folder}</div>}
            <div className="search-snippet">
              <Highlighted text={h.snippet} />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
