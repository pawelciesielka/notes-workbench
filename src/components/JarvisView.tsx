/**
 * Jarvis: zadania, podsumowanie dnia i konfiguracja lokalnego trackera (C:\WORK\jarvis\jarvis.py).
 * Opis integracji: docs/jarvis.md.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  type JarvisConfig,
  type JarvisGuardEvent,
  type JarvisInfo,
  type JarvisStatus,
  type JarvisTask,
  type JarvisTasks,
} from "../lib/api";
import { CalendarIcon, CheckIcon, ChevronRight, ExternalIcon, FolderOpenAction, PencilIcon, PlusIcon, TargetIcon, XIcon } from "./Icons";

type Tab = "tasks" | "day" | "settings";
type OnError = (m: string) => void;

// ===== Pomocnicze =====

export function fmtSecs(s: number): string {
  s = Math.round(s);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h) return `${h} h ${String(m).padStart(2, "0")} min`;
  if (m) return `${m} min`;
  return s ? `${s} s` : "0";
}

function addDays(iso: string, n: number): string {
  const d = new Date(`${iso}T12:00:00`);
  d.setDate(d.getDate() + n);
  return d.toLocaleDateString("sv-SE");
}

const DAY_SHORT = ["nd", "pn", "wt", "śr", "cz", "pt", "sb"];

export function dayLabel(date: string, today: string): string {
  if (date === today) return "dziś";
  if (date === addDays(today, 1)) return "jutro";
  if (date === addDays(today, -1)) return "wczoraj";
  const d = new Date(`${date}T12:00:00`);
  return `${DAY_SHORT[d.getDay()]} ${d.getDate()}.${String(d.getMonth() + 1).padStart(2, "0")}${
    date.slice(0, 4) !== today.slice(0, 4) ? `.${date.slice(0, 4)}` : ""
  }`;
}

/** Czy zadanie może być fokusem: Jarvis bierze tylko otwarte zadania widoczne dziś (dziś, zaległe, bez terminu). */
export function canFocus(t: JarvisTask, today: string): boolean {
  return !t.done && (t.date === null || t.date <= today);
}

/** Otwarte zadania na dziś (z zaległymi i bez terminu) – do paska w Daily brief. */
export function openToday(tasks: JarvisTask[], today: string): JarvisTask[] {
  return tasks.filter((t) => canFocus(t, today));
}

const byPrio = (a: JarvisTask, b: JarvisTask) => (a.prio ?? 3) - (b.prio ?? 3);

function Prio({ prio, onClick }: { prio: number | null; onClick?: () => void }) {
  const p = prio ?? 3;
  return (
    <button
      className={`jv-prio p${p}${prio === null ? " implicit" : ""}`}
      onClick={onClick}
      disabled={!onClick}
      title={onClick ? `Priorytet ${p}${prio === null ? " (domyślny)" : ""}. Kliknij, by zmienić.` : `Priorytet ${p}`}
    >
      !{p}
    </button>
  );
}

// ===== Widok =====

