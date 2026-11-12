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
  vault_tree: (a) => (a.trash ? [] : tree),
  vault_index: () => ({
    notes,
    tags: [
      { tag: "linux", count: 12 },
      { tag: "admin", count: 7 },
      { tag: "dziennik", count: 23 },
    ],
    noai: ["Przykłady/Sieć.md"],
  }),
  read_report: () => ({ ok: true, errors: 0, warnings: 19, counts: {}, issues: [], source: "", vault: "", date: "", started: "", finished: "" }),
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

export async function mockInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const h = handlers[cmd];
  if (!h) throw new Error(`Podgląd: brak danych dla ${cmd}`);
  return h(args) as T;
}
