import { useEffect, useRef } from "react";
import { EditorView, keymap, placeholder } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { basicSetup } from "codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { autocompletion, type Completion, type CompletionContext } from "@codemirror/autocomplete";
import type { Summary } from "../lib/api";

type Props = {
  value: string;
  summary: Summary | null;
  onChange: (value: string) => void;
  onSave: () => void;
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
  "&": { height: "100%", fontSize: "14px", backgroundColor: "var(--surface)", color: "var(--fg)" },
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

export function Editor({ value, summary, onChange, onSave }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  // Aktualne wartości dla rozszerzeń utworzonych raz.
  const latest = useRef({ summary, onChange, onSave });
  latest.current = { summary, onChange, onSave };

  useEffect(() => {
    const v = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          markdown(),
          EditorView.lineWrapping,
          placeholder("Pisz w Markdownie. [[ wstawia Link do Notatki, # dodaje Tag."),
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
          EditorView.updateListener.of((u) => {
            if (u.docChanged) latest.current.onChange(u.state.doc.toString());
          }),
          theme,
          syntaxHighlighting(highlight),
        ],
      }),
    });
    view.current = v;
    v.focus();
    return () => v.destroy();
    // Edytor tworzymy raz na Notatkę (komponent ma key = ścieżka).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div className="editor" ref={host} />;
}
