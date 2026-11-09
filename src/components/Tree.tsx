import { useMemo, useState } from "react";
import type { TreeNode } from "../lib/api";

function norm(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Drzewo z odfiltrowanymi węzłami: zostają pasujące i ich przodkowie. */
function filterTree(nodes: TreeNode[], q: string): TreeNode[] {
  if (!q) return nodes;
  const out: TreeNode[] = [];
  for (const n of nodes) {
    const kids = filterTree(n.children, q);
    if (norm(n.title).includes(q) || kids.length > 0) out.push({ ...n, children: kids });
  }
  return out;
}

type Props = {
  nodes: TreeNode[];
  selected: string | null;
  /** Notatki ukryte przed AI (oznaczane kłódką). */
  noai?: Set<string>;
  onOpen: (notePath: string) => void;
};

export function Tree({ nodes, selected, noai, onOpen }: Props) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const q = norm(query.trim());
  const visible = useMemo(() => filterTree(nodes, q), [nodes, q]);

  const toggle = (path: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const render = (list: TreeNode[], depth: number) =>
    list.map((n) => {
      const isOpen = q !== "" || expanded.has(n.path);
      const isFolder = n.kind === "folder";
      const active = selected !== null && (selected === n.path || selected === n.folder_note);
      return (
        <div key={n.path}>
          <div
            className={`tree-row${active ? " active" : ""}${isFolder ? " folder" : ""}`}
            style={{ paddingLeft: 8 + depth * 14 }}
            title={n.title}
            onClick={() => {
              if (isFolder) {
                toggle(n.path);
                if (n.folder_note) onOpen(n.folder_note);
              } else onOpen(n.path);
            }}
          >
            <span className="tree-icon">{isFolder ? (isOpen ? "▾" : "▸") : ""}</span>
            <span className="tree-label">{n.title}</span>
            {noai && (noai.has(n.path) || (n.folder_note !== null && noai.has(n.folder_note))) && (
              <span className="tree-lock" title="Ukryta przed AI (#noai)">
                🔒
              </span>
            )}
            {isFolder && n.folder_note && <span className="tree-badge" title="Folder ma Notatkę folderu">●</span>}
          </div>
          {isFolder && isOpen && render(n.children, depth + 1)}
        </div>
      );
    });

  return (
    <div className="tree">
      <input
        className="tree-filter"
        placeholder="Filtruj po tytule…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <div className="tree-list">
        {visible.length === 0 ? <div className="muted pad">Brak pasujących Notatek</div> : render(visible, 0)}
      </div>
    </div>
  );
}