export function JarvisView({ onError, onOpenBrief }: { onError: OnError; onOpenBrief?: () => void }) {
  const [tab, setTab] = useState<Tab>("tasks");
  const [info, setInfo] = useState<JarvisInfo | null>(null);
  const loadInfo = useCallback(() => api.jarvisInfo().then(setInfo).catch((e) => onError(String(e))), [onError]);
  useEffect(() => {
    loadInfo();
  }, [loadInfo]);

  const missing = info && (!info.dir || !info.python);
  return (
    <div className="note jarvis">
      <div className="note-head">
        <h1 className="note-title">
          Jarvis
          {info && (
            <span className={`pill${info.running ? " ok" : " muted-pill"}`} title={info.dir ?? ""}>
              {info.running ? "tracker działa" : "tracker zatrzymany"}
            </span>
          )}
        </h1>
        <p className="muted mcp-lead">
          Jedno zadanie w fokusie i strażnik, który przypomina o nim, gdy uwaga odpływa. Zadania są w rejestrze na Dysku Google,
          czas i decyzje strażnika zapisuje Jarvis na tym komputerze.
        </p>
      </div>
      <div className="tabs jv-tabs">
        <button className={`tab${tab === "tasks" ? " on" : ""}`} onClick={() => setTab("tasks")}>
          Zadania
        </button>
        <button className={`tab${tab === "day" ? " on" : ""}`} onClick={() => setTab("day")}>
          Dzień
        </button>
        <button className={`tab${tab === "settings" ? " on" : ""}`} onClick={() => setTab("settings")}>
          Konfiguruj Jarvisa
        </button>
        {onOpenBrief && (
          <button className="tab" onClick={onOpenBrief} title="Daily brief: podsumowanie dnia i ustawienia">
            <CalendarIcon size={14} /> Daily brief
          </button>
        )}
      </div>
      {missing && tab !== "settings" && (
        <div className="mcp-section">
          <div className="banner">
            {!info.dir ? "Nie znaleziono Jarvisa (jarvis.py). " : "Nie znaleziono Pythona (python.exe). "}
            <button className="ghost" onClick={() => setTab("settings")}>
              Wskaż w „Konfiguruj Jarvisa” <ChevronRight size={14} />
            </button>
          </div>
        </div>
      )}
      {tab === "tasks" && info?.dir && <TasksTab onError={onError} />}
      {tab === "day" && info?.dir && info.python && <DayTab onError={onError} />}
      {tab === "settings" && info && <SettingsTab info={info} reloadInfo={loadInfo} onError={onError} />}
    </div>
  );
}

// ===== Zadania =====

