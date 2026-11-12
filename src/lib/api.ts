import { convertFileSrc, invoke as tauriInvoke } from "@tauri-apps/api/core";

/** Poza Tauri (podgląd interfejsu w przeglądarce, tylko tryb deweloperski) odpowiadają przykładowe dane. */
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri || !import.meta.env.DEV) return tauriInvoke<T>(cmd, args);
  return import("./mock").then((m) => m.mockInvoke<T>(cmd, args));
}

export type TreeNode = {
  name: string;
  path: string;
  kind: "folder" | "note";
  title: string;
  order: number | null;
  folder_note: string | null;
  /** Ostatnia modyfikacja pliku (ms od 1970); dla Folderu: jego Notatki folderu. */
  modified: number | null;
  children: TreeNode[];
};

export type Attachment = {
  /** Nazwa pliku w `<Notatka>.assets`. */
  name: string;
  /** Adres względem katalogu Notatki, gotowy do Markdownu. */
  link: string;
  image: boolean;
};

export type NoteData = {
  path: string;
  title: string;
  frontmatter: string;
  body: string;
  raw: string;
};

export type Issue = {
  level: "błąd" | "ostrzeżenie" | "zmiana";
  kind: string;
  note: string | null;
  message: string;
};

export type Report = {
  ok: boolean;
  source: string;
  vault: string;
  date: string;
  started: string;
  finished: string;
  errors: number;
  warnings: number;
  counts: Record<string, number>;
  issues: Issue[];
};

export type NoteRef = { path: string; title: string; stem: string };
export type TagCount = { tag: string; count: number };
/** `noai`: Notatki niedostępne dla AI (Tag noai na Notatce albo na Notatce folderu nad nią). */
export type Summary = { notes: NoteRef[]; tags: TagCount[]; noai: string[] };

/** Link albo Tag w treści; pozycje w znakach JavaScriptu. */
export type Span = {
  start: number;
  end: number;
  target: string;
  label: string;
  kind: "link" | "tag";
  path: string | null;
};

export type Backlink = { path: string; title: string; lines: string[]; related: boolean };
export type NoteLinks = { spans: Span[]; tags: string[]; related: NoteRef[]; backlinks: Backlink[] };
export type GraphNode = { id: string; label: string; kind: "note" | "tag" };
export type Graph = { nodes: GraphNode[]; edges: [string, string][] };

export type SearchHit = { path: string; title: string; folder: string; snippet: string };
export type SearchResults = { hits: SearchHit[]; partial: boolean };

export type SemanticInfo = {
  enabled: boolean;
  model_ready: boolean;
  running: boolean;
  phase: string;
  done: number;
  total: number;
  notes: number;
  error: string | null;
  model: string;
  model_mb: number;
};
export type SemHit = SearchHit & { score: number };

export type McpClient = {
  id: "claude-desktop" | "claude-code" | "codex";
  name: string;
  detected: boolean;
  connected: boolean;
  config: string | null;
  after: string;
};
export type McpStatus = { command: string; args: string[]; vault: string; clients: McpClient[]; snippet: string };

/** Wynik przeniesienia: nowa ścieżka i liczba Notatek z poprawionymi Linkami. */
export type OpResult = { path: string; fixed: number };

export type Proposal = {
  id: number;
  /** relation, tag: wpis we frontmatterze; note: nowa Notatka (`note` to Folder); edit: nowa treść Notatki;
   *  folder: nowy Folder (`note` to Folder nadrzędny, `title` nazwa, `body` Notatka folderu); rename: nowa nazwa Folderu `note`. */
  kind: "relation" | "tag" | "note" | "edit" | "folder" | "rename";
  note: string;
  target?: string;
  tag?: string;
  /** Tytuł nowej Notatki. */
  title?: string;
  /** Proponowana treść (nowa Notatka, zmiana). */
  body?: string;
  /** Treść Notatki w chwili Propozycji zmiany. */
  base?: string;
  reason: string;
  source: string;
  created: string;
  status: "pending" | "rejected";
  note_title: string;
  target_title: string | null;
};

export type AcceptResult = {
  done: number;
  /** Notatki utworzone z Propozycji. */
  created: string[];
  /** Propozycje, których nie dało się zastosować: [id, powód]. */
  failed: [number, string][];
};

