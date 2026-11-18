/**
 * Historia otwieranych Notatek (osobno dla każdego Skarbca), do ekranu „Ostatnie”.
 * Edycje bierzemy z dat modyfikacji plików w drzewie, więc łapią też zmiany spoza aplikacji.
 */

export type Viewed = { path: string; at: number };

const MAX = 60;
const key = (vault: string) => `nw.recent.${vault}`;

export function loadViewed(vault: string): Viewed[] {
  try {
    const v = JSON.parse(localStorage.getItem(key(vault)) ?? "[]");
    return Array.isArray(v) ? v.filter((x) => typeof x?.path === "string" && typeof x?.at === "number") : [];
  } catch {
    return [];
  }
}

function save(vault: string, list: Viewed[]) {
  try {
    localStorage.setItem(key(vault), JSON.stringify(list.slice(0, MAX)));
  } catch {
    /* brak miejsca albo zablokowany storage: historia jest tylko wygodą */
  }
}

export function markViewed(vault: string, path: string) {
  if (path.startsWith(".kosz/")) return;
  save(vault, [{ path, at: Date.now() }, ...loadViewed(vault).filter((x) => x.path !== path)]);
}

/** Notatka albo Folder zmienił ścieżkę: historia idzie za nim. */
export function moveViewed(vault: string, from: string, to: string, isFolder: boolean) {
  const list = loadViewed(vault).map((x) => {
    if (!isFolder && x.path === from) return { ...x, path: to };
    if (isFolder && x.path.startsWith(from + "/")) return { ...x, path: to + x.path.slice(from.length) };
    return x;
  });
  save(vault, list);
}
