// Edytor wizualny (WYSIWYG) Notatki: TipTap na HTML z Markdownu, zapis z powrotem do Markdownu (lib/mdHtml).
import { useEffect, useMemo, useRef, useState } from "react";
import { EditorContent, useEditor, useEditorState, type Editor as TEditor } from "@tiptap/react";
import { Extension, type Range } from "@tiptap/core";
import { PluginKey } from "@tiptap/pm/state";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import { TableKit, TableCell, TableHeader } from "@tiptap/extension-table";
import { TextStyle, Color, BackgroundColor } from "@tiptap/extension-text-style";
import { TextAlign } from "@tiptap/extension-text-align";
import { TaskList, TaskItem } from "@tiptap/extension-list";
import { Placeholder } from "@tiptap/extensions";
import Suggestion, { type SuggestionProps, type SuggestionKeyDownProps } from "@tiptap/suggestion";
import type { Summary } from "../lib/api";
import { filesOf } from "../lib/attach";
import { DATE_KEY, DATETIME_KEY, dateText, dateTimeText } from "../lib/dateText";
import { editorHtmlToMd, mdToEditorHtml, sameRendering } from "../lib/mdHtml";

type Props = {
  value: string;
  notePath: string;
  summary: Summary | null;
  onChange: (value: string) => void;
  onSave: () => void;
  onFiles?: (files: File[], pasted: boolean) => Promise<string>;
  /** Przejście do edytora Markdown (np. gdy edytor wizualny nie zachowa formatowania). */
  onMarkdown: () => void;
};

// ---------- rozszerzenia ----------

/** Tło komórki z Importu (`style="background:…"`). */
const cellBackground = {
  background: {
    default: null as string | null,
    parseHTML: (el: HTMLElement) => el.style.backgroundColor || el.style.background || null,
    renderHTML: (a: Record<string, unknown>) =>
      a.background ? { style: `background:${a.background}`, "data-background": a.background } : {},
  },
};
const Cell = TableCell.extend({
  addAttributes() {
    return { ...this.parent?.(), ...cellBackground };
  },
});
const Header = TableHeader.extend({
  addAttributes() {
    return { ...this.parent?.(), ...cellBackground };
  },
});

/** Obraz pamięta adres z Notatki; edytor pokazuje go przez vault://. */
const NoteImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      dataSrc: {
        default: null,
        parseHTML: (el: HTMLElement) => el.getAttribute("data-src"),
        renderHTML: (a: Record<string, unknown>) => (a.dataSrc ? { "data-src": a.dataSrc } : {}),
      },
    };
  },
}).configure({ inline: true });

