import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export type TreeNode = {
  name: string;
  path: string;
  kind: "folder" | "note";
  title: string;
  order: number | null;
  folder_note: string | null;
  children: TreeNode[];
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

export type Proposal = {
  id: number;
  kind: "relation" | "tag";
  note: string;
  target?: string;
  tag?: string;
  reason: string;
  source: string;
  created: string;
  status: "pending" | "rejected";
  note_title: string;
  target_title: string | null;
};

export const api = {
  proposals: () => invoke<Proposal[]>("list_proposals"),
  acceptProposals: (ids: number[]) => invoke<number>("accept_proposals", { ids }),
  rejectProposals: (ids: number[]) => invoke<void>("reject_proposals", { ids }),
  semanticStatus: () => invoke<SemanticInfo>("semantic_status"),
  semanticEnable: () => invoke<void>("semantic_enable"),
  semanticSearch: (query: string) => invoke<SemHit[]>("semantic_search", { query }),
  similarNotes: (path: string) => invoke<NoteRef[]>("similar_notes", { path }),
  search:(query: string) => invoke<SearchResults>("search_notes", { query }),
  index:(rebuild: boolean) => invoke<Summary>("vault_index", { rebuild }),
  noteLinks: (path: string) => invoke<NoteLinks>("note_links", { path }),
  resolveLink: (from: string, target: string) => invoke<string | null>("resolve_link", { from, target }),
  tagNotes: (tag: string) => invoke<NoteRef[]>("tag_notes", { tag }),
  graph: () => invoke<Graph>("graph"),
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
  readReport: () => invoke<Report | null>("read_report"),
  openVaultFile: (path: string) => invoke<void>("open_vault_file", { path }),
  openLink: (url: string) => invoke<void>("open_link", { url }),
};

/** Adres pliku Skarbca do wyświetlenia w widoku (obraz, PDF). */
export function vaultUrl(relPath: string): string {
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
