/**
 * Przykładowe dane do podglądu interfejsu w zwykłej przeglądarce (`npm run dev` poza Tauri).
 * Ładowane tylko w trybie deweloperskim; w aplikacji wszystkie wywołania idą do backendu.
 */
const H = 3_600_000;
const ago = (h: number) => Date.now() - h * H;

const tree = [
  {
    name: "Przykłady", path: "Przykłady", kind: "folder", title: "Przykłady", order: 1, folder_note: "Przykłady/Przykłady.md", modified: ago(20*24),
    children: [
      {
        name: "Monitoring", path: "Przykłady/Monitoring", kind: "folder", title: "Monitoring", order: 1, folder_note: null, modified: ago(3*24*365),
        children: [
          { name: "graphite", path: "Przykłady/Monitoring/graphite.md", kind: "note", title: "graphite", order: 1, folder_note: null, modified: ago(30*24), children: [] },
          { name: "grafana", path: "Przykłady/Monitoring/grafana.md", kind: "note", title: "Grafana – dashboardy", order: 2, folder_note: null, modified: null, children: [] },
        ],
      },
      { name: "Router", path: "Przykłady/Router.md", kind: "note", title: "Router domowy", order: 2, folder_note: null, modified: ago(2), children: [] },
      { name: "Sieć", path: "Przykłady/Sieć.md", kind: "note", title: "Sieć domowa", order: 3, folder_note: null, modified: ago(5*24), children: [] },
    ],
  },
  {
    name: "Dziennik", path: "Dziennik", kind: "folder", title: "Dziennik", order: 2, folder_note: null, modified: ago(26),
    children: [
      { name: "2024-03-15", path: "Dziennik/2024-03-15.md", kind: "note", title: "Notatki z 15-03-2024", order: 1, folder_note: null, modified: ago(40*24), children: [] },
    ],
  },
  { name: "systemd", path: "systemd.md", kind: "note", title: "Usługa systemd – przykład", order: 3, folder_note: null, modified: ago(400*24), children: [] },
  { name: "Pomysły", path: "Pomysły.md", kind: "note", title: "Pomysły", order: 4, folder_note: null, modified: null, children: [] },
];

type MockNode = { name: string; path: string; kind: string; title: string; order: number | null; folder_note: string | null; modified: number | null; children: MockNode[] };

/** Zawartość Folderu `folder` (`""` to korzeń) w podglądowym drzewie. */
function levelOf(folder: string): MockNode[] {
  if (!folder) return tree as MockNode[];
  let list = tree as MockNode[];
  const parts = folder.split("/");
  for (let i = 0; i < parts.length; i++) {
    const p = parts.slice(0, i + 1).join("/");
    const f = list.find((n) => n.path === p);
    if (!f) throw new Error(`Podgląd: nie ma Folderu ${p}`);
    list = f.children;
  }
  return list;
}

/** Nowy węzeł na końcu Folderu (jak w backendzie); zwraca jego ścieżkę. */
function add(folder: string, base: string, kind: "note" | "folder"): string {
  const list = levelOf(folder);
  let name = base;
  for (let i = 2; list.some((n) => n.name === name); i++) name = `${base} ${i}`;
  const path = `${folder ? `${folder}/` : ""}${name}${kind === "note" ? ".md" : ""}`;
  list.push({ name, path, kind, title: name, order: null, folder_note: null, modified: Date.now(), children: [] });
  return path;
}

const body = `Krótka instrukcja wdrożenia aplikacji jako **usługi systemd** w Ubuntu. Zobacz też [[Router domowy]] i [[Nieistniejąca notatka]]. #linux #admin

## Plik usługi

Utwórz plik \`/etc/systemd/system/app.service\`:

\`\`\`
[Unit]
Description=Moja aplikacja
After=network.target

[Service]
ExecStart=/usr/bin/python3 /opt/app/main.py
Restart=always
\`\`\`

> Po każdej zmianie pliku uruchom \`systemctl daemon-reload\`.

- [x] utworzyć użytkownika
- [ ] dodać logrotate

| Polecenie | Opis |
| --- | --- |
| \`systemctl start app\` | uruchamia usługę |
| \`journalctl -u app\` | logi |
`;

const notes = [
  { path: "systemd.md", title: "Usługa systemd – przykład", stem: "systemd" },
  { path: "Przykłady/Router.md", title: "Router domowy", stem: "Router" },
  { path: "Przykłady/Monitoring/graphite.md", title: "graphite", stem: "graphite" },
];

