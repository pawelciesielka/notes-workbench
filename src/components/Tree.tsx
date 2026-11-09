import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { TreeNode } from "../lib/api";
import { ChevronRight, CollapseIcon, FileIcon, FolderIcon, FolderOpenIcon, LockIcon, SearchIcon, XIcon } from "./Icons";

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

/** Liczba Notatek w Folderze (z podfolderami i Notatkami folderów). */
function countIn(n: TreeNode): number {
  return n.children.reduce((s, c) => s + (c.kind === "note" ? 1 : 0) + (c.folder_note ? 1 : 0) + countIn(c), 0);
}

/** Foldery nad Notatką (do rozwinięcia, gdy Notatkę otwarto spoza drzewa). */
function ancestors(path: string): string[] {
  const parts = path.split("/").slice(0, -1);
  return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}

type Row = { node: TreeNode; depth: number; open: boolean };

type Props = {
  nodes: TreeNode[];
  selected: string | null;
  /** Notatki ukryte przed AI (oznaczane kłódką). */
  noai?: Set<string>;
  onOpen: (notePath: string) => void;
  /** Prawy przycisk (albo klawisz Menu) na węźle; `null` to puste miejsce drzewa. */
  onMenu?: (node: TreeNode | null, x: number, y: number) => void;
};