function TasksTab({ onError }: { onError: OnError }) {
  const [data, setData] = useState<JarvisTasks | null>(null);
  const [secs, setSecs] = useState<Map<string, number>>(new Map());
  const [showDone, setShowDone] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.jarvisTasks().then(setData).catch((e) => onError(String(e)));
    // Czas dziś liczy Jarvis; bez niego lista i tak działa.
    api
      .jarvisStatus()
      .then((s) => setSecs(new Map(s.tasks.map((t) => [t.raw, t.secs]))))
      .catch(() => setSecs(new Map()));
  }, [onError]);
  useEffect(load, [load]);
  // Jarvis co 5 min dopisuje zadania z porannego briefu, a fokus zmienia też okno strażnika.
  useEffect(() => {
    const onFocus = () => load();
    window.addEventListener("focus", onFocus);
    const t = window.setInterval(load, 60_000);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.clearInterval(t);
    };
  }, [load]);

  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
      load();
    }
  };
  const update = (t: JarvisTask, patch: Partial<JarvisTask>) => run(() => api.jarvisUpdateTask(t.raw, { ...t, ...patch }));
  const setFocus = (t: JarvisTask) => run(() => api.jarvisSetFocus(t.name));

  if (!data) return <div className="mcp-section muted">Wczytywanie…</div>;
  const { today, tasks, focus } = data;
  const open = tasks.filter((t) => !t.done);
  const groups: [string, JarvisTask[]][] = [
    ["Zaległe", open.filter((t) => t.date && t.date < today).sort(byPrio)],
    ["Dziś", open.filter((t) => t.date === today).sort(byPrio)],
    ["Bez terminu", open.filter((t) => t.date === null).sort(byPrio)],
    ["Zaplanowane", open.filter((t) => t.date && t.date > today).sort((a, b) => a.date!.localeCompare(b.date!) || byPrio(a, b))],
  ];
  const weekAgo = addDays(today, -7);
  const done = tasks.filter((t) => t.done).reverse();
  const recentDone = done.filter((t) => !t.date || t.date >= weekAgo);
  const shownDone = showDone ? done : recentDone;
  const focusTask = focus ? open.find((t) => t.name.toLowerCase() === focus.toLowerCase()) : undefined;

  const row = (t: JarvisTask) =>
    editing === t.raw ? (
      <TaskEditor
        key={t.raw}
        task={t}
        onCancel={() => setEditing(null)}
        onSave={(n) => {
          setEditing(null);
          update(t, n);
        }}
      />
    ) : (
      <TaskRow
        key={t.raw}
        task={t}
        today={today}
        secs={secs.get(t.raw) ?? 0}
        isFocus={!!focus && t.name.toLowerCase() === focus.toLowerCase() && !t.done}
        busy={busy}
        onToggle={() => update(t, { done: !t.done })}
        onPrio={() => update(t, { prio: ((t.prio ?? 3) % 3) + 1 })}
        onFocus={() => setFocus(t)}
        onEdit={() => setEditing(t.raw)}
      />
    );

  return (
    <>
      <div className="mcp-section">
        <div className={`jv-focus${focus ? " on" : ""}`}>
          <TargetIcon size={22} />
          <div className="jv-focus-body">
            <div className="jv-focus-label">W fokusie</div>
            {focus ? (
              <div className="jv-focus-name">
                {focusTask?.name ?? focus}
                {focusTask && secs.get(focusTask.raw) ? <span className="muted"> · dziś {fmtSecs(secs.get(focusTask.raw)!)}</span> : null}
              </div>
            ) : (
              <div className="jv-focus-name muted">Nic. Wybierz jedno zadanie, od którego zaczynasz (ikona celu przy zadaniu).</div>
            )}
          </div>
          {focusTask && (
            <button disabled={busy} onClick={() => update(focusTask, { done: true })} title="Oznacz zadanie w fokusie jako zrobione">
              <CheckIcon size={15} /> Zrobione
            </button>
          )}
        </div>
      </div>
      <AddTask today={today} busy={busy} onAdd={(text) => run(() => api.jarvisAddTask(text))} />
      {groups.map(
        ([title, list]) =>
          list.length > 0 && (
            <div className="mcp-section" key={title}>
              <h2>
                {title} <span className="jv-count">{list.length}</span>
              </h2>
              <div className="jv-list">{list.map(row)}</div>
            </div>
          ),
      )}
      {open.length === 0 && <div className="mcp-section muted">Brak otwartych zadań.</div>}
      {done.length > 0 && (
        <div className="mcp-section">
          <h2>
            Zrobione {showDone ? "" : "(ostatnie 7 dni)"} <span className="jv-count">{shownDone.length}</span>
            {done.length > recentDone.length && (
              <button className="ghost jv-h2-btn" onClick={() => setShowDone(!showDone)}>
                {showDone ? "tylko ostatnie" : `pokaż wszystkie (${done.length})`}
              </button>
            )}
          </h2>
          <div className="jv-list done">{shownDone.map(row)}</div>
        </div>
      )}
      <div className="mcp-section muted small jv-file" title={data.file}>
        Rejestr: <code>{data.file}</code>
      </div>
    </>
  );
}

function TaskRow(props: {
  task: JarvisTask;
  today: string;
  secs: number;
  isFocus: boolean;
  busy: boolean;
  onToggle: () => void;
  onPrio: () => void;
  onFocus: () => void;
  onEdit: () => void;
}) {
  const { task: t, today } = props;
  const late = !t.done && t.date && t.date < today;
  return (
    <div className={`jv-task${t.done ? " done" : ""}${props.isFocus ? " focus" : ""}`}>
      <input type="checkbox" checked={t.done} disabled={props.busy} onChange={props.onToggle} title={t.done ? "Przywróć" : "Zrobione"} />
      <Prio prio={t.prio} onClick={t.done || props.busy ? undefined : props.onPrio} />
      <div className="jv-task-main" onDoubleClick={props.onEdit}>
        <div className="jv-task-name">{t.name}</div>
        <div className="jv-task-meta">
          {t.date && <span className={late ? "jv-late" : ""}>{dayLabel(t.date, today)}</span>}
          {t.tags.map((g) => (
            <span key={g} className="jv-tag">
              #{g}
            </span>
          ))}
          {props.secs > 0 && <span className="jv-secs">dziś {fmtSecs(props.secs)}</span>}
        </div>
      </div>
      <div className="jv-task-actions">
        {canFocus(t, today) && !props.isFocus && (
          <button className="ghost icon-only" disabled={props.busy} onClick={props.onFocus} title="Ustaw jako zadanie w fokusie">
            <TargetIcon size={15} />
          </button>
        )}
        {props.isFocus && <span className="pill ok">fokus</span>}
        <button className="ghost icon-only" disabled={props.busy} onClick={props.onEdit} title="Edytuj nazwę, priorytet, datę i #słowa">
          <PencilIcon size={15} />
        </button>
      </div>
    </div>
  );
}

