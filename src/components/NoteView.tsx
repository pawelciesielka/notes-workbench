import { useCallback, useEffect, useRef, useState, type ComponentProps, type CSSProperties, type DragEvent } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkBreaks from "remark-breaks";
import rehypeRaw from "rehype-raw";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  api,
  fileName,
  folderOf,
  isExternal,
  linkify,
  resolveRelative,
  vaultUrl,
  type NoteData,
  type NoteLinks,
  type NoteRef,
  type Summary,
} from "../lib/api";
import { Editor } from "./Editor";
import { useNoteScale } from "../lib/zoom";
import { attachFiles, filesOf } from "../lib/attach";
import { LAYOUTS, useEditorMode, useSplit } from "../lib/split";
import { VisualEditor } from "./VisualEditor";
import { rehypeSourceLines, scrollPreviewTo } from "../lib/scrollSync";
import { CheckIcon, LayoutIcon, LinkIcon, LockIcon, PencilIcon, SparkIcon, TrashIcon, UnlockIcon, XIcon } from "./Icons";

const IMAGE_EXT = /\.(png|jpe?g|gif|bmp|webp|svg|ico)$/i;
const TRASH = ".kosz/";

type Props = {
  path: string;
  summary: Summary | null;
  startEditing?: boolean;
  onError: (msg: string) => void;
  onOpen: (path: string, edit?: boolean) => void;
  onTag: (tag: string) => void;
  /** Zmieniły się pliki Skarbca: `tree` gdy trzeba odświeżyć drzewo (tytuł, nowa Notatka). */
  onChanged: (tree: boolean) => void;
};

type SaveState = "saved" | "dirty" | "saving" | "error";