function norm(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

type Item = { label: string; detail: string; insert: string };

/** Lista podpowiedzi pod kursorem (bez Reacta, żyje tyle co podpowiedź). */
function popup() {
  let el: HTMLDivElement | null = null;
  let items: Item[] = [];
  let index = 0;
  let pick: (item: Item) => void = () => {};
  const draw = () => {
    if (!el) return;
    el.innerHTML = "";
    if (!items.length) {
      el.style.display = "none";
      return;
    }
    el.style.display = "";
    items.forEach((it, i) => {
      const row = document.createElement("div");
      row.className = `ve-suggest-item${i === index ? " on" : ""}`;
      const label = document.createElement("span");
      label.textContent = it.label;
      const detail = document.createElement("span");
      detail.className = "ve-suggest-detail";
      detail.textContent = it.detail;
      row.append(label, detail);
      row.onmousedown = (e) => {
        e.preventDefault();
        pick(it);
      };
      el!.append(row);
    });
    el.querySelector(".on")?.scrollIntoView({ block: "nearest" });
  };
  const place = (props: SuggestionProps<Item>) => {
    const r = props.clientRect?.();
    if (!el || !r) return;
    el.style.left = `${r.left}px`;
    el.style.top = `${r.bottom + 4}px`;
  };
  return {
    onStart: (props: SuggestionProps<Item>) => {
      el = document.createElement("div");
      el.className = "ve-suggest";
      document.body.append(el);
      items = props.items;
      index = 0;
      pick = (it) => props.command(it);
      place(props);
      draw();
    },
    onUpdate: (props: SuggestionProps<Item>) => {
      items = props.items;
      index = 0;
      pick = (it) => props.command(it);
      place(props);
      draw();
    },
    onKeyDown: ({ event }: SuggestionKeyDownProps) => {
      if (!items.length) return false;
      if (event.key === "ArrowDown") index = (index + 1) % items.length;
      else if (event.key === "ArrowUp") index = (index - 1 + items.length) % items.length;
      else if (event.key === "Enter" || event.key === "Tab") {
        pick(items[index]);
        return true;
      } else if (event.key === "Escape") {
        items = [];
        draw();
        return true;
      } else return false;
      draw();
      return true;
    },
    onExit: () => {
      el?.remove();
      el = null;
    },
  };
}

/** Podpowiedzi `[[` (Notatki) i `#` (Tagi), jak w edytorze Markdown. */
function suggestions(getSummary: () => Summary | null) {
  const insert = ({ editor, range, props }: { editor: TEditor; range: Range; props: Item }) => {
    editor.chain().focus().insertContentAt(range, props.insert).run();
  };
  return Extension.create({
    name: "noteSuggestions",
    addProseMirrorPlugins() {
      return [
        Suggestion<Item>({
          editor: this.editor,
          pluginKey: new PluginKey("wikiSuggest"),
          char: "[[",
          allowSpaces: true,
          startOfLine: false,
          allowedPrefixes: null,
          items: ({ query }) => {
            const s = getSummary();
            if (!s) return [];
            const q = norm(query);
            const stems = new Map<string, number>();
            for (const n of s.notes) stems.set(n.stem.toLowerCase(), (stems.get(n.stem.toLowerCase()) ?? 0) + 1);
            return s.notes
              .filter((n) => !q || norm(n.title).includes(q) || norm(n.stem).includes(q) || norm(n.path).includes(q))
              .slice(0, 50)
              .map((n) => {
                const target = (stems.get(n.stem.toLowerCase()) ?? 0) > 1 ? n.path.replace(/\.md$/i, "") : n.stem;
                const text = target === n.title ? target : `${target}|${n.title}`;
                return { label: n.title, detail: n.path, insert: `[[${text}]]` };
              });
          },
          command: insert,
          render: popup,
        }),
        Suggestion<Item>({
          editor: this.editor,
          pluginKey: new PluginKey("tagSuggest"),
          char: "#",
          allowSpaces: false,
          items: ({ query }) => {
            const s = getSummary();
            if (!s) return [];
            const q = norm(query);
            return s.tags
              .filter((t) => !q || norm(t.tag).includes(q))
              .slice(0, 50)
              .map((t) => ({ label: t.tag, detail: String(t.count), insert: `#${t.tag} ` }));
          },
          command: insert,
          render: popup,
        }),
      ];
    },
  });
}

// ---------- pasek narzędzi ----------

const COLORS = ["#F02828", "#E67E22", "#2E9E44", "#2D6CDF", "#8E44AD", "#7F8C8D"];
const MARKS = ["#FFFF00", "#B5F5B5", "#BDE0FF", "#FFD1DC"];

function Btn({ on, title, onClick, children, disabled }: { on?: boolean; title: string; onClick: () => void; children: React.ReactNode; disabled?: boolean }) {
  return (
    <button
      type="button"
      className={`ve-btn${on ? " on" : ""}`}
      title={title}
      aria-label={title}
      aria-pressed={on}
      disabled={disabled}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function Toolbar({ editor }: { editor: TEditor }) {
  const s = useEditorState({
    editor,
    selector: ({ editor: e }) => ({
      bold: e.isActive("bold"),
      italic: e.isActive("italic"),
      underline: e.isActive("underline"),
      strike: e.isActive("strike"),
      code: e.isActive("code"),
      h1: e.isActive("heading", { level: 1 }),
      h2: e.isActive("heading", { level: 2 }),
      h3: e.isActive("heading", { level: 3 }),
      bullet: e.isActive("bulletList"),
      ordered: e.isActive("orderedList"),
      task: e.isActive("taskList"),
      quote: e.isActive("blockquote"),
      codeBlock: e.isActive("codeBlock"),
      link: e.isActive("link"),
      table: e.isActive("table"),
      center: e.isActive({ textAlign: "center" }),
      right: e.isActive({ textAlign: "right" }),
      undo: e.can().undo(),
      redo: e.can().redo(),
    }),
  });
  const [linkOpen, setLinkOpen] = useState(false);
  const [href, setHref] = useState("");
  const c = () => editor.chain().focus();

  const openLink = () => {
    if (s.link) {
      c().extendMarkRange("link").unsetLink().run();
      return;
    }
    setHref("https://");
    setLinkOpen(true);
  };
  const applyLink = () => {
    const h = href.trim();
    if (h && h !== "https://") {
      if (editor.state.selection.empty) c().insertContent({ type: "text", text: h, marks: [{ type: "link", attrs: { href: h } }] }).run();
      else c().extendMarkRange("link").setLink({ href: h }).run();
    }
    setLinkOpen(false);
  };

  return (
    <div className="ve-toolbar" role="toolbar" aria-label="Formatowanie">
      <Btn title="Cofnij (Ctrl+Z)" disabled={!s.undo} onClick={() => c().undo().run()}>↶</Btn>
      <Btn title="Ponów (Ctrl+Y)" disabled={!s.redo} onClick={() => c().redo().run()}>↷</Btn>
      <span className="ve-sep" />
      <Btn title="Nagłówek 1" on={s.h1} onClick={() => c().toggleHeading({ level: 1 }).run()}>H1</Btn>
      <Btn title="Nagłówek 2" on={s.h2} onClick={() => c().toggleHeading({ level: 2 }).run()}>H2</Btn>
      <Btn title="Nagłówek 3" on={s.h3} onClick={() => c().toggleHeading({ level: 3 }).run()}>H3</Btn>
      <span className="ve-sep" />
      <Btn title="Pogrubienie (Ctrl+B)" on={s.bold} onClick={() => c().toggleBold().run()}><b>B</b></Btn>
      <Btn title="Kursywa (Ctrl+I)" on={s.italic} onClick={() => c().toggleItalic().run()}><i>I</i></Btn>
      <Btn title="Podkreślenie (Ctrl+U)" on={s.underline} onClick={() => c().toggleUnderline().run()}><u>U</u></Btn>
      <Btn title="Przekreślenie" on={s.strike} onClick={() => c().toggleStrike().run()}><s>S</s></Btn>
      <Btn title="Kod" on={s.code} onClick={() => c().toggleCode().run()}>{"</>"}</Btn>
      <span className="ve-sep" />
      <span className="ve-swatches" title="Kolor tekstu">
        {COLORS.map((col) => (
          <button key={col} type="button" className="ve-swatch" style={{ color: col }} title={`Kolor ${col}`} aria-label={`Kolor tekstu ${col}`}
            onMouseDown={(e) => e.preventDefault()} onClick={() => c().setColor(col).run()}>A</button>
        ))}
        {MARKS.map((col) => (
          <button key={col} type="button" className="ve-swatch mark" style={{ background: col }} title={`Wyróżnienie ${col}`} aria-label={`Tło tekstu ${col}`}
            onMouseDown={(e) => e.preventDefault()} onClick={() => c().setBackgroundColor(col).run()} />
        ))}
        <Btn title="Usuń kolor i wyróżnienie" onClick={() => c().unsetColor().unsetBackgroundColor().removeEmptyTextStyle().run()}>⌀</Btn>
      </span>
      <span className="ve-sep" />
      <Btn title="Lista punktowana" on={s.bullet} onClick={() => c().toggleBulletList().run()}>•≡</Btn>
      <Btn title="Lista numerowana" on={s.ordered} onClick={() => c().toggleOrderedList().run()}>1.</Btn>
      <Btn title="Lista zadań" on={s.task} onClick={() => c().toggleTaskList().run()}>☑</Btn>
      <Btn title="Cytat" on={s.quote} onClick={() => c().toggleBlockquote().run()}>❝</Btn>
      <Btn title="Blok kodu" on={s.codeBlock} onClick={() => c().toggleCodeBlock().run()}>{"{ }"}</Btn>
      <Btn title="Linia pozioma" onClick={() => c().setHorizontalRule().run()}>―</Btn>
      <span className="ve-sep" />
      <Btn title="Do lewej" on={!s.center && !s.right} onClick={() => c().unsetTextAlign().run()}>⫷</Btn>
      <Btn title="Do środka" on={s.center} onClick={() => c().setTextAlign("center").run()}>≡</Btn>
      <Btn title="Do prawej" on={s.right} onClick={() => c().setTextAlign("right").run()}>⫸</Btn>
      <span className="ve-sep" />
      <Btn title={s.link ? "Usuń link" : "Link do strony"} on={s.link} onClick={openLink}>🔗</Btn>
      <Btn title="Wstaw Link do Notatki ([[)" onClick={() => c().insertContent("[[").run()}>[[ ]]</Btn>
      <span className="ve-sep" />
      <Btn title="Wstaw datę (Ctrl+;)" onClick={() => c().insertContent(dateText()).run()}>📅</Btn>
      <Btn title="Wstaw datę i godzinę (Ctrl+Shift+;)" onClick={() => c().insertContent(dateTimeText()).run()}>🕒</Btn>
      <span className="ve-sep" />
      {!s.table ? (
        <Btn title="Wstaw tabelę" onClick={() => c().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()}>▦</Btn>
      ) : (
        <>
          <Btn title="Wiersz poniżej" onClick={() => c().addRowAfter().run()}>+w</Btn>
          <Btn title="Kolumna z prawej" onClick={() => c().addColumnAfter().run()}>+k</Btn>
          <Btn title="Usuń wiersz" onClick={() => c().deleteRow().run()}>−w</Btn>
          <Btn title="Usuń kolumnę" onClick={() => c().deleteColumn().run()}>−k</Btn>
          <Btn title="Scal / rozdziel komórki" onClick={() => c().mergeOrSplit().run()}>⊞</Btn>
          <Btn title="Usuń tabelę" onClick={() => c().deleteTable().run()}>✕▦</Btn>
        </>
      )}
      {linkOpen && (
        <span className="ve-link">
          <input
            autoFocus
            value={href}
            onChange={(e) => setHref(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") applyLink();
              if (e.key === "Escape") setLinkOpen(false);
            }}
            aria-label="Adres linku"
          />
          <button type="button" className="primary" onClick={applyLink}>OK</button>
        </span>
      )}
    </div>
  );
}

// ---------- edytor ----------

export function VisualEditor({ value, notePath, summary, onChange, onSave, onFiles, onMarkdown }: Props) {
  const latest = useRef({ summary, onChange, onSave, onFiles, notePath });
  latest.current = { summary, onChange, onSave, onFiles, notePath };
  const trailing = useRef(value.endsWith("\n"));
  const original = useRef(value);
  const emitted = useRef(value);
  const baseline = useRef<string | null>(null);
  const initialHtml = useMemo(() => mdToEditorHtml(value, notePath), []); // eslint-disable-line react-hooks/exhaustive-deps
  const [lossy, setLossy] = useState(false);

  const insertFiles = (editor: TEditor, files: File[], pasted: boolean, pos: number) => {
    const handler = latest.current.onFiles;
    if (!handler) return;
    void handler(files, pasted).then((md) => {
      if (md) editor.chain().focus().insertContentAt(pos, mdToEditorHtml(md, latest.current.notePath)).run();
    });
  };

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ link: { openOnClick: false, autolink: true } }),
      TextStyle,
      Color,
      BackgroundColor,
      TextAlign.configure({ types: ["heading", "paragraph"] }),
      NoteImage,
      TableKit.configure({ table: { resizable: false }, tableCell: false, tableHeader: false }),
      Cell,
      Header,
      TaskList,
      TaskItem.configure({ nested: true }),
      Placeholder.configure({ placeholder: "Pisz. [[ wstawia Link do Notatki, # dodaje Tag. Zrzut ekranu wklej Ctrl+V, pliki przeciągnij." }),
      suggestions(() => latest.current.summary),
      Extension.create({
        name: "saveKey",
        addKeyboardShortcuts: () => ({
          [DATE_KEY]: ({ editor: e }) => e.commands.insertContent(dateText()),
          [DATETIME_KEY]: ({ editor: e }) => e.commands.insertContent(dateTimeText()),
          "Mod-s": () => {
            latest.current.onSave();
            return true;
          },
        }),
      }),
    ],
    content: initialHtml,
    autofocus: true,
    immediatelyRender: true,
    shouldRerenderOnTransaction: false,
    editorProps: {
      handlePaste: (view, event) => {
        const files = filesOf(event.clipboardData);
        if (!files.length || !latest.current.onFiles || !editorRef.current) return false;
        event.preventDefault();
        insertFiles(editorRef.current, files, true, view.state.selection.head);
        return true;
      },
      handleDrop: (view, event) => {
        const files = filesOf(event.dataTransfer);
        if (!files.length || !latest.current.onFiles || !editorRef.current) return false;
        event.preventDefault();
        const pos = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos ?? view.state.selection.head;
        insertFiles(editorRef.current, files, false, pos);
        return true;
      },
    },
    onCreate: ({ editor: e }) => {
      baseline.current ??= editorHtmlToMd(e.getHTML(), trailing.current);
    },
    // TipTap poprawia dokument przy starcie (np. pusty akapit za tabelą): to nie jest zmiana Notatki.
    // Zapis tylko wtedy, gdy treść różni się od stanu po wczytaniu; powrót do niego oddaje oryginalny plik.
    onUpdate: ({ editor: e }) => {
      const md = editorHtmlToMd(e.getHTML(), trailing.current);
      if (baseline.current === null) {
        baseline.current = md;
        return;
      }
      const out = md === baseline.current ? original.current : md;
      if (out === emitted.current) return;
      emitted.current = out;
      latest.current.onChange(out);
    },
  });
  const editorRef = useRef<TEditor | null>(null);
  editorRef.current = editor;

  // Czy zapis z tego edytora odda tę samą Notatkę; jeśli nie, ostrzeżenie (zapis i tak dopiero po zmianie).
  useEffect(() => {
    if (!editor) return;
    const md = baseline.current ?? editorHtmlToMd(editor.getHTML(), trailing.current);
    setLossy(!sameRendering(value, md));
  }, [editor]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!editor) return null;
  return (
    <div className="visual-editor">
      <Toolbar editor={editor} />
      {lossy && (
        <div className="banner warn ve-warn">
          Edytor wizualny nie odda dokładnie całego formatowania tej Notatki (np. HTML, którego nie obsługuje). Dopóki
          niczego nie zmienisz, plik zostaje nietknięty.
          <button type="button" onClick={onMarkdown}>Edytuj w Markdownie</button>
        </div>
      )}
      <EditorContent editor={editor} className="ve-content markdown" />
    </div>
  );
}
