import { useEffect, useRef } from "react";
import { EditorView, keymap, placeholder } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { basicSetup } from "codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { autocompletion, type Completion, type CompletionContext } from "@codemirror/autocomplete";
import type { Summary } from "../lib/api";
import { filesOf } from "../lib/attach";

type Props = {
  value: string;
  summary: Summary | null;
  onChange: (value: string) => void;
  onSave: () => void;
  /** Wklejone albo upuszczone pliki: zapisuje je jako Załączniki i zwraca Markdown do wstawienia. */
  onFiles?: (files: File[], pasted: boolean) => Promise<string>;
  /**
   * Miejsce pracy w edytorze: linia (ułamkowa, od 1) z kursorem albo ze środka widoku, gdy kursora nie widać,
   * jej odległość w px od góry widoku i czy edytor przewinięto do końca.
   */
  onScrollLine?: (line: number, offset: number, atEnd: boolean) => void;
};

function norm(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Podpowiedzi po wpisaniu `[[` (Notatki) i `#` (istniejące Tagi). */
function completions(getSummary: () => Summary | null) {
  return (ctx: CompletionContext) => {
    const summary = getSummary();
    if (!summary) return null;
    const link = ctx.matchBefore(/\[\[[^\]\n|#]*/);
    if (link) {
      const q = norm(link.text.slice(2));
      const stems = new Map<string, number>();
      for (const n of summary.notes) stems.set(n.stem.toLowerCase(), (stems.get(n.stem.toLowerCase()) ?? 0) + 1);
      const after = ctx.state.sliceDoc(ctx.pos, ctx.pos + 2);
      const close = after === "]]" ? "" : "]]";
      const options: Completion[] = summary.notes
        .filter((n) => !q || norm(n.title).includes(q) || norm(n.stem).includes(q) || norm(n.path).includes(q))
        .slice(0, 200)
        .map((n) => {
          // Nazwa pliku wystarcza, gdy jest jedyna; inaczej ścieżka bez `.md`.
          const target = (stems.get(n.stem.toLowerCase()) ?? 0) > 1 ? n.path.replace(/\.md$/i, "") : n.stem;
          const text = target === n.title ? target : `${target}|${n.title}`;
          return { label: n.title, detail: n.path, apply: text + close, type: "text" };
        });
      return { from: link.from + 2, options, filter: false };
    }
    const tag = ctx.matchBefore(/(^|[\s(])#[\p{L}\p{N}_/-]*/u);
    if (tag) {
      const hash = tag.text.indexOf("#");
      const q = norm(tag.text.slice(hash + 1));
      const options: Completion[] = summary.tags
        .filter((t) => !q || norm(t.tag).includes(q))
        .map((t) => ({ label: t.tag, detail: String(t.count), type: "keyword" }));
      if (!options.length) return null;
      return { from: tag.from + hash + 1, options, filter: false };
    }
    return null;
  };
}

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "calc(14px * var(--note-scale, 1))", backgroundColor: "var(--surface)", color: "var(--fg)" },
  ".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "1.6" },
  ".cm-content": { caretColor: "var(--fg)", padding: "12px 0" },
  ".cm-gutters": { backgroundColor: "var(--bg-side)", color: "var(--muted)", border: "none" },
  ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--accent-softer)" },
  "&.cm-focused": { outline: "none" },
  ".cm-tooltip": { backgroundColor: "var(--bg)", border: "1px solid var(--border)", color: "var(--fg)" },
  ".cm-tooltip-autocomplete ul li[aria-selected]": { backgroundColor: "var(--accent-soft)", color: "var(--fg)" },
  ".cm-completionDetail": { color: "var(--muted)", marginLeft: "1em", fontStyle: "normal" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": { backgroundColor: "var(--accent-soft) !important" },
  ".cm-cursor": { borderLeftColor: "var(--fg)" },
  ".cm-searchMatch": { backgroundColor: "var(--mark)" },
  ".cm-panels": { backgroundColor: "var(--bg-side)", color: "var(--fg)", borderColor: "var(--border)" },
});

/** Kolory składni ze zmiennych motywu, żeby były czytelne w trybie jasnym i ciemnym. */
const highlight = HighlightStyle.define([
  { tag: t.heading, fontWeight: "700", color: "var(--fg)" },
  { tag: t.heading1, fontSize: "1.25em" },
  { tag: t.heading2, fontSize: "1.12em" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: [t.link, t.url], color: "var(--accent)" },
  { tag: t.monospace, color: "var(--code-fg)" },
  { tag: t.quote, color: "var(--muted)", fontStyle: "italic" },
  { tag: [t.processingInstruction, t.meta, t.contentSeparator], color: "var(--faint)" },
  { tag: [t.angleBracket, t.tagName, t.attributeName], color: "var(--faint)" },
  { tag: t.attributeValue, color: "var(--tag)" },
  { tag: [t.keyword, t.operatorKeyword], color: "var(--accent)" },
  { tag: [t.string, t.special(t.string)], color: "var(--ok-fg)" },
  { tag: [t.number, t.bool, t.null], color: "var(--tag)" },
  { tag: t.comment, color: "var(--faint)", fontStyle: "italic" },
]);

/** Wstawia Markdown Załączników w `pos`, w osobnych liniach. */
function insertBlock(v: EditorView, pos: number, md: string) {
  if (!md) return;
  const line = v.state.doc.lineAt(pos);
  const before = pos > line.from ? "\n" : "";
  const after = pos < line.to ? "\n" : "";
  const text = before + md + after;
  v.dispatch({ changes: { from: pos, insert: text }, selection: { anchor: pos + text.length } });
  v.focus();
}

export function Editor({ value, summary, onChange, onSave, onFiles, onScrollLine }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const syncRef = useRef<() => void>(() => {});
  // Aktualne wartości dla rozszerzeń utworzonych raz.
  const latest = useRef({ summary, onChange, onSave, onFiles, onScrollLine });
  latest.current = { summary, onChange, onSave, onFiles, onScrollLine };

  useEffect(() => {
    const v = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          markdown(),
          EditorView.lineWrapping,
          placeholder("Pisz w Markdownie. [[ wstawia Link do Notatki, # dodaje Tag. Zrzut ekranu wklej Ctrl+V, pliki przeciągnij."),
          autocompletion({ override: [completions(() => latest.current.summary)], activateOnTyping: true }),
          keymap.of([
            {
              key: "Mod-s",
              preventDefault: true,
              run: () => {
                latest.current.onSave();
                return true;
              },
            },
          ]),
          EditorView.domEventHandlers({
            paste: (e, v) => {
              const files = filesOf(e.clipboardData);
              const handler = latest.current.onFiles;
              if (!files.length || !handler) return false;
              e.preventDefault();
              const pos = v.state.selection.main.head;
              void handler(files, true).then((md) => insertBlock(v, pos, md));
              return true;
            },
            dragover: (e) => {
              if (!e.dataTransfer?.types.includes("Files")) return false;
              e.preventDefault();
              e.dataTransfer.dropEffect = "copy";
              return true;
            },
            drop: (e, v) => {
              const files = filesOf(e.dataTransfer);
              const handler = latest.current.onFiles;
              if (!files.length || !handler) return false;
              e.preventDefault();
              const pos = v.posAtCoords({ x: e.clientX, y: e.clientY }) ?? v.state.selection.main.head;
              void handler(files, false).then((md) => insertBlock(v, pos, md));
              return true;
            },
          }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) latest.current.onChange(u.state.doc.toString());
            // Układu nie wolno czytać w trakcie aktualizacji: po niej.
            if (u.docChanged || u.selectionSet || u.geometryChanged) window.setTimeout(() => syncRef.current(), 0);
          }),
          theme,
          syntaxHighlighting(highlight),
        ],
      }),
    });
    view.current = v;
    v.focus();
    const sc = v.scrollDOM;
    const onScroll = () => {
      const cb = latest.current.onScrollLine;
      if (!cb || !sc.isConnected) return;
      const viewTop = sc.getBoundingClientRect().top - v.documentTop;
      const h = sc.clientHeight;
      const head = v.state.selection.main.head;
      let blk = v.lineBlockAt(head);
      let line = v.state.doc.lineAt(head).number;
      let part = 0;
      if (blk.top < viewTop || blk.bottom > viewTop + h) {
        const mid = viewTop + h / 2;
        blk = v.lineBlockAtHeight(mid);
        line = v.state.doc.lineAt(blk.from).number;
        part = blk.height > 0 ? Math.min(1, Math.max(0, (mid - blk.top) / blk.height)) : 0;
      }
      const scrollable = sc.scrollHeight > sc.clientHeight + 2;
      cb(line + part, blk.top + part * blk.height - viewTop, scrollable && sc.scrollTop + sc.clientHeight >= sc.scrollHeight - 2);
    };
    syncRef.current = onScroll;
    sc.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      sc.removeEventListener("scroll", onScroll);
      syncRef.current = () => {};
      v.destroy();
    };
    // Edytor tworzymy raz na Notatkę (komponent ma key = ścieżka).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div className="editor" ref={host} />;
}
