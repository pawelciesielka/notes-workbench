import { useMemo, useState, type ReactNode } from "react";
import type { TreeNode } from "../lib/api";
import { loadViewed } from "../lib/recent";
import { ago } from "./Tree";
import { ClockIcon, EyeIcon, FileIcon, PencilIcon } from "./Icons";

type Filter = "all" | "viewed" | "edited";
type Row = { path: string; title: string; folder: string; viewed: number | null; modified: number | null; at: number; edited: boolean };

const LIMIT = 40;

/** Wszystkie Notatki z drzewa (z Notatkami folderów): ścieżka → tytuł, Folder, data modyfikacji. */
function flatten(nodes: TreeNode[], parent: string[], out: Map<string, { title: string; folder: string; modified: number | null }>) {
  for (const n of nodes) {
    if (n.kind === "note") out.set(n.path, { title: n.title, folder: parent.join(" / "), modified: n.modified });
    else {
      if (n.folder_note) out.set(n.folder_note, { title: n.title, folder: parent.join(" / "), modified: n.modified });
      flatten(n.children, [...parent, n.title], out);
    }
  }
  return out;
}

/** Ekran startowy: ostatnio otwierane i ostatnio zmienione Notatki. */
export function RecentView({
  vault,
  tree,
  onOpen,
  children,
}: {
  vault: string;
  tree: TreeNode[];
  onOpen: (path: string) => void;
  /** Pod listą (np. skróty klawiszowe). */
  children?: ReactNode;
}) {
  const [filter, setFilter] = useState<Filter>("all");
  const rows = useMemo(() => {
    const notes = flatten(tree, [], new Map());
    const viewed = new Map(loadViewed(vault).map((v) => [v.path, v.at]));
    const edited = [...notes.entries()]
      .filter(([, n]) => n.modified)
      .sort((a, b) => b[1].modified! - a[1].modified!)
      .slice(0, LIMIT)
      .map(([p]) => p);
    const paths = new Set([...viewed.keys(), ...edited]);
    const out: Row[] = [];
    for (const path of paths) {
      const n = notes.get(path);
      if (!n) continue; // usunięta albo w Koszu
      const v = viewed.get(path) ?? null;
      const m = n.modified;
      out.push({ path, title: n.title, folder: n.folder, viewed: v, modified: m, at: Math.max(v ?? 0, m ?? 0), edited: (m ?? 0) > (v ?? 0) });
    }
    return out.sort((a, b) => b.at - a.at);
  }, [vault, tree]);

  const shown = rows
    .filter((r) => (filter === "viewed" ? r.viewed : filter === "edited" ? r.modified : true))
    .sort((a, b) => (filter === "viewed" ? b.viewed! - a.viewed! : filter === "edited" ? b.modified! - a.modified! : 0))
    .slice(0, LIMIT);
  const now = Date.now();

  return (
    <div className="recent">
      <div className="recent-head">
        <h2>
          <ClockIcon size={18} /> Ostatnie Notatki
        </h2>
        <div className="seg" role="tablist">
          {(
            [
              ["all", "Wszystkie"],
              ["viewed", "Otwierane"],
              ["edited", "Zmieniane"],
            ] as const
          ).map(([k, label]) => (
            <button key={k} role="tab" aria-selected={filter === k} className={filter === k ? "on" : ""} onClick={() => setFilter(k)}>
              {label}
            </button>
          ))}
        </div>
      </div>
      {shown.length === 0 ? (
        <div className="recent-none muted">
          {filter === "viewed" ? "Nie otwierano jeszcze żadnej Notatki." : "Brak Notatek."} Otwórz Notatkę z drzewa po lewej albo utwórz nową.
        </div>
      ) : (
        <ul className="recent-list">
          {shown.map((r) => {
            const edited = filter === "edited" || (filter === "all" && r.edited);
            const at = filter === "viewed" ? r.viewed! : filter === "edited" ? r.modified! : r.at;
            return (
              <li key={r.path}>
                <button onClick={() => onOpen(r.path)} title={r.path}>
                  <span className="recent-icon">
                    <FileIcon size={15} />
                  </span>
                  <span className="recent-main">
                    <span className="recent-title">{r.title}</span>
                    {r.folder && <span className="recent-folder">{r.folder}</span>}
                  </span>
                  <span className="recent-when" title={edited ? "Ostatnia zmiana pliku" : "Ostatnio otwarta"}>
                    {edited ? <PencilIcon size={13} /> : <EyeIcon size={13} />}
                    {ago(at, now)}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {children}
    </div>
  );
}