export function Tree({ nodes, selected, noai, onOpen, onMenu }: Props) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [focus, setFocus] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const q = norm(query.trim());
  const visible = useMemo(() => filterTree(nodes, q), [nodes, q]);
  const counts = useMemo(() => {
    const m = new Map<string, number>();
    const walk = (list: TreeNode[]) =>
      list.forEach((n) => {
        if (n.kind === "folder") {
          m.set(n.path, countIn(n));
          walk(n.children);
        }
      });
    walk(nodes);
    return m;
  }, [nodes]);

  // Notatka otwarta z wyszukiwania, grafu albo Linku: rozwijamy drogę do niej i przewijamy.
  useEffect(() => {
    if (!selected) return;
    setExpanded((prev) => {
      const need = ancestors(selected).filter((a) => !prev.has(a));
      if (!need.length) return prev;
      const next = new Set(prev);
      need.forEach((a) => next.add(a));
      return next;
    });
    setFocus(null);
    const t = window.setTimeout(() => {
      listRef.current?.querySelector(".tree-row.active")?.scrollIntoView({ block: "nearest" });
    }, 30);
    return () => window.clearTimeout(t);
  }, [selected]);

  // Wiersze w kolejności wyświetlania (do nawigacji klawiaturą).
  const rows = useMemo(() => {
    const out: Row[] = [];
    const walk = (list: TreeNode[], depth: number) =>
      list.forEach((n) => {
        const open = n.kind === "folder" && (q !== "" || expanded.has(n.path));
        out.push({ node: n, depth, open });
        if (open) walk(n.children, depth + 1);
      });
    walk(visible, 0);
    return out;
  }, [visible, expanded, q]);

  const toggle = (path: string, open?: boolean) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      const want = open ?? !next.has(path);
      if (want) next.add(path);
      else next.delete(path);
      return next;
    });

  const activate = (n: TreeNode) => {
    if (n.kind === "folder") {
      toggle(n.path);
      if (n.folder_note) onOpen(n.folder_note);
    } else onOpen(n.path);
  };

  const isActive = (n: TreeNode) => selected !== null && (selected === n.path || selected === n.folder_note);

  const onKey = (e: KeyboardEvent) => {
    if (!rows.length) return;
    const cur = focus ?? rows.find((r) => isActive(r.node))?.node.path ?? null;
    let i = rows.findIndex((r) => r.node.path === cur);
    const r = rows[i];
    const go = (j: number) => {
      const n = rows[Math.max(0, Math.min(rows.length - 1, j))];
      setFocus(n.node.path);
      listRef.current?.querySelector(`[data-path="${CSS.escape(n.node.path)}"]`)?.scrollIntoView({ block: "nearest" });
    };
    switch (e.key) {
      case "ArrowDown":
        go(i < 0 ? 0 : i + 1);
        break;
      case "ArrowUp":
        go(i < 0 ? 0 : i - 1);
        break;
      case "ArrowRight":
        if (r?.node.kind === "folder") {
          if (!r.open) toggle(r.node.path, true);
          else go(i + 1);
        }
        break;
      case "ArrowLeft":
        if (r?.node.kind === "folder" && r.open) toggle(r.node.path, false);
        else if (r) {
          // Do Folderu nadrzędnego.
          while (i > 0 && rows[i].depth >= r.depth) i--;
          if (rows[i].depth < r.depth) go(i);
        }
        break;
      case "Enter":
        if (r) activate(r.node);
        break;
      case "Home":
        go(0);
        break;
      case "ContextMenu":
      case "F10": {
        if (e.key === "F10" && !e.shiftKey) return;
        const el = r && listRef.current?.querySelector(`[data-path="${CSS.escape(r.node.path)}"]`);
        const b = el?.getBoundingClientRect();
        if (r && b) onMenu?.(r.node, b.left + 24, b.bottom);
        break;
      }
      case "End":
        go(rows.length - 1);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  const render = (list: TreeNode[], depth: number) =>
    list.map((n) => {
      const isFolder = n.kind === "folder";
      const isOpen = isFolder && (q !== "" || expanded.has(n.path));
      const locked = !!noai && (noai.has(n.path) || (n.folder_note !== null && noai.has(n.folder_note)));
      const cls = [
        "tree-row",
        isFolder ? "folder" : "leaf",
        isActive(n) ? "active" : "",
        focus === n.path ? "focused" : "",
      ].join(" ");
      return (
        <div key={n.path} role="treeitem" aria-expanded={isFolder ? isOpen : undefined} aria-selected={isActive(n)}>
          <div
            className={cls}
            data-path={n.path}
            title={n.title}
            onClick={() => {
              setFocus(n.path);
              activate(n);
            }}
            onContextMenu={(e) => {
              if (!onMenu) return;
              e.preventDefault();
              e.stopPropagation();
              setFocus(n.path);
              onMenu(n, e.clientX, e.clientY);
            }}
          >
            {isFolder ? (
              <span
                className={`tree-chevron${isOpen ? " open" : ""}`}
                onClick={(e) => {
                  // Strzałka tylko zwija i rozwija, bez otwierania Notatki folderu.
                  e.stopPropagation();
                  toggle(n.path);
                }}
              >
                <ChevronRight size={14} />
              </span>
            ) : (
              <span className="tree-chevron spacer" />
            )}
            <span className="tree-icon">
              {isFolder ? isOpen ? <FolderOpenIcon size={15} /> : <FolderIcon size={15} /> : <FileIcon size={15} />}
            </span>
            <span className="tree-label">{n.title}</span>
            {locked && (
              <span className="tree-lock" title="Ukryta przed AI (#noai)">
                <LockIcon size={12} />
              </span>
            )}
            {isFolder && <span className="tree-count">{counts.get(n.path) ?? 0}</span>}
          </div>
          {isOpen && n.children.length > 0 && (
            <div className="tree-children" role="group">
              {render(n.children, depth + 1)}
            </div>
          )}
        </div>
      );
    });

  return (
    <div className="tree">
      <div className="tree-tools">
        <label className="search-field">
          <SearchIcon size={14} />
          <input placeholder="Filtruj po tytule…" value={query} onChange={(e) => setQuery(e.target.value)} />
          {query && (
            <button className="icon-btn tiny" onClick={() => setQuery("")} aria-label="Wyczyść filtr">
              <XIcon size={12} />
            </button>
          )}
        </label>
        <button
          className="icon-btn"
          title="Zwiń wszystkie Foldery"
          aria-label="Zwiń wszystkie Foldery"
          onClick={() => setExpanded(new Set())}
        >
          <CollapseIcon size={15} />
        </button>
      </div>
      <div
        className="tree-list"
        ref={listRef}
        role="tree"
        tabIndex={0}
        onKeyDown={onKey}
        onContextMenu={(e) => {
          if (!onMenu) return;
          e.preventDefault();
          onMenu(null, e.clientX, e.clientY);
        }}
      >
        {visible.length === 0 ? (
          <div className="muted pad">{q ? "Brak pasujących Notatek" : "Pusto"}</div>
        ) : (
          render(visible, 0)
        )}
      </div>
    </div>
  );
}