export const api = {
  /** Znacznik do dziennika startu (start.log); błędy pomijamy, to tylko diagnostyka. */
  startMark: (label: string) => {
    invoke<void>("start_mark", { label }).catch(() => {});
  },
  proposals: () => invoke<Proposal[]>("list_proposals"),
  acceptProposals: (ids: number[]) => invoke<AcceptResult>("accept_proposals", { ids }),
  rejectProposals: (ids: number[]) => invoke<void>("reject_proposals", { ids }),
  semanticStatus: () => invoke<SemanticInfo>("semantic_status"),
  semanticEnable: () => invoke<void>("semantic_enable"),
  semanticActivate: () => invoke<void>("semantic_activate"),
  semanticSearch: (query: string) => invoke<SemHit[]>("semantic_search", { query }),
  similarNotes: (path: string) => invoke<NoteRef[]>("similar_notes", { path }),
  search:(query: string) => invoke<SearchResults>("search_notes", { query }),
  index:(rebuild: boolean) => invoke<Summary>("vault_index", { rebuild }),
  noteLinks: (path: string) => invoke<NoteLinks>("note_links", { path }),
  resolveLink: (from: string, target: string) => invoke<string | null>("resolve_link", { from, target }),
  tagNotes: (tag: string) => invoke<NoteRef[]>("tag_notes", { tag }),
  graph: () => invoke<Graph>("graph"),
  /** Zapisuje plik jako Załącznik Notatki; treść idzie surowo, bez JSON. */
  addAttachment: async (note: string, name: string, data: Blob): Promise<Attachment> => {
    const bytes = new Uint8Array(await data.arrayBuffer());
    if (!inTauri && import.meta.env.DEV) return invoke<Attachment>("add_attachment", { note, name, size: bytes.length });
    return tauriInvoke<Attachment>("add_attachment", bytes, {
      headers: { "x-note": encodeURIComponent(note), "x-name": encodeURIComponent(name) },
    });
  },
  saveNote: (path: string, body: string, expected: string) => invoke<NoteData>("save_note", { path, body, expected }),
  setNoai: (path: string, on: boolean) => invoke<NoteData>("set_noai", { path, on }),
  setTitle: (path: string, title: string) => invoke<NoteData>("set_note_title", { path, title }),
  createNote: (folder: string, title: string) => invoke<string>("create_note", { folder, title }),
  trashNote: (path: string) => invoke<string>("trash_note", { path }),
  restoreNote: (path: string) => invoke<string>("restore_note", { path }),
  getVault: () => invoke<string | null>("get_vault"),
  setVault: (path: string) => invoke<void>("set_vault", { path }),
  importAmno: (exportPath: string, vault: string) => invoke<Report>("import_amno", { export: exportPath, vault }),
  tree: (trash: boolean) => invoke<TreeNode[]>("vault_tree", { trash }),
  readNote: (path: string) => invoke<NoteData>("read_note", { path }),
  revealVault: () => invoke<void>("reveal_vault"),
  mcpStatus: () => invoke<McpStatus>("mcp_status"),
  mcpConnect: (client: string) => invoke<McpStatus>("mcp_connect", { client }),
  mcpDisconnect: (client: string) => invoke<McpStatus>("mcp_disconnect", { client }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  createFolder: (parent: string, name: string) => invoke<string>("create_folder", { parent, name }),
  renameFolder: (path: string, name: string) => invoke<OpResult>("rename_folder", { path, name }),
  moveNote: (path: string, folder: string) => invoke<OpResult>("move_note", { path, folder }),
  moveFolder: (path: string, parent: string) => invoke<OpResult>("move_folder", { path, parent }),
  /** Miejsce w Kolejności Folderu: przed `before`, `null` na końcu. */
  setPosition: (path: string, before: string | null) => invoke<void>("set_position", { path, before }),
  trashFolder: (path: string) => invoke<string>("trash_folder", { path }),
  restoreFolder: (path: string) => invoke<string>("restore_folder", { path }),
  setFolderNoai: (path: string, on: boolean) => invoke<string>("set_folder_noai", { path, on }),
  openVaultFile: (path: string) => invoke<void>("open_vault_file", { path }),
  openLink: (url: string) => invoke<void>("open_link", { url }),
};

/** Adres pliku Skarbca do wyświetlenia w widoku (obraz, PDF). */
export function vaultUrl(relPath: string): string {
  // Podgląd poza Tauri: brak protokołu vault://, obraz po prostu się nie wczyta.
  if (!inTauri) return `vault-preview/${relPath}`;
  return convertFileSrc(relPath, "vault");
}

/** Czy adres prowadzi poza Skarbiec (ma schemat, np. https:, file:, mailto:). */
export function isExternal(href: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(href);
}

/** Rozwiązuje adres względny z Notatki na ścieżkę w Skarbcu. */
export function resolveRelative(notePath: string, href: string): string {
  let decoded = href;
  try {
    decoded = decodeURIComponent(href);
  } catch {
    /* zostaje surowy */
  }
  const parts = notePath.split("/").slice(0, -1);
  for (const seg of decoded.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") parts.pop();
    else parts.push(seg);
  }
  return parts.join("/");
}

export function folderOf(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

/**
 * Zamienia Linki `[[…]]` i Tagi w treści na zwykłe odnośniki Markdown ze schematami
 * `wiki:` i `tag:`, żeby widok mógł je obsłużyć. Pozycje wyznacza backend (pomija kod i HTML).
 */
export function linkify(body: string, spans: Span[]): string {
  const esc = (s: string) => s.replace(/[\\[\]*_`<>]/g, (c) => "\\" + c);
  let out = "";
  let pos = 0;
  for (const s of spans) {
    if (s.start < pos) continue;
    out += body.slice(pos, s.start);
    const href =
      s.kind === "tag"
        ? `tag:${encodeURIComponent(s.target)}`
        : s.path
          ? `wiki:${encodeURIComponent(s.path)}`
          : `wiki-new:${encodeURIComponent(s.target)}`;
    out += `[${esc(s.label)}](${href})`;
    pos = s.end;
  }
  return out + body.slice(pos);
}

export function fileName(path: string): string {
  return path.split("/").pop() ?? path;
}
