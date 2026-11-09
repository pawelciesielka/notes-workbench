import { useCallback, useEffect, useRef, useState, type ComponentProps } from "react";
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
  const inTrash = path.startsWith(TRASH);

  // Stan do zapisu poza cyklem renderowania (debounce, zamknięcie Notatki).
  const ref = useRef({ raw: "", draft: "", dirty: false, timer: 0 as number | undefined });

  const [similar, setSimilar] = useState<NoteRef[]>([]);

  const loadLinks = useCallback(() => {
    api
      .noteLinks(path)
      .then(setLinks)
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
      setNote(n);
      setSave(r.dirty ? "dirty" : "saved");
      onChanged(false);
    } catch (e) {
      r.dirty = true;
      setSave("error");
      onError(String(e));
    }
  }, [path, onChanged, onError]);

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
  const shownBody = links && !edit ? linkify(note.body, links.spans) : note.body;
  const saveLabel = { saved: "Zapisano", dirty: "Zmiany…", saving: "Zapisywanie…", error: "Błąd zapisu" }[save];

  return (
    <div className={`note-wrap${pdf ? " with-pdf" : ""}`}>
      <div className={`note${edit ? " editing" : ""}`}>
        <div className="note-head">
          <div className="crumbs">{crumbs.length ? crumbs.join(" / ") : "Skarbiec"}</div>
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
            {edit && <span className={`save-state ${save}`}>{saveLabel}</span>}
            {inTrash ? (
              <button onClick={restore}>Przywróć z Kosza</button>
            ) : (
              <>
                <button
                  className={`toggle${noai ? " on noai-on" : ""}`}
                  onClick={() => void toggleNoai()}
                  title={
                    noai
                      ? ownNoai
                        ? "Notatka jest ukryta przed AI (serwer MCP jej nie widzi). Kliknij, aby udostępnić."
                        : "Ukryta przed AI przez Tag #noai na Notatce folderu."
                      : "Ukryj Notatkę przed AI: serwer MCP nie pokaże jej Claude (dodaje Tag noai)."
                  }
                >
                  {noai ? "🔒 Ukryta przed AI" : "Ukryj przed AI"}
                </button>
                <button className={`toggle${edit ? " on" : ""}`} onClick={() => void toggleEdit()} title="Ctrl+E">
                  {edit ? "Gotowe" : "Edytuj"}
                </button>
                <button onClick={() => void toTrash()} title="Przenieś do Kosza">
                  🗑
                </button>
              </>
            )}
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
          <Editor value={draft} summary={summary} onChange={onDraft} onSave={() => void flush()} />
        ) : (
          <>
            {note.body.trim() === "" ? (
              <div className="muted empty-note">
                Pusta Notatka. <button onClick={() => void toggleEdit()}>Edytuj</button>
              </div>
            ) : (
              <div className="markdown">
                <Markdown
                  remarkPlugins={[remarkGfm, remarkBreaks]}
                  rehypePlugins={[rehypeRaw]}
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
                    <h2>Powiązane ({links.related.length})</h2>
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
                    <h2>Linki do tej Notatki ({links.backlinks.length})</h2>
                    <ul>
                      {links.backlinks.map((b) => (
                        <li key={b.path}>
                          <a href="#" className="wikilink" onClick={(e) => (e.preventDefault(), onOpen(b.path))}>
                            {b.title}
                          </a>
                          {b.related && <span className="muted"> (Powiązanie)</span>}
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
        )}
      </div>
      {pdf && (
        <div className="pdf-pane">
          <div className="pdf-bar">
            <span className="pdf-name" title={pdf}>
              {fileName(pdf)}
            </span>
            <button onClick={() => api.openVaultFile(pdf).catch((e) => onError(String(e)))}>Otwórz w programie</button>
            <button onClick={() => setPdf(null)} aria-label="Zamknij podgląd">
              ✕
            </button>
          </div>
          <iframe className="pdf-frame" src={vaultUrl(pdf)} title={fileName(pdf)} />
        </div>
      )}
    </div>
  );
}
