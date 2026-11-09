import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, folderOf, type Report, type Summary, type TreeNode } from "./lib/api";
import { Tree } from "./components/Tree";
import { NoteView } from "./components/NoteView";
import { ReportView } from "./components/ReportView";
import { ImportWizard } from "./components/ImportWizard";
import { TagsPanel } from "./components/TagsPanel";
import { GraphView } from "./components/GraphView";
import { SearchPanel } from "./components/SearchPanel";

type Main =
  | { kind: "note"; path: string; edit?: boolean }
  | { kind: "graph" }
  | { kind: "report" }
  | { kind: "import" }
  | { kind: "empty" };

type Side = "notes" | "search" | "tags" | "trash";

function countNotes(nodes: TreeNode[]): number {
  return nodes.reduce((n, x) => n + (x.kind === "note" ? 1 : 0) + (x.folder_note ? 1 : 0) + countNotes(x.children), 0);
}

export default function App() {
  const [vault, setVault] = useState<string | null | undefined>(undefined);
  const [side, setSide] = useState<Side>("notes");
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [trashTree, setTrashTree] = useState<TreeNode[]>([]);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [tag, setTag] = useState<string | null>(null);
  const [main, setMain] = useState<Main>({ kind: "empty" });
  // Ostatnio otwarta Notatka: środek grafu lokalnego.
  const [lastNote, setLastNote] = useState<string | null>(null);
  const [report, setReport] = useState<Report | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    api.getVault().then((v) => setVault(v ?? null));
  }, []);

  const loadTrees = useCallback(async () => {
    const [t, k] = await Promise.all([api.tree(false), api.tree(true)]);
    setTree(t);
    setTrashTree(k);
  }, []);

  const loadVault = useCallback(
    async (rebuild: boolean) => {
      try {
        const [, s, r] = await Promise.all([loadTrees(), api.index(rebuild), api.readReport()]);
        setSummary(s);
        setReport(r);
      } catch (e) {
        setError(String(e));
      }
    },
    [loadTrees],
  );

  useEffect(() => {
    if (vault) loadVault(true);
  }, [vault, loadVault]);

  // Pliki mogły zmienić się poza aplikacją: po powrocie do okna odtwarzamy indeks.
  useEffect(() => {
    if (!vault) return;
    const onFocus = () => loadVault(true);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [vault, loadVault]);

  const onError = useCallback((m: string) => setError(m), []);

  // Ctrl+Shift+F: przejście do wyszukiwania z kursorem w polu.
  const [searchFocus, setSearchFocus] = useState(0);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setSide("search");
        setSearchFocus((n) => n + 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const openNote = useCallback((path: string, edit?: boolean) => {
    if (!path) return setMain({ kind: "empty" });
    if (path.startsWith(".kosz/")) setSide("trash");
    setLastNote(path);
    setMain({ kind: "note", path, edit });
  }, []);

  const openTag = useCallback((t: string) => {
    setSide("tags");
    setTag(t);
  }, []);

  const onChanged = useCallback(
    (treeChanged: boolean) => {
      api.index(false).then(setSummary).catch(() => {});
      if (treeChanged) loadTrees().catch((e) => setError(String(e)));
    },
    [loadTrees],
  );

  const newNote = async () => {
    const current = main.kind === "note" && !main.path.startsWith(".kosz/") ? main.path : null;
    try {
      const p = await api.createNote(current ? folderOf(current) : "", "");
      onChanged(true);
      openNote(p, true);
    } catch (e) {
      setError(String(e));
    }
  };

  const openVault = async () => {
    const p = await open({ directory: true, title: "Wskaż katalog Skarbca" });
    if (typeof p !== "string") return;
    try {
      await api.setVault(p);
      setVault(p);
      setMain({ kind: "empty" });
    } catch (e) {
      setError(String(e));
    }
  };

  const onImported = (r: Report) => {
    setReport(r);
    setSide("notes");
    setVault(r.vault);
    setMain({ kind: "report" });
  };

  if (vault === undefined) return null;

  if (!vault) {
    return (
      <div className="welcome">
        {main.kind === "import" ? (
          <ImportWizard onDone={onImported} onCancel={() => setMain({ kind: "empty" })} />
        ) : (
          <div className="wizard">
            <h1>Notes Workbench</h1>
            <p className="muted">Notatki w plikach Markdown. Zacznij od Importu z All My Notes Organizer albo otwórz istniejący Skarbiec.</p>
            <div className="actions">
              <button onClick={openVault}>Otwórz Skarbiec…</button>
              <button className="primary" onClick={() => setMain({ kind: "import" })}>
                Importuj z AMNO…
              </button>
            </div>
            {error && <div className="banner bad">{error}</div>}
          </div>
        )}
      </div>
    );
  }

  const vaultName = vault.split(/[\\/]/).filter(Boolean).pop();
  const noaiSet = new Set(summary?.noai ?? []);
  const selected = main.kind === "note" ? main.path : null;

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="side-head">
          <div className="vault-name" title={vault}>
            {vaultName}
          </div>
          <div className="side-actions">
            <button onClick={openVault} title="Otwórz inny Skarbiec">
              Otwórz…
            </button>
            <button onClick={() => setMain({ kind: "import" })} title="Import z AMNO do nowego Skarbca">
              Import…
            </button>
          </div>
        </div>
        <div className="side-tools">
          <button className="primary" onClick={() => void newNote()} title="Nowa Notatka w Folderze bieżącej Notatki">
            + Notatka
          </button>
          <button className={main.kind === "graph" ? "on" : ""} onClick={() => setMain({ kind: "graph" })}>
            Graf wiedzy
          </button>
        </div>
        <div className="side-tabs">
          <button className={`tab${side === "notes" ? " on" : ""}`} onClick={() => setSide("notes")}>
            Notatki
          </button>
          <button className={`tab${side === "search" ? " on" : ""}`} onClick={() => setSide("search")}>
            Szukaj
          </button>
          <button className={`tab${side === "tags" ? " on" : ""}`} onClick={() => setSide("tags")}>
            Tagi ({summary?.tags.length ?? 0})
          </button>
          <button className={`tab${side === "trash" ? " on" : ""}`} onClick={() => setSide("trash")}>
            Kosz ({countNotes(trashTree)})
          </button>
        </div>
        {/* Panel wyszukiwania zostaje w drzewie DOM, żeby po przełączeniu zakładek nie gubić wyników. */}
        <div className="tree" style={{ display: side === "search" ? undefined : "none" }}>
          <SearchPanel activeNote={selected} focusKey={searchFocus} onOpen={openNote} onError={onError} />
        </div>
        {side === "search" ? null : side === "tags" ? (
          <div className="tree">
            <TagsPanel summary={summary} selected={tag} activeNote={selected} onSelect={setTag} onOpen={openNote} />
          </div>
        ) : (
          <Tree
            nodes={side === "trash" ? trashTree : tree}
            selected={selected}
            noai={noaiSet}
            onOpen={openNote}
          />
        )}
        {report && (
          <button className={`side-report${report.ok ? "" : " bad"}`} onClick={() => setMain({ kind: "report" })}>
            Raport importu {report.ok ? "✓" : `(${report.errors} błędów)`}
          </button>
        )}
      </aside>
      <main className="main">
        {error && (
          <div className="banner bad toast" onClick={() => setError("")} title="Kliknij, aby zamknąć">
            {error}
          </div>
        )}
        {main.kind === "note" && (
          <NoteView
            key={main.path}
            path={main.path}
            startEditing={main.edit}
            summary={summary}
            onError={onError}
            onOpen={openNote}
            onTag={openTag}
            onChanged={onChanged}
          />
        )}
        {main.kind === "graph" && (
          <GraphView center={lastNote} onOpen={openNote} onTag={openTag} onError={onError} />
        )}
        {main.kind === "report" && report && <ReportView report={report} onOpen={openNote} />}
        {main.kind === "import" && (
          <div className="welcome">
            <ImportWizard onDone={onImported} onCancel={() => setMain({ kind: "empty" })} />
          </div>
        )}
        {main.kind === "empty" && <div className="empty muted">Wybierz Notatkę z drzewa albo utwórz nową.</div>}
      </main>
    </div>
  );
}