const mcp = {
  command: "C:\\Users\\Jan\\.notes-workbench\\bin\\notes-workbench-mcp.exe",
  args: ["--mcp", "--vault", "C:\\Notatki"],
  vault: "C:\\Notatki",
  clients: [
    { id: "claude-desktop", name: "Claude Desktop", detected: true, connected: true, config: "C:\\Users\\Jan\\AppData\\Roaming\\Claude\\claude_desktop_config.json", after: "Zamknij Claude całkowicie (ikona w zasobniku → Quit) i uruchom ponownie." },
    { id: "claude-code", name: "Claude Code", detected: true, connected: false, config: "C:\\Users\\Jan\\.claude.json", after: "Działa w nowych sesjach Claude Code." },
    { id: "codex", name: "Codex", detected: false, connected: false, config: "C:\\Users\\Jan\\.codex\\config.toml", after: "Działa w nowych sesjach Codex." },
  ],
  snippet: '{\n  "notes-workbench": {\n    "command": "C:\\\\Users\\\\Jan\\\\.notes-workbench\\\\bin\\\\notes-workbench-mcp.exe",\n    "args": ["--mcp", "--vault", "C:\\\\Notatki"]\n  }\n}',
};

const handlers: Record<string, (a: Record<string, unknown>) => unknown> = {
  mcp_status: () => mcp,
  save_note: (a) => ({ path: a.path, title: "Usługa systemd – przykład", frontmatter: "", body: a.body, raw: a.body }),
  add_attachment: (a) => {
    const name = String(a.name);
    return { name, link: `systemd.assets/${encodeURI(name)}`, image: /\.(png|jpe?g|gif|webp)$/i.test(name) };
  },
  get_vault: () => "C:\\Notatki",
  set_vault: () => null,
  vault_tree: (a) => (a.trash ? [] : structuredClone(tree)),
  vault_index: () => ({
    notes,
    tags: [
      { tag: "linux", count: 12 },
      { tag: "admin", count: 7 },
      { tag: "dziennik", count: 23 },
    ],
    noai: ["Przykłady/Sieć.md"],
  }),
  list_proposals: () => [
    {
      id: 1, kind: "relation", note: "systemd.md", target: "Przykłady/Monitoring/graphite.md",
      reason: "Obie Notatki dotyczą konfiguracji usług na tym samym serwerze Ubuntu.", source: "Claude (MCP)",
      created: "2026-11-09 22:16", status: "pending", note_title: "Usługa systemd – przykład", target_title: "graphite",
    },
    {
      id: 2, kind: "tag", note: "Przykłady/Router.md", tag: "sieć", reason: "Notatka o sieci domowej.",
      source: "Claude (MCP)", created: "2026-11-09 22:17", status: "pending", note_title: "Router domowy", target_title: null,
    },
    {
      id: 3, kind: "note", note: "Przykłady", title: "Mapa przykładów", body: "# Mapa przykładów\n\nZbiorcza Notatka z Linkami:\n\n- [[Router domowy]]\n- [[graphite]]\n\n#przyklady",
      reason: "Brakuje Notatki, która łączy przykłady.", source: "Claude (MCP)", created: "2026-11-10 12:01",
      status: "pending", note_title: "Przykłady", target_title: null,
    },
    {
      id: 4, kind: "edit", note: "Przykłady/Router.md",
      base: "Konfiguracja routera.\n\nSerwer: router.example.com\nPort: 1194\n\nUwagi: brak",
      body: "Konfiguracja routera.\n\nSerwer: router.example.com\nPort: 443 (TCP)\n\nUwagi: od 2025 port 443.\nZobacz też [[systemd]].",
      reason: "Port zmieniony na 443 według nowszej Notatki.", source: "Claude (MCP)", created: "2026-11-10 12:02",
      status: "pending", note_title: "Router domowy", target_title: null,
    },
    {
      id: 5, kind: "folder", note: "Przykłady", title: "Sieć", body: "Notatki o sieci: router, Wi-Fi, DNS.",
      reason: "Notatki o sieci są rozrzucone po Przykładach.", source: "Claude (MCP)", created: "2026-11-10 12:03",
      status: "pending", note_title: "Przykłady", target_title: null,
    },
    {
      id: 6, kind: "rename", note: "Dziennik", title: "Dziennik 2024",
      reason: "Wszystkie Notatki w Folderze są z 2024 roku.", source: "Claude (MCP)", created: "2026-11-10 12:04",
      status: "pending", note_title: "Dziennik", target_title: null,
    },
  ],
  accept_proposals: (a) => ({ done: (a.ids as number[]).length - 1, created: [], failed: [[(a.ids as number[])[0], "Notatka „Router domowy” zmieniła się od czasu Propozycji, więc jej nie nadpisuję."]] }),
  reject_proposals: () => null,
  read_note: (a) => ({
    path: a.path,
    title: notes.find((n) => n.path === a.path)?.title ?? String(a.path).split("/").pop()?.replace(/\.md$/, ""),
    frontmatter: 'title: "x"\n',
    body,
    raw: body,
  }),
  note_links: () => {
    const find = (s: string) => body.indexOf(s);
    return {
      spans: [
        { start: find("[[Router"), end: find("[[Router") + "[[Router domowy]]".length, target: "Router domowy", label: "Router domowy", kind: "link", path: "Przykłady/Router.md" },
        { start: find("[[Nie"), end: find("[[Nie") + "[[Nieistniejąca notatka]]".length, target: "Nieistniejąca notatka", label: "Nieistniejąca notatka", kind: "link", path: null },
        { start: find("#linux"), end: find("#linux") + 6, target: "linux", label: "#linux", kind: "tag", path: null },
        { start: find("#admin"), end: find("#admin") + 6, target: "admin", label: "#admin", kind: "tag", path: null },
      ],
      tags: ["linux", "admin"],
      related: [{ path: "Przykłady/Monitoring/graphite.md", title: "graphite", stem: "graphite" }],
      backlinks: [{ path: "Przykłady/Router.md", title: "Router domowy", lines: ["Najpierw zestaw usługę: [[systemd]]"], related: false }],
    };
  },
  similar_notes: () => [{ path: "Przykłady/Monitoring/grafana.md", title: "Grafana – dashboardy", stem: "" }],
  semantic_activate: () => undefined,
  move_note: (a) => ({ path: `${a.folder ? `${a.folder}/` : ""}${String(a.path).split("/").pop()}`, fixed: 0 }),
  set_position: (a) => {
    const path = String(a.path);
    const list = levelOf(path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");
    const i = list.findIndex((n) => n.path === path);
    if (i < 0) throw new Error(`Podgląd: nie ma ${path}`);
    const [item] = list.splice(i, 1);
    const at = a.before == null ? list.length : list.findIndex((n) => n.path === a.before);
    list.splice(at < 0 ? list.length : at, 0, item);
    return undefined;
  },
  create_note: (a) => add(String(a.folder), "Nowa Notatka", "note"),
  create_folder: (a) => add(String(a.parent), String(a.name || "Nowy Folder"), "folder"),
  move_folder: (a) => ({ path: `${a.parent ? `${a.parent}/` : ""}${String(a.path).split("/").pop()}`, fixed: 0 }),
  semantic_status: () => ({ enabled: false, model_ready: false, running: false, phase: "", done: 0, total: 0, notes: 0, error: null, model: "multilingual-e5-small", model_mb: 470 }),
  search_notes: () => ({
    partial: false,
    hits: [
      { path: "systemd.md", title: "Usługa systemd – przykład", folder: "", snippet: "…wdrożenia aplikacji jako \u0001usługi\u0002 systemd w \u0001Ubuntu\u0002…" },
      { path: "Przykłady/Router.md", title: "Router domowy", folder: "Przykłady", snippet: "Konfiguracja na \u0001Ubuntu\u0002 20.04…" },
    ],
  }),
  graph: () => ({
    nodes: [...notes.map((n) => ({ id: n.path, label: n.title, kind: "note" })), { id: "#linux", label: "#linux", kind: "tag" }],
    edges: [
      ["systemd.md", "Przykłady/Router.md"],
      ["systemd.md", "#linux"],
      ["Przykłady/Monitoring/graphite.md", "#linux"],
    ],
  }),
};

const today = new Date().toISOString().slice(0, 10);
const briefMd = `# Brief spotkań — wtorek, 17 listopada 2026

**4 spotkania.** Brief wygenerowany o 7:02. Najważniejszy punkt dnia to **15:00 Spotkanie w biurze**, które nie ma agendy.

Założenia: sprawdziłem tylko kalendarz główny.

---

## 08:00–09:00 (60 min) · Alab Doktor — odbyło się
Google Meet: https://meet.google.com/xxx-yyyy-zzz

**Uczestnicy:** Maciej (COO) — zaakceptował; Artur (ALAB) — zaakceptował.

**Agenda:** brak agendy w zaproszeniu. Prawdopodobnie: zakres produktu.

**Do zrobienia po spotkaniu:** spisać ustalenia i następny krok.

---

## 10:00–10:30 (30 min) · BioStand-up — odbyło się (udział opcjonalny)
Google Meet: https://meet.google.com/crr

**Agenda:** co zrobione wczoraj, plan na dziś, blokery. **Kontekst:** brak maili.

---

## 15:00–16:30 (90 min) · Spotkanie w biurze (projekt call center) — PRZED TOBĄ
Biuro (Srebrna 16) / Google Meet: https://meet.google.com/ibt

**Uczestnicy:** Maciej (organizator) — zaakceptował; Paweł D. — **odrzucił**.

**Agenda:** brak agendy w zaproszeniu.

**Do przygotowania:**
- Dopytać Macieja przed 15:00, jaki jest cel spotkania.
- Odpowiedzieć na zaproszenie.

---

## 23:00–23:30 (30 min) · Późny call — PRZED TOBĄ
Teams

**Agenda:** przegląd tygodnia.

---

## Konflikty i uwagi
- **Nakładanie się rano:** Alab Doktor kolidował z daily.
- **Brak Twoich odpowiedzi** na zaproszenia: call center.
`;
let briefCfg = { dir: String.raw`C:\Dane\Notatki\Dziennik\Daily_brief` as string | null, autostart: false };
let briefSeen = false;
let briefTrig = { url: null as string | null, has_token: false, last_fire: null as number | null, last_session: null as string | null };
Object.assign(handlers, {
  app_mode: () => (new URLSearchParams(location.search).has("brief") ? "brief" : "main"),
  brief_config: () => briefCfg,
  set_brief_dir: (a: Record<string, unknown>) => {
    briefCfg = { ...briefCfg, dir: (a.dir as string | null) ?? null };
    return null;
  },
  set_brief_autostart: (a: Record<string, unknown>) => {
    briefCfg = { ...briefCfg, autostart: !!a.on };
    return briefCfg.autostart;
  },
  brief_today: () =>
    briefCfg.dir
      ? { name: `Brief spotkań ${today}.md`, file: `${briefCfg.dir}\\Brief spotkań ${today}.md`, note: `Dziennik/Daily_brief/Brief spotkań ${today}.md`, content: briefMd, modified: Date.now() - 3 * H }
      : null,
  brief_trigger: () => briefTrig,
  set_brief_trigger: (a: Record<string, unknown>) => {
    const url = (a.url as string | null) ?? null;
    briefTrig = { ...briefTrig, url, has_token: url ? briefTrig.has_token || !!a.token : false };
    return briefTrig;
  },
  brief_update: () => {
    if (!briefTrig.url || !briefTrig.has_token) throw new Error("Najpierw podaj Fire URL i token rutyny w ustawieniach Daily brief");
    briefTrig = { ...briefTrig, last_fire: Date.now(), last_session: "https://claude.ai/code/session_mock" };
    return briefTrig;
  },
  brief_seen: () => briefSeen,
  brief_mark_seen: () => {
    briefSeen = true;
    return null;
  },
  brief_open_main: () => null,
  brief_open_file: () => null,
  brief_close: () => null,
});

// ===== Jarvis =====
const jvDay = (n: number) => {
  const d = new Date();
  d.setDate(d.getDate() + n);
  return d.toLocaleDateString("sv-SE");
};
const jvToday = jvDay(0);
const jvLines = [
  `- [ ] Oferta dla ABC !1 #oferta #abc @${jvToday}`,
  `- [ ] Przegląd maili #outlook`,
  `- [ ] Raport kwartalny !2 #raport @${jvDay(-2)}`,
  `- [ ] Spotkanie z zespołem #teams @${jvDay(3)}`,
  `- [x] Faktury !2 #faktura @${jvToday}`,
];
let jvFocus: string | null = "Oferta dla ABC";
let jvInfo = {
  dir: String.raw`C:\WORK\jarvis`,
  dir_setting: null as string | null,
  python: String.raw`C:\Python\python.exe`,
  python_setting: null as string | null,
  running: true,
  autostart: false,
};
let jvCfg = {
  guard: { enabled: true, days: [0, 1, 2, 3, 4], hours: ["08:00", "18:00"], warn_minutes: 5, overlay_minutes: 10 },
  tasks_file: String.raw`C:\Dane\Notatki\Dziennik\Daily_brief\Zadania\zadania.md`,
};
type JvTask = { raw: string; done: boolean; name: string; prio: number | null; date: string | null; tags: string[] };
const jvParse = (raw: string): JvTask | null => {
  const m = /^\s*[-*]\s*\[( |x|X)\]\s*(.+?)\s*$/.exec(raw);
  if (!m) return null;
  const text = m[2];
  const p = /(?:^|\s)!([1-3])\b/.exec(text);
  return {
    raw,
    done: m[1] !== " ",
    name: text.replace(/\s*(?:[@#][\w\-.+]+|![1-3]\b)/gu, "").trim(),
    prio: p ? Number(p[1]) : null,
    date: /@(\d{4}-\d{2}-\d{2})/.exec(text)?.[1] ?? null,
    tags: [...text.matchAll(/#([\w\-.+]+)/gu)].map((x) => x[1]),
  };
};
const jvLine = (t: JvTask) =>
  `- [${t.done ? "x" : " "}] ${t.name}${t.prio ? ` !${t.prio}` : ""}${t.date ? ` @${t.date}` : ""}${t.tags.map((g) => ` #${g}`).join("")}`;
const jvTasks = () => jvLines.map(jvParse).filter((t): t is JvTask => !!t);
Object.assign(handlers, {
  jarvis_info: () => jvInfo,
  set_jarvis_paths: (a: Record<string, unknown>) => {
    jvInfo = { ...jvInfo, dir_setting: (a.dir as string) ?? null, python_setting: (a.python as string) ?? null };
    return null;
  },
  jarvis_tasks: () => ({ file: jvCfg.tasks_file, tasks: jvTasks(), focus: jvFocus, today: jvToday }),
  jarvis_status: (a: Record<string, unknown>) => ({
    day: (a.day as string) ?? jvToday,
    tracker: { running: jvInfo.running, pid: 1234 },
    autostart: jvInfo.autostart,
    focus: jvFocus,
    tasks_file: jvCfg.tasks_file,
    active: 6 * 3600 + 1240,
    idle: 3100,
    first: `${jvToday}T08:12:00`,
    last: `${jvToday}T16:40:00`,
    switches: 412,
    categories: [
      { name: "Dokumenty/Biuro", secs: 9800 },
      { name: "Komunikacja", secs: 6200 },
      { name: "Programowanie/IT", secs: 4100 },
      { name: "Inne", secs: 1300 },
      { name: "Rozrywka/Social", secs: 640 },
    ],
    colors: { "Dokumenty/Biuro": "#e8a33d", Komunikacja: "#16a37f", "Programowanie/IT": "#d9534f", Inne: "#9aa5b1", "Rozrywka/Social": "#4f7cff" },
    tasks: jvTasks().map((t, i) => ({ ...t, when: "dziś", prio: t.prio ?? 3, secs: [5400, 1800, 0, 0, 2400][i] ?? 0, worked: i < 2 })),
    guard: {
      drifts: 2,
      focus_changes: 3,
      backs: 1,
      breaks: 1,
      events: [
        { t: `${jvToday}T08:15:02`, ev: "focus", task: "Oferta dla ABC", reason: "pick", how: "list" },
        { t: `${jvToday}T10:41:10`, ev: "overlay", reason: "drift" },
        { t: `${jvToday}T10:41:40`, ev: "back", reason: "drift", task: "Oferta dla ABC" },
        { t: `${jvToday}T13:05:00`, ev: "break", reason: "drift" },
        { t: `${jvToday}T14:20:00`, ev: "done", task: "Faktury", ok: true },
      ],
    },
  }),
  jarvis_add_task: (a: Record<string, unknown>) => {
    jvLines.push(`- [ ] ${a.text}`);
    return jvParse(jvLines[jvLines.length - 1]);
  },
  jarvis_update_task: (a: Record<string, unknown>) => {
    const i = jvLines.indexOf(a.raw as string);
    if (i < 0) throw new Error("Rejestr zadań zmienił się w międzyczasie.");
    jvLines[i] = jvLine(a.task as JvTask);
    return jvParse(jvLines[i]);
  },
  jarvis_set_focus: (a: Record<string, unknown>) => {
    jvFocus = a.name as string;
    return null;
  },
  jarvis_control: (a: Record<string, unknown>) => {
    if (a.action === "start" || a.action === "restart") jvInfo = { ...jvInfo, running: true };
    if (a.action === "stop") jvInfo = { ...jvInfo, running: false };
    if (a.action === "install") jvInfo = { ...jvInfo, autostart: true };
    if (a.action === "uninstall") jvInfo = { ...jvInfo, autostart: false };
    return "";
  },
  jarvis_config: () => jvCfg,
  jarvis_set_config: (a: Record<string, unknown>) => {
    jvCfg = a.config as typeof jvCfg;
    return jvInfo.running;
  },
  jarvis_report: () => null,
});

export async function mockInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const h = handlers[cmd];
  if (!h) throw new Error(`Podgląd: brak danych dla ${cmd}`);
  return h(args) as T;
}
