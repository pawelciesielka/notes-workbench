/** Przewijanie podglądu razem z edytorem: bloki podglądu znają numer linii źródła. */

type HastNode = {
  type: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  position?: { start: { line: number } };
  children?: HastNode[];
};

const BLOCKS = new Set([
  "p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "pre", "blockquote", "table", "tr", "hr", "img", "div", "details", "figure",
]);

/** Wtyczka rehype: `data-line` (linia w Markdownie, od 1) na elementach blokowych. */
export function rehypeSourceLines() {
  return (tree: HastNode) => {
    const walk = (n: HastNode) => {
      if (n.type === "element" && n.position && BLOCKS.has(n.tagName ?? "")) {
        const pos = n.position as { start: { line: number }; end?: { line: number } };
        n.properties = { ...n.properties, dataLine: pos.start.line, dataLineEnd: pos.end?.line ?? pos.start.line };
      }
      n.children?.forEach(walk);
    };
    walk(tree);
  };
}

/** Położenie (px od góry treści `pane`) linii źródła `line`, z blokami wielolinijkowymi. */
function lineY(pane: HTMLElement, line: number): number | null {
  const els = Array.from(pane.querySelectorAll<HTMLElement>("[data-line]"));
  if (!els.length) return null;
  const base = pane.getBoundingClientRect().top - pane.scrollTop;
  const blocks = els
    .map((el) => {
      const r = el.getBoundingClientRect();
      const start = Number(el.dataset.line);
      return { start, end: Math.max(start, Number(el.dataset.lineEnd) || start), top: r.top - base, bottom: r.bottom - base };
    })
    .sort((a, b) => a.start - b.start || a.top - b.top);
  let a: (typeof blocks)[number] | null = null;
  let next: (typeof blocks)[number] | null = null;
  for (const bl of blocks) {
    if (bl.start <= line) a = bl;
    else {
      next = bl;
      break;
    }
  }
  if (!a) return 0;
  // Wewnątrz bloku (akapit z wieloma liniami, kod, tabela): proporcjonalnie do wysokości.
  if (line < a.end + 1) return a.top + ((line - a.start) / (a.end - a.start + 1)) * (a.bottom - a.top);
  // Między blokami (puste linie): od dołu poprzedniego do góry następnego.
  if (next && next.start > a.end + 1) return a.bottom + ((line - a.end - 1) / (next.start - a.end - 1)) * (next.top - a.bottom);
  return next ? next.top : a.bottom;
}

/**
 * Przewija `pane` tak, by linia źródła `line` była `offset` px od góry, jak w edytorze.
 * `atEnd`: edytor przewinięty do końca, więc podgląd też.
 */
export function scrollPreviewTo(pane: HTMLElement, line: number, offset: number, atEnd: boolean) {
  if (atEnd) {
    pane.scrollTop = pane.scrollHeight;
    return;
  }
  const y = lineY(pane, line);
  if (y !== null) pane.scrollTop = Math.max(0, y - offset);
}
