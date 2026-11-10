import { useCallback, useState } from "react";
import type { TreeNode } from "./api";

/** Sposób ułożenia zawartości Folderu: Kolejność (ręczna, z AMNO), po nazwie albo po dacie zmiany. */
export type SortMode = "order" | "name" | "date";

/** Ścieżka Folderu → sposób sortowania; `""` to korzeń Skarbca. Brak wpisu = Kolejność. */
export type SortMap = Record<string, Exclude<SortMode, "order">>;

export const SORT_LABELS: Record<SortMode, string> = {
  order: "Kolejność",
  name: "Nazwa (A–Z)",
  date: "Data zmiany (najnowsze)",
};

const KEY = "nw.tree.sort";

function load(): SortMap {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    if (!v || typeof v !== "object") return {};
    const out: SortMap = {};
    for (const [k, m] of Object.entries(v)) if (m === "name" || m === "date") out[k] = m;
    return out;
  } catch {
    return {};
  }
}

/** Zapamiętane (w tej przeglądarce aplikacji) sortowanie Folderów. */
export function useTreeSort() {
  const [map, setMap] = useState<SortMap>(load);
  const setSort = useCallback((folder: string, mode: SortMode) => {
    setMap((prev) => {
      const next = { ...prev };
      if (mode === "order") delete next[folder];
      else next[folder] = mode;
      try {
        localStorage.setItem(KEY, JSON.stringify(next));
      } catch {
        /* bez zapamiętania */
      }
      return next;
    });
  }, []);
  return { sort: map, setSort };
}

const collator = new Intl.Collator("pl", { numeric: true, sensitivity: "base" });

/** Najnowsza zmiana w węźle: Notatka, Notatka folderu albo cokolwiek w podfolderach. */
export function newest(n: TreeNode): number {
  return n.children.reduce((m, c) => Math.max(m, newest(c)), n.modified ?? 0);
}

function sortList(list: TreeNode[], mode: SortMode | undefined): TreeNode[] {
  if (!mode || mode === "order") return list;
  const folderFirst = (a: TreeNode, b: TreeNode) => (a.kind === b.kind ? 0 : a.kind === "folder" ? -1 : 1);
  const byName = (a: TreeNode, b: TreeNode) => collator.compare(a.title, b.title);
  const cmp =
    mode === "name"
      ? (a: TreeNode, b: TreeNode) => folderFirst(a, b) || byName(a, b)
      : (a: TreeNode, b: TreeNode) => folderFirst(a, b) || newest(b) - newest(a) || byName(a, b);
  return [...list].sort(cmp);
}

/** Drzewo z zawartością każdego Folderu ułożoną według jego sposobu sortowania. */
export function sortTree(nodes: TreeNode[], sort: SortMap, folder = ""): TreeNode[] {
  return sortList(nodes, sort[folder]).map((n) =>
    n.kind === "folder" && n.children.length ? { ...n, children: sortTree(n.children, sort, n.path) } : n,
  );
}
