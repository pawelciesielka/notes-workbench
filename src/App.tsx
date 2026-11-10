import { useCallback, useEffect, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, folderOf, type Report, type Summary, type TreeNode } from "./lib/api";
import { Tree } from "./components/Tree";
import { NoteView } from "./components/NoteView";
import { ReportView } from "./components/ReportView";
import { ImportWizard } from "./components/ImportWizard";
import { TagsPanel } from "./components/TagsPanel";
import { GraphView } from "./components/GraphView";
import { SearchPanel } from "./components/SearchPanel";
import { ProposalsView } from "./components/ProposalsView";
import { McpView } from "./components/McpView";
import { useTheme, type ThemeMode } from "./lib/theme";
import { ContextMenu, FolderPicker, PromptDialog, type MenuItem } from "./components/Menu";
import { sortItems } from "./components/Tree";
import { SORT_LABELS, useTreeSort } from "./lib/treeSort";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  CheckIcon,
  FileIcon,
  LinkIcon,
  LockIcon,
  MoveIcon,
  PencilIcon,
  RestoreIcon,
  TypeIcon,
  UnlockIcon,
  FolderIcon,
  FolderOpenAction,
  GraphIcon,
  ImportIcon,
  MonitorIcon,
  MoonIcon,
  PlusIcon,
  ReportIcon,
  SearchIcon,
  SparkIcon,
  PlugIcon,
  SunIcon,
  TagIcon,
  TrashIcon,
  VaultIcon,
  XIcon,
} from "./components/Icons";