export function NoteView({ path, summary, startEditing, onError, onOpen, onTag, onChanged }: Props) {
  const [note, setNote] = useState<NoteData | null>(null);
  const [links, setLinks] = useState<NoteLinks | null>(null);
  const [edit, setEdit] = useState(!!startEditing);
  const [draft, setDraft] = useState("");
  const [save, setSave] = useState<SaveState>("saved");
  const [title, setTitle] = useState("");
  const [pdf, setPdf] = useState<string | null>(null);
  // Ctrl + rolka nad treścią: wielkość czcionki widoku i edytora (Ctrl+0 przywraca 100%).
  const contentRef = useRef<HTMLDivElement>(null);
  const [scale, scaleFlash, setScale] = useNoteScale(contentRef);
  const inTrash = path.startsWith(TRASH);

  // Stan do zapisu poza cyklem renderowania (debounce, zamknięcie Notatki).
  const ref = useRef({ raw: "", draft: "", dirty: false, timer: 0 as number | undefined });

  const [similar, setSimilar] = useState<NoteRef[]>([]);

  // Podgląd obok edytora pokazuje zapisaną treść; Linki odświeżane po każdym zapisie.
  const [splitLayout, setLayout, ratio, setRatio] = useSplit();
  // Edytor wizualny sam pokazuje wynik: bez podglądu obok.
  const [visual, setVisual] = useEditorMode();
  const layout = visual ? "single" : splitLayout;
  const [dragging, setDragging] = useState(false);
  const splitRef = useRef<HTMLDivElement>(null);
  // Podgląd idzie za edytorem: ostatnia pozycja edytora, ponawiana po odświeżeniu podglądu.
  const previewRef = useRef<HTMLDivElement>(null);
  const editorPos = useRef({ line: 1, offset: 0, atEnd: false });
  const onScrollLine = useCallback((line: number, offset: number, atEnd: boolean) => {
    editorPos.current = { line, offset, atEnd };
    if (previewRef.current) scrollPreviewTo(previewRef.current, line, offset, atEnd);
  }, []);
  const splitOnRef = useRef(false);
  splitOnRef.current = edit && layout !== "single";
  // Numer zapisu: pozycje Linków pasują do treści tylko z tego samego zapisu.
  const saveSeq = useRef(0);
  const [linksSeq, setLinksSeq] = useState(0);
  const splitActive = edit && layout !== "single";

  const loadLinks = useCallback(() => {
    const seqAtStart = saveSeq.current;
    api
      .noteLinks(path)
      .then((l) => {
        setLinks(l);
        setLinksSeq(seqAtStart);
      })
      .catch(() => setLinks(null));
    api
      .similarNotes(path)
      .then(setSimilar)
      .catch(() => setSimilar([]));
  }, [path]);

  useEffect(() => {
    let alive = true;
    api
      .readNote(path)
      .then((n) => {
        if (!alive) return;
        setNote(n);
        setDraft(n.body);
        setTitle(n.title);
        ref.current.raw = n.raw;
        ref.current.draft = n.body;
      })
      .catch((e) => onError(String(e)));
    loadLinks();
    return () => {
      alive = false;
    };
  }, [path, onError, loadLinks]);

  const flush = useCallback(async () => {
    const r = ref.current;
    window.clearTimeout(r.timer);
    if (!r.dirty) return;
    r.dirty = false;
    setSave("saving");
    try {
      const n = await api.saveNote(path, r.draft, r.raw);
      r.raw = n.raw;
      saveSeq.current++;
      setNote(n);
      if (splitOnRef.current) loadLinks();
      setSave(r.dirty ? "dirty" : "saved");
      onChanged(false);
    } catch (e) {
      r.dirty = true;
      setSave("error");
      onError(String(e));
    }
  }, [path, onChanged, onError, loadLinks]);

  // Zapis przy zamknięciu Notatki (przełączenie na inną).
  useEffect(() => () => void flush(), [flush]);

  const onDraft = (v: string) => {
    const r = ref.current;
    r.draft = v;
    r.dirty = true;
    setDraft(v);
    setSave("dirty");
    window.clearTimeout(r.timer);
    r.timer = window.setTimeout(() => void flush(), 800);
  };

  const toggleEdit = useCallback(async () => {
    if (edit) {
      await flush();
      loadLinks();
    }
    setEdit(!edit);
  }, [edit, flush, loadLinks]);

  // Po odświeżeniu podglądu (zapis, Linki, zmiana układu) wraca do miejsca edytora.
  useEffect(() => {
    if (!splitActive || !previewRef.current) return;
    const { line, offset, atEnd } = editorPos.current;
    scrollPreviewTo(previewRef.current, line, offset, atEnd);
  }, [splitActive, note, links, linksSeq, layout]);

  // Wczytany obraz zmienia wysokości w podglądzie: wyrównanie jeszcze raz.
  useEffect(() => {
    const pane = previewRef.current;
    if (!splitActive || !pane) return;
    const onLoad = () => {
      const { line, offset, atEnd } = editorPos.current;
      scrollPreviewTo(pane, line, offset, atEnd);
    };
    pane.addEventListener("load", onLoad, true);
    return () => pane.removeEventListener("load", onLoad, true);
  }, [splitActive, layout]);

  // Włączenie podglądu obok edytora: Linki do zapisanej treści.
  useEffect(() => {
    if (splitActive) loadLinks();
  }, [splitActive, loadLinks]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "e" && !inTrash) {
        e.preventDefault();
        void toggleEdit();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleEdit, inTrash]);

  // Załączniki: w edytorze trafiają w miejsce kursora (albo upuszczenia), w podglądzie na koniec Notatki.
  const [dropping, setDropping] = useState(false);
  const onFiles = useCallback(
    (files: File[], pasted: boolean) =>
      attachFiles(path, files, pasted).catch((e) => {
        onError(String(e));
        return "";
      }),
    [path, onError],
  );
  const appendFiles = useCallback(
    async (files: File[], pasted: boolean) => {
      if (inTrash || !files.length) return;
      try {
        await flush();
        const md = await attachFiles(path, files, pasted);
        const r = ref.current;
        const base = r.draft.replace(/\s+$/, "");
        const n = await api.saveNote(path, (base ? base + "\n\n" : "") + md + "\n", r.raw);
        r.raw = n.raw;
        r.draft = n.body;
        saveSeq.current++;
        setNote(n);
        setDraft(n.body);
        onChanged(false);
        loadLinks();
      } catch (e) {
        onError(String(e));
      }
    },
    [path, inTrash, flush, onChanged, onError, loadLinks],
  );

  // Ctrl+V ze zrzutem ekranu w podglądzie (edytor obsługuje wklejanie sam).
  useEffect(() => {
    if (edit || inTrash) return;
    const onPaste = (e: ClipboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest("input, textarea, [contenteditable=true]")) return;
      const files = filesOf(e.clipboardData);
      if (!files.length) return;
      e.preventDefault();
      void appendFiles(files, true);
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, [edit, inTrash, appendFiles]);

  const dropProps =
    edit || inTrash
      ? {}
      : {
          onDragOver: (e: DragEvent) => {
            if (!e.dataTransfer.types.includes("Files")) return;
            e.preventDefault();
            e.dataTransfer.dropEffect = "copy";
            setDropping(true);
          },
          onDragLeave: (e: DragEvent) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropping(false);
          },
          onDrop: (e: DragEvent) => {
            setDropping(false);
            const files = filesOf(e.dataTransfer);
            if (!files.length) return;
            e.preventDefault();
            void appendFiles(files, false);
          },
        };

  if (!note) return <div className="note muted pad">Wczytywanie…</div>;

  // Ukrycie przed AI: własny Tag noai (frontmatter albo treść) albo odziedziczony po Notatce folderu.
  const noai = !!summary?.noai.includes(path);
  const ownNoai = !!links?.tags.some((t) => t.toLowerCase() === "noai");
  const noaiInFm = /\bnoai\b/i.test(note.frontmatter);
  const toggleNoai = async () => {
    if (noai && !ownNoai) {
      onError("Notatka jest ukryta przed AI, bo Tag #noai ma Notatka folderu nad nią. Zdejmij go tam.");
      return;
    }
    if (noai && !noaiInFm) {
      onError("Tag #noai jest wpisany w treści Notatki: usuń go w edycji, żeby udostępnić Notatkę AI.");
      return;
    }
    try {
      await flush();
      const n = await api.setNoai(path, !noai);
      ref.current.raw = n.raw;
      setNote(n);
      onChanged(false);
      loadLinks();
    } catch (e) {
      onError(String(e));
    }
  };

  const commitTitle = async () => {
    if (title.trim() === note.title) return;
    try {
      const n = await api.setTitle(path, title);
      ref.current.raw = n.raw;
      setNote(n);
      setTitle(n.title);
      onChanged(true);
    } catch (e) {
      onError(String(e));
    }
  };

  const toTrash = async () => {
    const ok = await ask(`Przenieść Notatkę „${note.title}” do Kosza? Można ją potem przywrócić.`, {
      title: "Do Kosza",
      kind: "warning",
    });
    if (!ok) return;
    try {
      await flush();
      await api.trashNote(path);
      onChanged(true);
      onOpen("");
    } catch (e) {
      onError(String(e));
    }
  };

  const restore = async () => {
    try {
      const p = await api.restoreNote(path);
      onChanged(true);
      onOpen(p);
    } catch (e) {
      onError(String(e));
    }
  };

  /** Kliknięcie Linku do nieistniejącej Notatki tworzy ją (w Folderze bieżącej albo wskazanym w Linku). */
  const createFromLink = async (target: string) => {
    const i = target.lastIndexOf("/");
    const folder = i >= 0 ? target.slice(0, i) : folderOf(path).replace(/^\.kosz\/?/, "");
    const name = i >= 0 ? target.slice(i + 1) : target;
    const ok = await ask(`Notatka „${name}” nie istnieje. Utworzyć ją${folder ? ` w Folderze ${folder}` : ""}?`, {
      title: "Nowa Notatka",
    });
    if (!ok) return;
    try {
      const p = await api.createNote(folder, name);
      onChanged(true);
      onOpen(p, true);
    } catch (e) {
      onError(String(e));
    }
  };

  const openAttachment = (rel: string) => {
    if (/\.pdf$/i.test(rel)) setPdf(rel);
    else api.openVaultFile(rel).catch((e) => onError(String(e)));
  };

  const components: ComponentProps<typeof Markdown>["components"] = {
    img: ({ src, node: _node, ...rest }) => {
      const s = typeof src === "string" ? src : "";
      const url = s && !isExternal(s) ? vaultUrl(resolveRelative(note.path, s)) : s;
      return <img {...rest} src={url} loading="lazy" />;
    },
    a: ({ href, children, node: _node, ...rest }) => {
      const h = href ?? "";
      if (!h) return <span>{children}</span>;
      const scheme = /^(wiki|wiki-new|tag):(.*)$/.exec(h);
      if (scheme) {
        const arg = decodeURIComponent(scheme[2]);
        if (scheme[1] === "tag")
          return (
            <a href="#" className="tag" onClick={(e) => (e.preventDefault(), onTag(arg))}>
              {children}
            </a>
          );
        const isNew = scheme[1] === "wiki-new";
        return (
          <a
            href="#"
            className={`wikilink${isNew ? " unresolved" : ""}`}
            title={isNew ? `Notatka „${arg}” jeszcze nie istnieje: kliknij, aby ją utworzyć` : arg}
            onClick={(e) => {
              e.preventDefault();
              if (isNew) void createFromLink(arg);
              else onOpen(arg);
            }}
          >
            {children}
          </a>
        );
      }
      if (isExternal(h)) {
        const isFile = /^file:/i.test(h);
        return (
          <a
            {...rest}
            href={h}
            className={isFile ? "external-file" : undefined}
            title={isFile ? `Link zewnętrzny: ${decodeURI(h.replace(/^file:\/+/i, ""))}` : h}
            onClick={(e) => {
              e.preventDefault();
              api.openLink(h).catch((err) => onError(String(err)));
            }}
          >
            {children}
          </a>
        );
      }
      const rel = resolveRelative(note.path, h);
      if (/\.md$/i.test(rel)) {
        return (
          <a {...rest} href="#" className="wikilink" onClick={(e) => (e.preventDefault(), onOpen(rel))}>
            {children}
          </a>
        );
      }
      if (IMAGE_EXT.test(rel)) {
        return (
          <a {...rest} href="#" onClick={(e) => (e.preventDefault(), openAttachment(rel))}>
            {children}
          </a>
        );
      }
      return (
        <a
          {...rest}
          href="#"
          className="attachment"
          title={`Załącznik: ${fileName(rel)}`}
          onClick={(e) => (e.preventDefault(), openAttachment(rel))}
        >
          <span className="attachment-icon">{/\.pdf$/i.test(rel) ? "PDF" : "📎"}</span>
          {children}
        </a>
      );
    },
  };

  const crumbs = note.path.split("/").slice(0, -1);
  // Pozycje Linków dotyczą zapisanej treści; podczas edycji widok nie jest pokazywany.
  const shownBody =
    links && (!edit || layout !== "single") && linksSeq === saveSeq.current ? linkify(note.body, links.spans) : note.body;
  const saveLabel = { saved: "Zapisano", dirty: "Zmiany…", saving: "Zapisywanie…", error: "Błąd zapisu" }[save];

  const preview = (
    <>
            {note.body.trim() === "" ? (
              <div className="muted empty-note">
                Pusta Notatka. <button onClick={() => void toggleEdit()}>Edytuj</button>
              </div>
            ) : (
              <div className="markdown">
                <Markdown
                  remarkPlugins={[remarkGfm, remarkBreaks]}
                  rehypePlugins={[rehypeRaw, rehypeSourceLines]}
                  urlTransform={(u) => u}
                  components={components}
                >
                  {shownBody}
                </Markdown>
              </div>
            )}
            {links && (links.backlinks.length > 0 || links.related.length > 0 || similar.length > 0) && (
              <section className="backlinks">
                {links.related.length > 0 && (
                  <>
                    <h2>
                      <SparkIcon size={13} /> Powiązane ({links.related.length})
                    </h2>
                    <ul>
                      {links.related.map((r) => (
                        <li key={r.path}>
                          <a href="#" className="wikilink" onClick={(e) => (e.preventDefault(), onOpen(r.path))}>
                            {r.title}
                          </a>
                        </li>
                      ))}
                    </ul>
                  </>
                )}
                {links.backlinks.length > 0 && (
                  <>
                    <h2>
                      <LinkIcon size={13} /> Linki do tej Notatki ({links.backlinks.length})
                    </h2>
                    <ul>
                      {links.backlinks.map((b) => (
                        <li key={b.path}>
                          <a href="#" className="wikilink" onClick={(e) => (e.preventDefault(), onOpen(b.path))}>
                            {b.title}
                          </a>
                          {b.related && <span className="muted small"> · Powiązanie</span>}
                          {b.lines.map((l, i) => (
                            <div key={i} className="backlink-line">
                              {l}
                            </div>
                          ))}
                        </li>
                      ))}
                    </ul>
                  </>
                )}
                {similar.length > 0 && (
                  <>
                    <h2 title="Notatki o podobnej treści według lokalnego modelu; to nie są Linki.">
                      Podobne treścią ({similar.length})
                    </h2>
                    <ul>
                      {similar.map((r) => (
                        <li key={r.path}>
                          <a href="#" className="wikilink" onClick={(e) => (e.preventDefault(), onOpen(r.path))}>
                            {r.title}
                          </a>
                          <span className="muted small"> {folderOf(r.path).replace(/\//g, " / ")}</span>
                        </li>
                      ))}
                    </ul>
                  </>
                )}
              </section>
            )}
    </>
  );

  const splitOn = edit && layout !== "single";
  const vertical = layout === "top" || layout === "bottom";
  // Udział edytora; podgląd zajmuje resztę.
  const editorPane = (
    <div key="ed" className="split-pane split-editor" style={{ flexBasis: splitOn ? `${ratio * 100}%` : "100%" }}>
      {visual ? (
        <VisualEditor
          value={draft}
          notePath={path}
          summary={summary}
          onChange={onDraft}
          onSave={() => void flush()}
          onFiles={onFiles}
          onMarkdown={() => setVisual(false)}
        />
      ) : (
      <Editor
        value={draft}
        summary={summary}
        onChange={onDraft}
        onSave={() => void flush()}
        onFiles={onFiles}
        onScrollLine={onScrollLine}
      />
      )}
    </div>
  );
  const previewPane = splitOn ? (
    <div key="pv" ref={previewRef} className="split-pane split-preview" aria-label="Podgląd">
      {preview}
    </div>
  ) : null;
  const editorFirst = layout === "left" || layout === "top";
  const divider = splitOn ? (
    <div
      key="dv"
      className="split-divider"
      role="separator"
      aria-orientation={vertical ? "horizontal" : "vertical"}
      aria-valuenow={Math.round(ratio * 100)}
      aria-label="Proporcja edytora i podglądu"
      tabIndex={0}
      title="Przeciągnij, aby zmienić proporcje. Dwuklik: po równo."
      onPointerDown={(e) => {
        const box = splitRef.current?.getBoundingClientRect();
        if (!box) return;
        e.preventDefault();
        (e.target as HTMLElement).setPointerCapture(e.pointerId);
        setDragging(true);
        const at = (ev: PointerEvent) => {
          const f = vertical ? (ev.clientY - box.top) / box.height : (ev.clientX - box.left) / box.width;
          return editorFirst ? f : 1 - f;
        };
        const move = (ev: PointerEvent) => setRatio(at(ev), false);
        const up = (ev: PointerEvent) => {
          setRatio(at(ev));
          setDragging(false);
          window.removeEventListener("pointermove", move);
          window.removeEventListener("pointerup", up);
        };
        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", up);
      }}
      onDoubleClick={() => setRatio(0.5)}
      onKeyDown={(e) => {
        const grow = vertical ? { ArrowDown: 1, ArrowUp: -1 } : { ArrowRight: 1, ArrowLeft: -1 };
        const d = grow[e.key as keyof typeof grow];
        if (!d) return;
        e.preventDefault();
        setRatio(ratio + (editorFirst ? d : -d) * 0.05);
      }}
    />
  ) : null;

  return (
    <div className={`note-wrap${pdf ? " with-pdf" : ""}`} style={{ "--note-scale": scale } as CSSProperties}>
      {scaleFlash && (
        <button className="scale-badge" onClick={() => setScale(1)} title="Przywróć 100% (Ctrl+0)">
          {Math.round(scale * 100)}%
        </button>
      )}
      <div className={`note${edit ? " editing" : ""}${splitActive ? " wide" : ""}${dropping ? " dropping" : ""}`} ref={contentRef} {...dropProps}>
        {dropping && <div className="drop-hint">Upuść, aby dodać Załącznik na końcu Notatki</div>}
        <div className="note-head">
          <div className="crumbs">
            {inTrash && <span className="tag-name">Kosz ·&nbsp;</span>}
            {(crumbs[0] === ".kosz" ? crumbs.slice(1) : crumbs).length === 0
              ? "Skarbiec"
              : (crumbs[0] === ".kosz" ? crumbs.slice(1) : crumbs).map((c, i) => (
                  <span key={i}>
                    {i > 0 && <span className="sep">›</span>}
                    {c}
                  </span>
                ))}
          </div>
          <div className="note-title-row">
            {edit ? (
              <input
                className="note-title title-input"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                onBlur={() => void commitTitle()}
                onKeyDown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
                aria-label="Tytuł Notatki"
              />
            ) : (
              <h1 className="note-title">{note.title}</h1>
            )}
            <div className="note-actions">
              {edit && <span className={`save-state ${save}`}>{saveLabel}</span>}
              {edit && (
                <div className="mode-switch" role="radiogroup" aria-label="Rodzaj edytora">
                  <button role="radio" aria-checked={!visual} className={!visual ? "on" : ""} onClick={() => setVisual(false)}
                    title="Edycja tekstu Markdown z podglądem">
                    Markdown
                  </button>
                  <button role="radio" aria-checked={visual} className={visual ? "on" : ""} onClick={() => setVisual(true)}
                    title="Edycja wizualna (WYSIWYG), jak w edytorze tekstu">
                    Wizualny
                  </button>
                </div>
              )}
              {edit && !visual && (
                <div className="layout-picker" role="radiogroup" aria-label="Układ edytora i podglądu">
                  {LAYOUTS.map((l) => (
                    <button
                      key={l.id}
                      role="radio"
                      aria-checked={layout === l.id}
                      className={`icon-btn${layout === l.id ? " on" : ""}`}
                      title={l.label}
                      aria-label={l.label}
                      onClick={() => setLayout(l.id)}
                    >
                      <LayoutIcon layout={l.id} size={16} />
                    </button>
                  ))}
                </div>
              )}
              {inTrash ? (
                <button className="primary" onClick={restore}>
                  Przywróć z Kosza
                </button>
              ) : (
                <>
                  <button
                    className={`ghost toggle${noai ? " noai-on" : ""}`}
                    onClick={() => void toggleNoai()}
                    title={
                      noai
                        ? ownNoai
                          ? "Notatka jest ukryta przed AI (serwer MCP jej nie widzi). Kliknij, aby udostępnić."
                          : "Ukryta przed AI przez Tag #noai na Notatce folderu."
                        : "Ukryj Notatkę przed AI: serwer MCP nie pokaże jej Claude (dodaje Tag noai)."
                    }
                  >
                    {noai ? <LockIcon size={15} /> : <UnlockIcon size={15} />}
                    {noai ? "Ukryta przed AI" : "Widoczna dla AI"}
                  </button>
                  <button className={edit ? "primary" : ""} onClick={() => void toggleEdit()} title="Ctrl+E">
                    {edit ? <CheckIcon size={15} /> : <PencilIcon size={15} />}
                    {edit ? "Gotowe" : "Edytuj"}
                  </button>
                  <button
                    className="icon-btn danger"
                    onClick={() => void toTrash()}
                    title="Przenieś do Kosza"
                    aria-label="Przenieś do Kosza"
                  >
                    <TrashIcon />
                  </button>
                </>
              )}
            </div>
          </div>
          {!edit && links && links.tags.length > 0 && (
            <div className="note-tags">
              {links.tags.map((t) => (
                <button key={t} className="chip" onClick={() => onTag(t)}>
                  #{t}
                </button>
              ))}
            </div>
          )}
        </div>
        {edit ? (
          <div
            ref={splitRef}
            className={`split split-${layout}${dragging ? " dragging" : ""}`}
            style={{ "--split": ratio } as CSSProperties}
          >
            {layout === "right" || layout === "bottom" ? [previewPane, divider, editorPane] : [editorPane, divider, previewPane]}
          </div>
        ) : (
          preview
        )}
      </div>
      {pdf && (
        <div className="pdf-pane">
          <div className="pdf-bar">
            <span className="pdf-name" title={pdf}>
              {fileName(pdf)}
            </span>
            <button onClick={() => api.openVaultFile(pdf).catch((e) => onError(String(e)))}>Otwórz w programie</button>
            <button className="icon-btn" onClick={() => setPdf(null)} aria-label="Zamknij podgląd" title="Zamknij podgląd">
              <XIcon />
            </button>
          </div>
          <iframe className="pdf-frame" src={vaultUrl(pdf)} title={fileName(pdf)} />
        </div>
      )}
    </div>
  );
}
