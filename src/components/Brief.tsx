import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { api, type Brief, type BriefConfig, type BriefTrigger } from "../lib/api";
import { isAhead, isNow, parseBrief, untilLabel, type BriefMeeting } from "../lib/brief";
import { JarvisStrip } from "./JarvisView";
import { CalendarIcon, ChevronRight, ExternalIcon, FileIcon, FolderOpenAction, RefreshIcon, XIcon } from "./Icons";

/** Markdown briefu; odnośniki otwierają się w przeglądarce, nie w oknie aplikacji. */
function Md({ children, onError }: { children: string; onError: (m: string) => void }) {
  return (
    <Markdown
      remarkPlugins={[remarkGfm]}
      components={{
        a: ({ href, children: c }) => (
          <a
            href={href}
            title={href}
            onClick={(e) => {
              e.preventDefault();
              if (href) api.openLink(href).catch((err) => onError(String(err)));
            }}
          >
            {c}
          </a>
        ),
      }}
    >
      {children}
    </Markdown>
  );
}

/** Odświeżanie „teraz” co minutę (status spotkań, „za 2 h”). */
function useNow(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = window.setInterval(() => setNow(new Date()), 60_000);
    return () => window.clearInterval(t);
  }, []);
  return now;
}

function Meeting({ m, now, onError }: { m: BriefMeeting; now: Date; onError: (s: string) => void }) {
  const ahead = isAhead(m, now);
  const live = isNow(m, now);
  const [open, setOpen] = useState(false);
  const short = m.where?.replace(/https?:\/\/\S+/g, "").replace(/[:/\s]+$/, "").trim();
  return (
    <li className={`brief-meeting${ahead ? " ahead" : " past"}${live ? " live" : ""}${open ? " open" : ""}`}>
      <button className="brief-meeting-head" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <span className="brief-time">
          {m.start}
          <span className="brief-time-end">{m.end}</span>
        </span>
        <span className="brief-meeting-main">
          <span className="brief-meeting-title">{m.title}</span>
          <span className="brief-meeting-meta">
            {live ? (
              <span className="pill live-pill">trwa</span>
            ) : ahead ? (
              <span className="pill ahead-pill">{untilLabel(m.start, now)}</span>
            ) : (
              <span className="pill muted-pill">odbyło się</span>
            )}
            {m.duration && <span>{m.duration}</span>}
            {short && <span className="brief-where">{short}</span>}
          </span>
        </span>
        <ChevronRight size={14} className="brief-chevron" />
      </button>
      {ahead && !open && (m.agenda || m.todo.length > 0) && (
        <div className="brief-meeting-sum markdown">
          {m.agenda && (
            <div className="brief-agenda">
              <b>Agenda:</b> <Md onError={onError}>{m.agenda}</Md>
            </div>
          )}
          {m.todo.length > 0 && (
            <>
              <div className="brief-todo-label">{m.todoLabel}</div>
              <ul>
                {m.todo.map((t, i) => (
                  <li key={i}>
                    <Md onError={onError}>{t}</Md>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
      {open && (
        <div className="brief-meeting-sum markdown">
          <Md onError={onError}>{m.body}</Md>
        </div>
      )}
    </li>
  );
}

/** Podsumowanie dzisiejszego briefu: wstęp, oś dnia ze spotkaniami (nadchodzące rozwinięte) i uwagi. */
export function BriefSummary({ brief, onError }: { brief: Brief; onError: (m: string) => void }) {
  const now = useNow();
  const p = useMemo(() => parseBrief(brief.content), [brief.content]);
  const meetings = p.sections.filter((s): s is BriefMeeting => s.kind === "meeting");
  const others = p.sections.filter((s) => s.kind === "other");
  const ahead = meetings.filter((m) => isAhead(m, now));
  const next = ahead.find((m) => !isNow(m, now));
  const [showExtra, setShowExtra] = useState(false);

  return (
    <div className="brief">
      <div className="brief-title">
        <CalendarIcon size={20} />
        <h1>{p.title || brief.name.replace(/\.md$/i, "")}</h1>
      </div>
      {meetings.length > 0 && (
        <div className="brief-stats">
          <span>
            <b>{meetings.length}</b> {meetings.length === 1 ? "spotkanie" : meetings.length < 5 ? "spotkania" : "spotkań"}
          </span>
          <span>
            <b>{ahead.length}</b> przed Tobą
          </span>
          {next && (
            <span className="brief-next">
              Następne: <b>{next.start}</b> {next.title} ({untilLabel(next.start, now)})
            </span>
          )}
        </div>
      )}
      {p.lead && (
        <div className="brief-lead markdown">
          <Md onError={onError}>{p.lead}</Md>
        </div>
      )}
      {p.extra.length > 0 && (
        <div className="brief-extra">
          <button className="ghost small" onClick={() => setShowExtra((s) => !s)}>
            {showExtra ? "Ukryj założenia" : "Założenia briefu"}
          </button>
          {showExtra && (
            <div className="markdown muted">
              <Md onError={onError}>{p.extra.join("\n\n")}</Md>
            </div>
          )}
        </div>
      )}
      {meetings.length > 0 && (
        <ul className="brief-meetings">
          {meetings.map((m, i) => (
            <Meeting key={i} m={m} now={now} onError={onError} />
          ))}
        </ul>
      )}
      {others.map((s, i) => (
        <section key={i} className="brief-other markdown">
          <h3>{s.title}</h3>
          <Md onError={onError}>{s.body}</Md>
        </section>
      ))}
    </div>
  );
}

/** Przyciski pod briefem: plik w Windows, Notatka w aplikacji. */
function BriefLinks({ brief, onOpenNote, onError }: { brief: Brief; onOpenNote?: (p: string) => void; onError: (m: string) => void }) {
  return (
    <>
      {brief.note && onOpenNote && (
        <button onClick={() => onOpenNote(brief.note!)} title={brief.note}>
          <FileIcon size={15} /> Otwórz jako Notatkę
        </button>
      )}
      <button onClick={() => api.briefOpenFile(brief.file).catch((e) => onError(String(e)))} title={brief.file}>
        <ExternalIcon size={15} /> Otwórz plik
      </button>
    </>
  );
}

/** Brief jako okno nad aplikacją (pierwsze otwarcie aplikacji danego dnia). */
export function BriefDialog(props: { brief: Brief; onClose: () => void; onOpenNote: (p: string) => void; onError: (m: string) => void }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [props]);
  return (
    <div className="modal-back" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div className="modal brief-modal" role="dialog" aria-label="Daily brief">
        <button className="icon-btn brief-x" onClick={props.onClose} aria-label="Zamknij" title="Zamknij (Esc)">
          <XIcon size={15} />
        </button>
        <div className="brief-scroll">
          <BriefSummary brief={props.brief} onError={props.onError} />
        </div>
        <div className="actions">
          <BriefLinks
            brief={props.brief}
            onError={props.onError}
            onOpenNote={(p) => {
              props.onClose();
              props.onOpenNote(p);
            }}
          />
          <button className="primary" onClick={props.onClose}>
            Zamknij
          </button>
        </div>
      </div>
    </div>
  );
}

/** Samodzielne okno uruchomione z Harmonogramu zadań (logowanie albo odblokowanie komputera). */
export function BriefWindow({ onOpenApp }: { onOpenApp: () => void }) {
  const [brief, setBrief] = useState<Brief | null | undefined>(undefined);
  const [error, setError] = useState("");
  useEffect(() => {
    api
      .briefToday()
      .then((b) => {
        setBrief(b);
        if (b) api.briefMarkSeen().catch(() => {});
      })
      .catch((e) => setError(String(e)));
  }, []);
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && api.briefClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);
  return (
    <div className="brief-window">
      <div className="brief-scroll">
        {error && <div className="banner bad">{error}</div>}
        {brief === null && <div className="muted">Nie ma dziś briefu.</div>}
        {brief && <BriefSummary brief={brief} onError={setError} />}
      </div>
      <div className="actions brief-window-actions">
        {brief && <BriefLinks brief={brief} onError={setError} />}
        <button onClick={onOpenApp}>Otwórz Notes Workbench</button>
        <button className="primary" onClick={() => api.briefClose()} autoFocus>
          Zamknij
        </button>
      </div>
    </div>
  );
}

/** Widok Daily brief w aplikacji: ustawienia i dzisiejsze podsumowanie. */
export function BriefView({
  onOpenNote,
  onError,
  onOpenJarvis,
  embedded = false,
}: {
  onOpenNote: (p: string) => void;
  onError: (m: string) => void;
  onOpenJarvis?: () => void;
  /** Zakładka w widoku Jarvisa: bez własnego nagłówka (tytuł i zakładki daje Jarvis). */
  embedded?: boolean;
}) {
  const [cfg, setCfg] = useState<BriefConfig | null>(null);
  const [brief, setBrief] = useState<Brief | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.briefConfig().then(setCfg).catch((e) => onError(String(e)));
    api
      .briefToday()
      .then(setBrief)
      .catch((e) => {
        setBrief(null);
        onError(String(e));
      });
  }, [onError]);
  useEffect(load, [load]);

  const pickDir = async () => {
    const p = await open({ directory: true, title: "Wskaż Folder z plikami Daily brief", defaultPath: cfg?.dir ?? undefined });
    if (typeof p !== "string") return;
    try {
      await api.setBriefDir(p);
      load();
    } catch (e) {
      onError(String(e));
    }
  };

  const toggleAuto = async (on: boolean) => {
    setBusy(true);
    try {
      const now = await api.setBriefAutostart(on);
      setCfg((c) => (c ? { ...c, autostart: now } : c));
    } catch (e) {
      onError(`Harmonogram zadań: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const today = new Date().toLocaleDateString("sv-SE");

  return (
    <div className={embedded ? "brief-embedded" : "note"}>
      {embedded ? (
        <p className="muted mcp-lead brief-embedded-lead">
          Podsumowanie dnia z pliku Markdown, który przygotowuje AI (plik z datą <code>{today}</code> w nazwie).
        </p>
      ) : (
        <div className="note-head">
          <h1 className="note-title">Daily brief</h1>
          <p className="muted mcp-lead">
            Podsumowanie dnia z pliku Markdown, który przygotowuje AI. Aplikacja szuka w wybranym Folderze pliku z dzisiejszą datą
            w nazwie (<code>{today}</code>) i pokazuje go przy pierwszym wejściu na komputer danego dnia.
          </p>
        </div>
      )}
      <div className="mcp-clients">
        <div className="mcp-card brief-settings">
          <div className="mcp-body">
            <div className="mcp-name">Folder z briefami</div>
            <div className="mcp-config" title={cfg?.dir ?? ""}>
              {cfg ? (cfg.dir ?? "nie wybrano") : "…"}
            </div>
          </div>
          <div className="mcp-actions">
            {cfg?.dir && (
              <button
                onClick={() =>
                  api
                    .setBriefDir(null)
                    .then(load)
                    .catch((e) => onError(String(e)))
                }
              >
                Wyczyść
              </button>
            )}
            <button className={cfg?.dir ? "" : "primary"} onClick={pickDir}>
              <FolderOpenAction size={15} /> Wybierz…
            </button>
          </div>
        </div>
        <label className={`mcp-card brief-settings brief-check${cfg?.autostart ? " on" : ""}`}>
          <input
            type="checkbox"
            checked={cfg?.autostart ?? false}
            disabled={!cfg || busy}
            onChange={(e) => toggleAuto(e.target.checked)}
          />
          <div className="mcp-body">
            <div className="mcp-name">Pokazuj po zalogowaniu do Windows</div>
            <div className="muted small">
              Zadanie w Harmonogramie zadań uruchamia okno briefu przy logowaniu i odblokowaniu komputera, ale tylko pierwszy raz
              danego dnia i tylko wtedy, gdy jest już dzisiejszy plik.
            </div>
          </div>
        </label>
        <BriefTriggerCard onError={onError} />
      </div>
      <div className="mcp-section brief-today">
        <h2>Dzisiaj</h2>
        {onOpenJarvis && <JarvisStrip onOpen={onOpenJarvis} />}
        {brief === undefined && <div className="muted">Sprawdzanie…</div>}
        {brief === null && (
          <div className="muted">{cfg?.dir ? `W Folderze nie ma jeszcze pliku z datą ${today} w nazwie.` : "Najpierw wybierz Folder z briefami."}</div>
        )}
        {brief && (
          <div className="brief-card">
            <BriefSummary brief={brief} onError={onError} />
          </div>
        )}
        {brief !== undefined && cfg?.dir && (
          <div className="actions">
            <BriefUpdateButton onError={onError} onBriefReady={setBrief} />
            {brief && <BriefLinks brief={brief} onOpenNote={onOpenNote} onError={onError} />}
          </div>
        )}
      </div>
    </div>
  );
}

/** Wejście do aplikacji: okno briefu z Harmonogramu albo pełna aplikacja. */
export function BriefGate({ app }: { app: ReactNode }) {
  const [mode, setMode] = useState<"brief" | "main" | undefined>(undefined);
  useEffect(() => {
    api
      .appMode()
      .then(setMode)
      .catch(() => setMode("main"));
  }, []);
  if (mode === undefined) return null;
  if (mode === "brief")
    return (
      <BriefWindow
        onOpenApp={() =>
          api
            .briefOpenMain()
            .catch(() => {})
            .finally(() => setMode("main"))
        }
      />
    );
  return <>{app}</>;
}

// ===== Aktualizacja na żądanie (rutyna Claude w chmurze, docs/jarvis.md) =====

/** Rutyna zwykle kończy w kilka minut; przez ten czas kolejne kliknięcie pyta o potwierdzenie. */
const UPDATE_PENDING_MS = 15 * 60 * 1000;

const hhmm = (ms: number) => new Date(ms).toLocaleTimeString("pl-PL", { hour: "2-digit", minute: "2-digit" });

/** Jarvis importuje inbox/ sam co 5 min, ale tylko gdy tracker działa; po aktualizacji briefu robimy to od razu. */
function syncJarvis() {
  api.jarvisControl("sync").catch(() => {});
}

/**
 * Przycisk „Aktualizuj brief”: uruchamia rutynę, która dopisuje zmiany od poprzedniego briefu.
 * Potem co minutę (do 30 min) sprawdza, czy w Folderze jest już nowy plik; gdy jest, importuje zadania do Jarvisa
 * (i jeszcze raz po 2 min, bo plik w inbox/ może dojść z Dysku Google później niż brief).
 */
export function BriefUpdateButton({
  onError,
  onBriefReady,
  className,
}: {
  onError: (m: string) => void;
  onBriefReady?: (b: Brief) => void;
  className?: string;
}) {
  const [trig, setTrig] = useState<BriefTrigger | null>(null);
  const [busy, setBusy] = useState(false);
  const [waitingSince, setWaitingSince] = useState<number | null>(null);
  useEffect(() => {
    api
      .briefTrigger()
      .then(setTrig)
      .catch(() => setTrig(null));
  }, []);
  useEffect(() => {
    if (!waitingSince) return;
    const id = window.setInterval(async () => {
      if (Date.now() - waitingSince > 30 * 60 * 1000) {
        setWaitingSince(null);
        return;
      }
      const b = await api.briefToday().catch(() => null);
      if (b?.modified && b.modified >= waitingSince) {
        setWaitingSince(null);
        syncJarvis();
        // Drugi import ma się odbyć także po opuszczeniu widoku, więc tego timeoutu nie czyścimy.
        window.setTimeout(syncJarvis, 2 * 60 * 1000);
        onBriefReady?.(b);
      }
    }, 60 * 1000);
    return () => window.clearInterval(id);
  }, [waitingSince, onBriefReady]);
  const ready = !!trig?.url && trig.has_token;
  const pending = !!trig?.last_fire && Date.now() - trig.last_fire < UPDATE_PENDING_MS;

  const fire = async () => {
    // Ustawienia mogły się zmienić w innym miejscu (karta w Daily brief): czytamy je od nowa.
    const cur = await api.briefTrigger().catch(() => trig);
    setTrig(cur);
    if (!cur?.url || !cur.has_token) {
      onError("Aktualizacja briefu: najpierw podaj Fire URL i token rutyny w ustawieniach Daily brief.");
      return;
    }
    if (
      cur.last_fire &&
      Date.now() - cur.last_fire < UPDATE_PENDING_MS &&
      !(await ask(`Aktualizację zlecono o ${hhmm(cur.last_fire)} i może jeszcze trwać. Zlecić kolejną?`, {
        title: "Aktualizuj brief",
        kind: "warning",
      }))
    )
      return;
    setBusy(true);
    try {
      const t = await api.briefUpdate();
      setTrig(t);
      setWaitingSince(t.last_fire);
    } catch (e) {
      onError(`Aktualizacja briefu: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <button
        className={className}
        disabled={busy}
        onClick={fire}
        title={
          ready
            ? "Uruchamia rutynę Claude, która dopisze do dzisiejszego briefu zmiany w kalendarzu, nowe maile i zadania"
            : "Wymaga Fire URL i tokenu rutyny (ustawienia Daily brief)"
        }
      >
        <RefreshIcon size={14} /> {busy ? "Zlecanie…" : "Aktualizuj brief"}
      </button>
      {pending && (
        <span className="muted small brief-pending">
          zlecono o {hhmm(trig!.last_fire!)}
          {trig!.last_session && (
            <>
              {" · "}
              <a
                href="#"
                onClick={(e) => {
                  e.preventDefault();
                  api.openLink(trig!.last_session!).catch((er) => onError(String(er)));
                }}
              >
                sesja
              </a>
            </>
          )}
        </span>
      )}
    </>
  );
}

/** Ustawienia wyzwalacza: Fire URL i token z claude.ai/code/routines (Edytuj → Trigger → API). */
function BriefTriggerCard({ onError, onChange }: { onError: (m: string) => void; onChange?: () => void }) {
  const [trig, setTrig] = useState<BriefTrigger | null>(null);
  const [edit, setEdit] = useState(false);
  const [url, setUrl] = useState("");
  const [token, setToken] = useState("");
  useEffect(() => {
    api
      .briefTrigger()
      .then(setTrig)
      .catch((e) => onError(String(e)));
  }, [onError]);

  const save = async (u: string | null, t: string | null) => {
    try {
      setTrig(await api.setBriefTrigger(u, t));
      setEdit(false);
      setToken("");
      onChange?.();
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="mcp-card brief-settings brief-trigger">
      <div className="mcp-body">
        <div className="mcp-name">Aktualizacja na żądanie</div>
        {!edit && (
          <div className="mcp-config" title={trig?.url ?? ""}>
            {trig?.url ? `${trig.url.replace(/^.*\/routines\//, "").replace(/\/fire$/, "")} · ${trig.has_token ? "token zapisany" : "brak tokenu"}` : "nie ustawiono"}
          </div>
        )}
        {edit && (
          <div className="brief-trigger-form">
            <div className="muted small">
              W claude.ai/code/routines otwórz rutynę, która pisze brief, wybierz Edytuj → Trigger → <b>+</b> → API, potem Generate token.
            </div>
            <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="Fire URL: https://api.anthropic.com/v1/claude_code/routines/trig_…/fire" />
            <input
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder={trig?.has_token ? "Token (puste = bez zmian)" : "Token: sk-ant-oat01-…"}
              autoComplete="off"
            />
          </div>
        )}
      </div>
      <div className="mcp-actions">
        {edit ? (
          <>
            <button onClick={() => setEdit(false)}>Anuluj</button>
            <button className="primary" disabled={!url.trim() || (!trig?.has_token && !token.trim())} onClick={() => save(url, token || null)}>
              Zapisz
            </button>
          </>
        ) : (
          <>
            {trig?.url && <button onClick={() => save(null, null)}>Wyczyść</button>}
            <button
              className={trig?.url ? "" : "primary"}
              onClick={() => {
                setUrl(trig?.url ?? "");
                setToken("");
                setEdit(true);
              }}
            >
              {trig?.url ? "Zmień…" : "Ustaw…"}
            </button>
          </>
        )}
      </div>
    </div>
  );
}