const tagsText = (tags: string[]) => tags.map((t) => `#${t}`).join(" ");
const parseTags = (s: string) =>
  s
    .split(/[\s,]+/)
    .map((t) => t.replace(/^#+/, ""))
    .filter(Boolean);

function TaskEditor({ task, onSave, onCancel }: { task: JarvisTask; onSave: (p: Partial<JarvisTask>) => void; onCancel: () => void }) {
  const [name, setName] = useState(task.name);
  const [prio, setPrio] = useState<number | null>(task.prio);
  const [date, setDate] = useState(task.date ?? "");
  const [tags, setTags] = useState(tagsText(task.tags));
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    onSave({ name: name.trim(), prio, date: date || null, tags: parseTags(tags) });
  };
  return (
    <form className="jv-task jv-edit" onSubmit={submit} onKeyDown={(e) => e.key === "Escape" && onCancel()}>
      <input className="jv-edit-name" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Nazwa zadania" />
      <div className="jv-edit-row">
        <label>
          Priorytet
          <select value={prio ?? ""} onChange={(e) => setPrio(e.target.value ? Number(e.target.value) : null)}>
            <option value="1">!1 najważniejsze</option>
            <option value="2">!2</option>
            <option value="3">!3</option>
            <option value="">brak (= 3)</option>
          </select>
        </label>
        <label>
          Dzień
          <input type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        </label>
        {date && (
          <button type="button" className="ghost" onClick={() => setDate("")} title="Bez terminu: przypominane codziennie aż do zrobienia">
            bez terminu
          </button>
        )}
        <label className="jv-edit-tags">
          #słowa
          <input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="#oferta #klient_abc" />
        </label>
      </div>
      <div className="jv-edit-row end">
        <span className="muted small">#słowa dopasowują zadanie do tytułów okien (np. #oferta → „Oferta_ABC.docx”).</span>
        <button type="button" onClick={onCancel}>
          Anuluj
        </button>
        <button type="submit" className="primary">
          Zapisz
        </button>
      </div>
    </form>
  );
}

function AddTask({ today, busy, onAdd }: { today: string; busy: boolean; onAdd: (text: string) => Promise<unknown> }) {
  const [text, setText] = useState("");
  const [date, setDate] = useState(today);
  const ref = useRef<HTMLInputElement>(null);
  const hasDate = /@\d{4}-\d{2}-\d{2}/.test(text);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const t = text.trim();
    if (!t || t.replace(/\s*(?:[@#][\w\-.+]+|![1-3]\b)/gu, "").trim() === "") return;
    await onAdd(hasDate || !date ? t : `${t} @${date}`);
    setText("");
    ref.current?.focus();
  };
  return (
    <form className="mcp-section jv-add" onSubmit={submit}>
      <input
        ref={ref}
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Nowe zadanie, np. Oferta dla ABC !1 #oferta #abc"
        disabled={busy}
      />
      <input
        type="date"
        value={hasDate ? "" : date}
        disabled={busy || hasDate}
        onChange={(e) => setDate(e.target.value)}
        title="Dzień zadania; puste = bez terminu (przypominane codziennie aż do zrobienia)"
      />
      <button type="submit" className="primary" disabled={busy || !text.trim()}>
        <PlusIcon size={15} /> Dodaj
      </button>
    </form>
  );
}

// ===== Dzień =====

const EVENT_LABEL: Record<string, string> = {
  focus: "Fokus",
  back: "Powrót do zadania",
  done: "Zrobione",
  break: "Przerwa",
  snooze: "Odłożone",
  overlay: "Okno strażnika",
  switch: "Zmiana zadania",
  new: "Nowe zadanie",
  pick: "Wybór zadania",
};
const REASON_LABEL: Record<string, string> = {
  drift: "odpłynięcie",
  pick: "brak fokusu",
  manual: "ręcznie",
  cli: "z aplikacji",
  demo: "test",
};

function eventText(e: JarvisGuardEvent): ReactNode {
  const label = EVENT_LABEL[e.ev] ?? e.ev;
  const extra = [e.reason && (REASON_LABEL[e.reason] ?? e.reason), e.via && (REASON_LABEL[String(e.via)] ?? String(e.via))].filter(Boolean);
  return (
    <>
      <b>{label}</b>
      {e.task ? `: ${e.task}` : ""}
      {extra.length > 0 && <span className="muted"> ({extra.join(", ")})</span>}
    </>
  );
}

function Bars({ rows, total, colors }: { rows: { name: string; secs: number }[]; total: number; colors?: Record<string, string> }) {
  return (
    <div className="jv-bars">
      {rows.map((r) => (
        <div className="jv-bar" key={r.name}>
          <div className="jv-bar-label" title={r.name}>
            {r.name}
          </div>
          <div className="jv-bar-track">
            <div
              className="jv-bar-fill"
              style={{ width: `${total ? Math.max(1.5, (100 * r.secs) / total) : 0}%`, background: colors?.[r.name] }}
            />
          </div>
          <div className="jv-bar-val">{fmtSecs(r.secs)}</div>
        </div>
      ))}
    </div>
  );
}

function DayTab({ onError }: { onError: OnError }) {
  const today = new Date().toLocaleDateString("sv-SE");
  const [day, setDay] = useState(today);
  const [st, setSt] = useState<JarvisStatus | null | undefined>(undefined);
  useEffect(() => {
    let gone = false;
    setSt(undefined);
    api
      .jarvisStatus(day)
      .then((s) => !gone && setSt(s))
      .catch((e) => {
        if (gone) return;
        setSt(null);
        onError(String(e));
      });
    return () => {
      gone = true;
    };
  }, [day, onError]);

  const tasks = useMemo(() => (st ? [...st.tasks].filter((t) => t.secs > 0).sort((a, b) => b.secs - a.secs) : []), [st]);
  const time = (iso: string | null) => (iso ? iso.slice(11, 16) : "–");

  return (
    <>
      <div className="mcp-section jv-daybar">
        <button className="ghost icon-only" onClick={() => setDay(addDays(day, -1))} title="Poprzedni dzień">
          <ChevronRight size={15} style={{ transform: "rotate(180deg)" }} />
        </button>
        <input type="date" value={day} max={today} onChange={(e) => e.target.value && setDay(e.target.value)} />
        <button className="ghost icon-only" disabled={day >= today} onClick={() => setDay(addDays(day, 1))} title="Następny dzień">
          <ChevronRight size={15} />
        </button>
        <span className="muted">{dayLabel(day, today)}</span>
        <span className="jv-spacer" />
        <button onClick={() => api.jarvisReport(day).catch((e) => onError(String(e)))} title="Raport HTML z historią przeglądarki (jarvis report)">
          <ExternalIcon size={15} /> Pełny raport
        </button>
      </div>
      {st === undefined && <div className="mcp-section muted">Liczenie…</div>}
      {st && (
        <>
          <div className="mcp-section">
            <div className="jv-stats">
              <Stat label="Aktywnie" value={fmtSecs(st.active)} hint={`${time(st.first)}–${time(st.last)}`} />
              <Stat label="Odpłynięcia" value={st.guard.drifts} hint="pełny ekran strażnika" warn={st.guard.drifts > 0} />
              <Stat label="Zmiany fokusu" value={st.guard.focus_changes} />
              <Stat label="Powroty" value={st.guard.backs} hint="do zadania po odpłynięciu" />
              <Stat label="Przerwy" value={st.guard.breaks} hint={`bezczynnie ${fmtSecs(st.idle)}`} />
              <Stat label="Przełączenia" value={st.switches} hint="między aplikacjami" />
            </div>
          </div>
          {st.active === 0 && <div className="mcp-section muted">Brak zapisanej aktywności tego dnia.</div>}
          {st.categories.length > 0 && (
            <div className="mcp-section">
              <h2>Kategorie</h2>
              <Bars rows={st.categories} total={st.active} colors={st.colors} />
            </div>
          )}
          <div className="mcp-section">
            <h2>Czas na zadaniach</h2>
            {tasks.length ? (
              <Bars rows={tasks.map((t) => ({ name: `${t.done ? "✓ " : ""}${t.name}`, secs: t.secs }))} total={Math.max(...tasks.map((t) => t.secs))} />
            ) : (
              <div className="muted">Żadne zadanie nie dopasowało się do okien. Dodaj zadaniom #słowa z nazw dokumentów albo stron.</div>
            )}
          </div>
          <div className="mcp-section">
            <h2>Strażnik</h2>
            {st.guard.events.length ? (
              <div className="jv-events">
                {st.guard.events.map((e, i) => (
                  <div className="jv-event" key={i}>
                    <span className="jv-event-time">{e.t.slice(11, 16)}</span>
                    <span>{eventText(e)}</span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="muted">Brak decyzji strażnika tego dnia.</div>
            )}
          </div>
        </>
      )}
    </>
  );
}

function Stat({ label, value, hint, warn }: { label: string; value: ReactNode; hint?: string; warn?: boolean }) {
  return (
    <div className={`jv-stat${warn ? " warn" : ""}`}>
      <div className="jv-stat-label">{label}</div>
      <div className="jv-stat-value">{value}</div>
      {hint && <div className="jv-stat-hint">{hint}</div>}
    </div>
  );
}

// ===== Konfiguracja =====

const DAYS = ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"];

function SettingsTab({ info, reloadInfo, onError }: { info: JarvisInfo; reloadInfo: () => Promise<unknown>; onError: OnError }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [cfg, setCfg] = useState<JarvisConfig | null>(null);
  const [saved, setSaved] = useState<JarvisConfig | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const loadCfg = useCallback(() => {
    if (!info.dir) return;
    api
      .jarvisConfig()
      .then((c) => {
        setCfg(c);
        setSaved(c);
      })
      .catch((e) => onError(String(e)));
  }, [info.dir, onError]);
  useEffect(loadCfg, [loadCfg]);

  const act = async (label: string, f: () => Promise<unknown>) => {
    setBusy(label);
    setMsg(null);
    try {
      await f();
    } catch (e) {
      onError(String(e));
    } finally {
      await reloadInfo();
      setBusy(null);
    }
  };

  const pickDir = async () => {
    const p = await open({ directory: true, title: "Katalog Jarvisa (z jarvis.py)", defaultPath: info.dir ?? undefined });
    if (typeof p === "string") act("paths", () => api.setJarvisPaths(p, info.python_setting));
  };
  const pickPython = async () => {
    const p = await open({ title: "Interpreter Pythona (python.exe)", filters: [{ name: "python.exe", extensions: ["exe"] }] });
    if (typeof p === "string") act("paths", () => api.setJarvisPaths(info.dir_setting, p));
  };
  const pickTasks = async () => {
    const p = await open({ title: "Rejestr zadań (zadania.md)", filters: [{ name: "Markdown", extensions: ["md"] }], defaultPath: cfg?.tasks_file });
    if (typeof p === "string" && cfg) setCfg({ ...cfg, tasks_file: p });
  };

  const dirty = !!cfg && JSON.stringify(cfg) !== JSON.stringify(saved);
  const save = () =>
    cfg &&
    act("save", async () => {
      const restarted = await api.jarvisSetConfig(cfg);
      setSaved(cfg);
      setMsg(restarted ? "Zapisano. Tracker uruchomiony ponownie z nowymi ustawieniami." : "Zapisano. Tracker nie działa: ustawienia wczyta przy starcie.");
    });
  const g = cfg?.guard;
  const setG = (patch: Partial<JarvisConfig["guard"]>) => cfg && setCfg({ ...cfg, guard: { ...cfg.guard, ...patch } });

  return (
    <>
      <div className="mcp-section">
        <h2>Tracker</h2>
        <div className="mcp-clients">
          <div className={`mcp-card${info.running ? " on" : ""}`}>
            <div className="mcp-body">
              <div className="mcp-name">
                {info.running ? "Działa w tle" : "Zatrzymany"}
                <span className={`pill${info.running ? " ok" : " muted-pill"}`}>{info.running ? "włączony" : "wyłączony"}</span>
              </div>
              <div className="muted small">Co 2 s zapisuje aktywne okno; w godzinach pracy pilnuje zadania w fokusie.</div>
            </div>
            <div className="mcp-actions">
              {info.running ? (
                <>
                  <button disabled={!!busy || !info.python} onClick={() => act("restart", () => api.jarvisControl("restart"))}>
                    {busy === "restart" ? "…" : "Uruchom ponownie"}
                  </button>
                  <button disabled={!!busy || !info.python} onClick={() => act("stop", () => api.jarvisControl("stop"))}>
                    {busy === "stop" ? "…" : "Zatrzymaj"}
                  </button>
                </>
              ) : (
                <button className="primary" disabled={!!busy || !info.dir || !info.python} onClick={() => act("start", () => api.jarvisControl("start"))}>
                  {busy === "start" ? "Uruchamianie…" : "Uruchom"}
                </button>
              )}
            </div>
          </div>
          <label className={`mcp-card brief-settings brief-check${info.autostart ? " on" : ""}`}>
            <input
              type="checkbox"
              checked={info.autostart}
              disabled={!!busy || !info.dir || !info.python}
              onChange={(e) => act("auto", () => api.jarvisControl(e.target.checked ? "install" : "uninstall"))}
            />
            <div className="mcp-body">
              <div className="mcp-name">Uruchamiaj z Windows</div>
              <div className="muted small">
                Skrót <code>Jarvis.lnk</code> w folderze Autostart: tracker startuje po zalogowaniu, w Twojej sesji. To nie jest usługa
                Windows, i celowo: usługi działają w osobnej sesji bez pulpitu (sesja 0), więc nie widzą aktywnego okna ani nie mogą
                pokazać powiadomień i okna strażnika.
              </div>
            </div>
          </label>
        </div>
      </div>

      {g && cfg && (
        <div className="mcp-section">
          <h2>Strażnik</h2>
          <div className="jv-form">
            <label className="jv-check">
              <input type="checkbox" checked={g.enabled} onChange={(e) => setG({ enabled: e.target.checked })} />
              Pilnuj zadania w fokusie (powiadomienie i pełny ekran po odpłynięciu)
            </label>
            <div className="jv-field">
              <span>Dni pracy</span>
              <div className="jv-days">
                {DAYS.map((d, i) => (
                  <button
                    key={d}
                    type="button"
                    className={`jv-day${g.days.includes(i) ? " on" : ""}`}
                    disabled={!g.enabled}
                    onClick={() => setG({ days: g.days.includes(i) ? g.days.filter((x) => x !== i) : [...g.days, i].sort() })}
                  >
                    {d}
                  </button>
                ))}
              </div>
            </div>
            <div className="jv-field">
              <span>Godziny</span>
              <div className="jv-inline">
                <input type="time" value={g.hours[0]} disabled={!g.enabled} onChange={(e) => setG({ hours: [e.target.value, g.hours[1]] })} />
                –
                <input type="time" value={g.hours[1]} disabled={!g.enabled} onChange={(e) => setG({ hours: [g.hours[0], e.target.value] })} />
              </div>
            </div>
            <div className="jv-field">
              <span>Powiadomienie po</span>
              <div className="jv-inline">
                <input
                  type="number"
                  min={1}
                  step={1}
                  value={g.warn_minutes}
                  disabled={!g.enabled}
                  onChange={(e) => setG({ warn_minutes: Number(e.target.value) })}
                />
                min poza zadaniem
              </div>
            </div>
            <div className="jv-field">
              <span>Pełny ekran po</span>
              <div className="jv-inline">
                <input
                  type="number"
                  min={1}
                  step={1}
                  value={g.overlay_minutes}
                  disabled={!g.enabled}
                  onChange={(e) => setG({ overlay_minutes: Number(e.target.value) })}
                />
                min poza zadaniem
              </div>
            </div>
            <div className="jv-field">
              <span>Rejestr zadań</span>
              <div className="jv-inline grow">
                <input value={cfg.tasks_file} onChange={(e) => setCfg({ ...cfg, tasks_file: e.target.value })} />
                <button type="button" onClick={pickTasks}>
                  <FolderOpenAction size={15} /> Wybierz…
                </button>
              </div>
            </div>
            <div className="jv-save">
              {msg && (
                <span className="jv-msg">
                  <CheckIcon size={14} /> {msg}
                </span>
              )}
              {dirty && (
                <button type="button" onClick={() => setCfg(saved)} disabled={!!busy}>
                  <XIcon size={14} /> Cofnij
                </button>
              )}
              <button type="button" className="primary" disabled={!dirty || !!busy} onClick={save}>
                {busy === "save" ? "Zapisywanie…" : info.running ? "Zapisz i uruchom ponownie" : "Zapisz"}
              </button>
            </div>
          </div>
          <p className="muted small">
            Pozostałe ustawienia (kategorie, przypomnienia, słowa spotkań) są w <code>{info.dir}\config.json</code>.
          </p>
        </div>
      )}

      <div className="mcp-section">
        <h2>Ścieżki</h2>
        <div className="mcp-clients">
          <div className="mcp-card brief-settings">
            <div className="mcp-body">
              <div className="mcp-name">Katalog Jarvisa</div>
              <div className="mcp-config" title={info.dir ?? ""}>
                {info.dir ?? "nie znaleziono jarvis.py"}
                {!info.dir_setting && info.dir ? " (domyślny)" : ""}
              </div>
            </div>
            <div className="mcp-actions">
              {info.dir_setting && (
                <button disabled={!!busy} onClick={() => act("paths", () => api.setJarvisPaths(null, info.python_setting))}>
                  Domyślny
                </button>
              )}
              <button className={info.dir ? "" : "primary"} disabled={!!busy} onClick={pickDir}>
                <FolderOpenAction size={15} /> Wybierz…
              </button>
            </div>
          </div>
          <div className="mcp-card brief-settings">
            <div className="mcp-body">
              <div className="mcp-name">Python</div>
              <div className="mcp-config" title={info.python ?? ""}>
                {info.python ?? "nie znaleziono python.exe"}
                {!info.python_setting && info.python ? " (wykryty)" : ""}
              </div>
            </div>
            <div className="mcp-actions">
              {info.python_setting && (
                <button disabled={!!busy} onClick={() => act("paths", () => api.setJarvisPaths(info.dir_setting, null))}>
                  Wykryj
                </button>
              )}
              <button className={info.python ? "" : "primary"} disabled={!!busy} onClick={pickPython}>
                <FolderOpenAction size={15} /> Wybierz…
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

// ===== Pasek w Daily brief =====

/** Krótko: fokus i liczba otwartych zadań na dziś; klik otwiera widok Jarvisa. Bez Jarvisa nic nie pokazuje. */
export function JarvisStrip({ onOpen }: { onOpen: () => void }) {
  const [data, setData] = useState<JarvisTasks | null>(null);
  useEffect(() => {
    api
      .jarvisTasks()
      .then(setData)
      .catch(() => setData(null));
  }, []);
  if (!data) return null;
  const n = openToday(data.tasks, data.today).length;
  return (
    <button className="jv-strip" onClick={onOpen} title="Otwórz zadania w Jarvisie">
      <TargetIcon size={16} />
      <span>
        Fokus: <b>{data.focus ?? "nie wybrano"}</b>
      </span>
      <span className="muted">· otwarte zadania na dziś: {n}</span>
      <ChevronRight size={14} />
    </button>
  );
}