type Main =
  | { kind: "note"; path: string; edit?: boolean }
  | { kind: "graph" }
  | { kind: "proposals" }
  | { kind: "mcp" }
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
  const [proposalCount, setProposalCount] = useState(0);
  const [themeMode, setThemeMode, theme] = useTheme();
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
        api
          .proposals()
          .then((p) => setProposalCount(p.length))
          .catch(() => {});
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
    (_treeChanged: boolean) => {
      api.index(false).then(setSummary).catch(() => {});
      // Drzewo odświeżane zawsze, bo niesie też daty modyfikacji (filtr „Zmienione”).
      loadTrees().catch((e) => setError(String(e)));
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

  // ===== Menu kontekstowe drzewa =====
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const treeSort = useTreeSort();
  const [dialog, setDialog] = useState<ReactNode>(null);
  const [info, setInfo] = useState("");
  const [reloadKey, setReloadKey] = useState(0);
  const closeMenu = useCallback(() => setMenu(null), []);
  const closeDialog = () => setDialog(null);

  useEffect(() => {
    if (!info) return;
    const t = window.setTimeout(() => setInfo(""), 3500);
    return () => window.clearTimeout(t);
  }, [info]);

  const run = async (f: () => Promise<unknown>) => {
    try {
      await f();
    } catch (e) {
      setError(String(e));
    }
  };

  const openPath = main.kind === "note" ? main.path : null;
  /** Otwarta Notatka zmieniła ścieżkę (przeniesienie, zmiana nazwy Folderu). */
  const followMove = (from: string, to: string, isFolder: boolean) => {
    if (!openPath) return;
    if (!isFolder && openPath === from) openNote(to);
    else if (isFolder && openPath.startsWith(from + "/")) openNote(to + openPath.slice(from.length));
  };
  const fixedMsg = (n: number) => (n > 0 ? ` Poprawiono Linki w ${n} Notatkach.` : "");

  const noteMenu = (n: TreeNode): MenuItem[] => {
    const locked = summary?.noai.includes(n.path) ?? false;
    const stem = n.path.split("/").pop()!.replace(/\.md$/, "");
    const link = stem === n.title ? `[[${stem}]]` : `[[${stem}|${n.title}]]`;
    return [
      { label: "Otwórz", icon: <FileIcon size={15} />, onClick: () => openNote(n.path) },
      { label: "Edytuj", icon: <PencilIcon size={15} />, hint: "Ctrl+E", onClick: () => openNote(n.path, true) },
      {
        label: "Zmień tytuł…",
        icon: <TypeIcon size={15} />,
        onClick: () =>
          setDialog(
            <PromptDialog
              title="Zmień tytuł Notatki"
              label="Tytuł"
              initial={n.title}
              okLabel="Zmień"
              hint="Nazwa pliku zostaje ta sama, więc Linki do Notatki działają dalej."
              onClose={closeDialog}
              onOk={(t) =>
                run(async () => {
                  await api.setTitle(n.path, t);
                  onChanged(true);
                  setReloadKey((k) => k + 1);
                })
              }
            />,
          ),
      },
      { separator: true },
      {
        label: "Nowa Notatka obok",
        icon: <PlusIcon size={15} />,
        onClick: () =>
          run(async () => {
            const p = await api.createNote(folderOf(n.path), "");
            onChanged(true);
            openNote(p, true);
          }),
      },
      {
        label: "Przenieś do Folderu…",
        icon: <MoveIcon size={15} />,
        onClick: () =>
          setDialog(
            <FolderPicker
              title={`Przenieś „${n.title}”`}
              tree={tree}
              current={folderOf(n.path)}
              onClose={closeDialog}
              onPick={(f) =>
                run(async () => {
                  const r = await api.moveNote(n.path, f);
                  onChanged(true);
                  followMove(n.path, r.path, false);
                  setInfo(`Przeniesiono do ${f || "głównego katalogu"}.${fixedMsg(r.fixed)}`);
                })
              }
            />,
          ),
      },
      {
        label: "Kopiuj Link",
        icon: <LinkIcon size={15} />,
        hint: link.length > 22 ? undefined : link,
        title: link,
        onClick: () =>
          run(async () => {
            await navigator.clipboard.writeText(link);
            setInfo(`Skopiowano ${link}`);
          }),
      },
      {
        label: locked ? "Udostępnij AI" : "Ukryj przed AI",
        icon: locked ? <UnlockIcon size={15} /> : <LockIcon size={15} />,
        onClick: () =>
          run(async () => {
            await api.setNoai(n.path, !locked);
            const s = await api.index(false);
            setSummary(s);
            setReloadKey((k) => k + 1);
            if (locked && s.noai.includes(n.path))
              setError("Notatka nadal jest ukryta: Tag #noai ma w treści albo ma go Notatka folderu nad nią.");
          }),
      },
      { label: "Pokaż w Eksploratorze", icon: <FolderOpenAction size={15} />, onClick: () => run(() => api.revealPath(n.path)) },
      { separator: true },
      {
        label: "Przenieś do Kosza",
        icon: <TrashIcon size={15} />,
        danger: true,
        onClick: () =>
          run(async () => {
            if (!(await ask(`Przenieść Notatkę „${n.title}” do Kosza? Można ją potem przywrócić.`, { title: "Do Kosza", kind: "warning" })))
              return;
            await api.trashNote(n.path);
            onChanged(true);
            if (openPath === n.path) setMain({ kind: "empty" });
          }),
      },
    ];
  };

  const folderMenu = (n: TreeNode): MenuItem[] => {
    const locked = !!n.folder_note && (summary?.noai.includes(n.folder_note) ?? false);
    const count = (function c(x: TreeNode): number {
      return x.children.reduce((s, k) => s + (k.kind === "note" ? 1 : 0) + (k.folder_note ? 1 : 0) + c(k), 0);
    })(n) + (n.folder_note ? 1 : 0);
    return [
      ...(n.folder_note
        ? [{ label: "Otwórz Notatkę folderu", icon: <FileIcon size={15} />, onClick: () => openNote(n.folder_note!) }, { separator: true } as MenuItem]
        : []),
      {
        label: "Nowa Notatka w tym Folderze",
        icon: <PlusIcon size={15} />,
        onClick: () =>
          run(async () => {
            const p = await api.createNote(n.path, "");
            onChanged(true);
            openNote(p, true);
          }),
      },
      {
        label: "Nowy Folder…",
        icon: <FolderIcon size={15} />,
        onClick: () =>
          setDialog(
            <PromptDialog
              title={`Nowy Folder w „${n.title}”`}
              label="Nazwa"
              initial="Nowy Folder"
              okLabel="Utwórz"
              onClose={closeDialog}
              onOk={(name) =>
                run(async () => {
                  await api.createFolder(n.path, name);
                  onChanged(true);
                })
              }
            />,
          ),
      },
      {
        label: "Zmień nazwę…",
        icon: <TypeIcon size={15} />,
        onClick: () =>
          setDialog(
            <PromptDialog
              title="Zmień nazwę Folderu"
              label="Nazwa"
              initial={n.name}
              okLabel="Zmień"
              hint="Notatka folderu zmieni nazwę razem z nim, a Linki zapisane ze ścieżką zostaną poprawione."
              onClose={closeDialog}
              onOk={(name) =>
                run(async () => {
                  const r = await api.renameFolder(n.path, name);
                  onChanged(true);
                  followMove(n.path, r.path, true);
                  if (r.path !== n.path) setInfo(`Zmieniono nazwę Folderu.${fixedMsg(r.fixed)}`);
                })
              }
            />,
          ),
      },
      { separator: true },
      {
        label: locked ? "Udostępnij Folder AI" : "Ukryj Folder przed AI",
        icon: locked ? <UnlockIcon size={15} /> : <LockIcon size={15} />,
        title: locked ? undefined : "Dodaje Tag noai do Notatki folderu (tworzy ją, jeśli jej nie ma).",
        onClick: () =>
          run(async () => {
            await api.setFolderNoai(n.path, !locked);
            onChanged(true);
            setReloadKey((k) => k + 1);
          }),
      },
      { label: "Pokaż w Eksploratorze", icon: <FolderOpenAction size={15} />, onClick: () => run(() => api.revealPath(n.path)) },
      { separator: true },
      {
        label: "Przenieś Folder do Kosza",
        icon: <TrashIcon size={15} />,
        danger: true,
        onClick: () =>
          run(async () => {
            const msg = `Przenieść Folder „${n.title}”${count ? ` razem z ${count} Notatkami` : ""} do Kosza? Można go potem przywrócić.`;
            if (!(await ask(msg, { title: "Do Kosza", kind: "warning" }))) return;
            await api.trashFolder(n.path);
            onChanged(true);
            if (openPath && (openPath.startsWith(n.path + "/") || openPath === n.folder_note)) setMain({ kind: "empty" });
          }),
      },
    ];
  };

  const trashMenu = (n: TreeNode): MenuItem[] => [
    ...(n.kind === "note" ? [{ label: "Otwórz", icon: <FileIcon size={15} />, onClick: () => openNote(n.path) }] : []),
    {
      label: n.kind === "folder" ? "Przywróć Folder" : "Przywróć",
      icon: <RestoreIcon size={15} />,
      onClick: () =>
        run(async () => {
          const p = n.kind === "folder" ? await api.restoreFolder(n.path) : await api.restoreNote(n.path);
          onChanged(true);
          setInfo(`Przywrócono: ${p}`);
          if (openPath === n.path) openNote(p);
        }),
    },
    { label: "Pokaż w Eksploratorze", icon: <FolderOpenAction size={15} />, onClick: () => run(() => api.revealPath(n.path)) },
  ];

  const rootMenu = (): MenuItem[] => [
    {
      label: "Nowa Notatka",
      icon: <PlusIcon size={15} />,
      onClick: () =>
        run(async () => {
          const p = await api.createNote("", "");
          onChanged(true);
          openNote(p, true);
        }),
    },
    {
      label: "Nowy Folder…",
      icon: <FolderIcon size={15} />,
      onClick: () =>
        setDialog(
          <PromptDialog
            title="Nowy Folder"
            label="Nazwa"
            initial="Nowy Folder"
            okLabel="Utwórz"
            onClose={closeDialog}
            onOk={(name) =>
              run(async () => {
                await api.createFolder("", name);
                onChanged(true);
              })
            }
          />,
        ),
    },
    { label: "Pokaż Skarbiec w Eksploratorze", icon: <FolderOpenAction size={15} />, onClick: () => run(() => api.revealVault()) },
  ];

  const onTreeMenu = (n: TreeNode | null, x: number, y: number) => {
    let items =
      side === "trash" ? (n ? trashMenu(n) : []) : !n ? rootMenu() : n.kind === "folder" ? folderMenu(n) : noteMenu(n);
    if (side !== "trash" && (!n || n.kind === "folder")) {
      const folder = n ? n.path : "";
      const mode = treeSort.sort[folder] ?? "order";
      items = [
        ...items,
        { separator: true },
        {
          label: `Sortowanie: ${SORT_LABELS[mode]}…`,
          onClick: () => setMenu({ x, y, items: sortItems(mode, (m) => treeSort.setSort(folder, m)) }),
        },
      ];
    }
    if (items.length) setMenu({ x, y, items });
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

  const nextTheme: Record<ThemeMode, ThemeMode> = { auto: "light", light: "dark", dark: "auto" };
  const themeLabel: Record<ThemeMode, string> = {
    auto: "Motyw: jak w Windows",
    light: "Motyw: jasny",
    dark: "Motyw: ciemny",
  };
  const trashCount = countNotes(trashTree);

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="side-head">
          <div className="vault-name">
            <span className="vault-icon">
              <VaultIcon size={15} />
            </span>
            <div className="vault-text">
              <span className="vault-title">{vaultName}</span>
              <button
                className="vault-path"
                title={`Skarbiec: ${vault}\nKliknij, aby otworzyć katalog w Eksploratorze`}
                onClick={() => api.revealVault().catch((e) => setError(String(e)))}
              >
                <bdi dir="ltr">{vault}</bdi>
              </button>
            </div>
          </div>
          <div className="side-actions">
            <button
              className="icon-btn"
              onClick={() => setThemeMode(nextTheme[themeMode])}
              title={`${themeLabel[themeMode]} (kliknij, aby zmienić)`}
              aria-label={themeLabel[themeMode]}
            >
              {themeMode === "auto" ? <MonitorIcon /> : themeMode === "light" ? <SunIcon /> : <MoonIcon />}
            </button>
            <button className="icon-btn" onClick={openVault} title="Otwórz inny Skarbiec" aria-label="Otwórz inny Skarbiec">
              <FolderOpenAction />
            </button>
            <button
              className="icon-btn"
              onClick={() => setMain({ kind: "import" })}
              title="Import z AMNO do nowego Skarbca"
              aria-label="Import z AMNO"
            >
              <ImportIcon />
            </button>
          </div>
        </div>
        <div className="side-tools">
          <button className="primary new-note" onClick={() => void newNote()} title="Nowa Notatka w Folderze bieżącej Notatki">
            <PlusIcon size={15} /> Nowa Notatka
          </button>
          <button
            className={`icon-btn${main.kind === "graph" ? " on" : ""}`}
            onClick={() => setMain({ kind: "graph" })}
            title="Graf wiedzy"
            aria-label="Graf wiedzy"
          >
            <GraphIcon />
          </button>
          <button
            className={`icon-btn${main.kind === "proposals" ? " on" : ""}`}
            onClick={() => setMain({ kind: "proposals" })}
            title="Propozycje powiązań i Tagów od AI"
            aria-label="Propozycje"
          >
            <SparkIcon />
            {proposalCount > 0 && <span className="badge">{proposalCount}</span>}
          </button>
          <button
            className={`icon-btn${main.kind === "mcp" ? " on" : ""}`}
            onClick={() => setMain({ kind: "mcp" })}
            title="Połącz z AI: Claude Desktop, Claude Code, Codex (MCP)"
            aria-label="Połącz z AI"
          >
            <PlugIcon />
          </button>
        </div>
        <div className="side-tabs" role="tablist">
          <button className={`tab${side === "notes" ? " on" : ""}`} onClick={() => setSide("notes")} title="Drzewo Notatek">
            <FolderIcon size={14} /> Notatki
          </button>
          <button className={`tab${side === "search" ? " on" : ""}`} onClick={() => setSide("search")} title="Szukaj (Ctrl+Shift+F)">
            <SearchIcon size={14} /> Szukaj
          </button>
          <button className={`tab${side === "tags" ? " on" : ""}`} onClick={() => setSide("tags")} title="Tagi">
            <TagIcon size={14} /> Tagi
            {(summary?.tags.length ?? 0) > 0 && <span className="tab-count">{summary?.tags.length}</span>}
          </button>
          <button className={`tab${side === "trash" ? " on" : ""}`} onClick={() => setSide("trash")} title="Kosz">
            <TrashIcon size={14} /> Kosz
            {trashCount > 0 && <span className="tab-count">{trashCount}</span>}
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
            onMenu={onTreeMenu}
            sort={side === "trash" ? undefined : treeSort.sort}
            onSort={side === "trash" ? undefined : treeSort.setSort}
          />
        )}
        {report && (
          <div className="side-foot">
            <button
              className={`ghost side-report${report.ok ? "" : " bad"}${main.kind === "report" ? " on" : ""}`}
              onClick={() => setMain({ kind: "report" })}
            >
              <ReportIcon size={15} /> Raport importu {report.ok ? <CheckIcon size={14} /> : `(${report.errors} błędów)`}
            </button>
          </div>
        )}
      </aside>
      <main className="main">
        {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={closeMenu} />}
        {dialog}
        {info && !error && (
          <div className="banner ok toast" onClick={() => setInfo("")} role="status">
            <CheckIcon size={15} />
            <span>{info}</span>
          </div>
        )}
        {error && (
          <div className="banner bad toast" onClick={() => setError("")} title="Kliknij, aby zamknąć" role="alert">
            <span>{error}</span>
            <XIcon size={14} />
          </div>
        )}
        {main.kind === "note" && (
          <NoteView
            key={`${main.path}#${reloadKey}`}
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
          <GraphView center={lastNote} theme={theme} onOpen={openNote} onTag={openTag} onError={onError} />
        )}
        {main.kind === "mcp" && <McpView onError={onError} onOpenProposals={() => setMain({ kind: "proposals" })} />}
        {main.kind === "proposals" && (
          <ProposalsView
            onOpen={openNote}
            onError={onError}
            onChanged={() => {
              onChanged(false);
              api
                .proposals()
                .then((p) => setProposalCount(p.length))
                .catch(() => {});
            }}
          />
        )}
        {main.kind === "report" && report && <ReportView report={report} onOpen={openNote} />}
        {main.kind === "import" && (
          <div className="welcome">
            <ImportWizard onDone={onImported} onCancel={() => setMain({ kind: "empty" })} />
          </div>
        )}
        {main.kind === "empty" && (
          <div className="empty">
            <div className="empty-icon">
              <FileIcon size={26} />
            </div>
            <h2>Wybierz Notatkę</h2>
            <div>Otwórz Notatkę z drzewa po lewej albo utwórz nową.</div>
            <div className="shortcuts">
              <kbd>Ctrl+Shift+F</kbd>
              <span>Szukaj w treści</span>
              <kbd>Ctrl+E</kbd>
              <span>Edycja i podgląd Notatki</span>
              <kbd>Ctrl+S</kbd>
              <span>Zapisz od razu (zapis i tak dzieje się sam)</span>
              <kbd>↑ ↓ ← →</kbd>
              <span>Poruszanie się po drzewie</span>
              <kbd>[[</kbd>
              <span>Link do innej Notatki (w edycji)</span>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
