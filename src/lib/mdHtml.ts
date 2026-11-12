// Edytor wizualny: Markdown Notatki ↔ HTML edytora (TipTap).
// Markdown → HTML tym samym potokiem co podgląd (GFM, łamanie linii, wstawki HTML; ADR 0001).
// HTML → Markdown przez Turndown z regułami, które zapisują formatowanie AMNO tak jak Import:
// `<span style="color:…;background:…">`, `<u>`, `<div align>`, tabele HTML ze scalonymi komórkami.
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkBreaks from "remark-breaks";
import remarkRehype from "remark-rehype";
import rehypeRaw from "rehype-raw";
import rehypeStringify from "rehype-stringify";
import TurndownService from "turndown";
import { isExternal, resolveRelative, vaultUrl } from "./api";

const toHtmlProc = unified()
  .use(remarkParse)
  .use(remarkGfm)
  .use(remarkBreaks)
  .use(remarkRehype, { allowDangerousHtml: true })
  .use(rehypeRaw)
  .use(rehypeStringify);

function render(md: string): string {
  return String(toHtmlProc.processSync(md));
}

/** HTML do edytora: obrazy przez vault://, `<div align>` jako akapity z wyrównaniem, listy zadań TipTap. */
export function mdToEditorHtml(md: string, notePath: string): string {
  const doc = new DOMParser().parseFromString(`<body>${render(md)}</body>`, "text/html");
  for (const img of Array.from(doc.querySelectorAll("img"))) {
    const src = img.getAttribute("src") ?? "";
    if (!src || isExternal(src)) continue;
    img.setAttribute("data-src", src);
    img.setAttribute("src", vaultUrl(resolveRelative(notePath, src)));
  }
  for (const div of Array.from(doc.querySelectorAll("div[align]"))) {
    const align = div.getAttribute("align") ?? "";
    // Wyrównanie z AMNO dotyczy jednej linii: akapit TipTap z text-align.
    if (div.querySelector("p, div, table, ul, ol, h1, h2, h3, h4, h5, h6, pre, blockquote")) continue;
    const p = doc.createElement("p");
    p.style.textAlign = align;
    p.innerHTML = div.innerHTML;
    div.replaceWith(p);
  }
  for (const ul of Array.from(doc.querySelectorAll("ul.contains-task-list"))) {
    ul.setAttribute("data-type", "taskList");
    for (const li of Array.from(ul.children)) {
      if (!(li instanceof HTMLElement) || !li.classList.contains("task-list-item")) continue;
      const box = li.querySelector(":scope > input[type=checkbox], :scope > p > input[type=checkbox]");
      li.setAttribute("data-type", "taskItem");
      li.setAttribute("data-checked", box?.hasAttribute("checked") ? "true" : "false");
      box?.remove();
    }
  }
  return doc.body.innerHTML;
}

// ---------- HTML → Markdown ----------

function hex(color: string): string {
  const c = color.trim();
  const m = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([\d.]+)\s*)?\)$/i.exec(c);
  if (!m) return c;
  if (m[4] !== undefined && Number(m[4]) === 0) return "transparent";
  return "#" + [m[1], m[2], m[3]].map((x) => Number(x).toString(16).padStart(2, "0")).join("").toUpperCase();
}

/** Styl tekstu w zapisie Importu: `color:#…;background:#…` (bez innych właściwości). */
function spanCss(el: HTMLElement): string {
  const css: string[] = [];
  if (el.style.color) css.push(`color:${hex(el.style.color)}`);
  const bg = el.style.backgroundColor || el.style.background;
  if (bg) css.push(`background:${hex(bg)}`);
  return css.join(";");
}

function attr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}

function text(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/ /g, "&nbsp;");
}

/** Oryginalny adres obrazu (edytor pokazuje go przez vault://). */
function imgSrc(el: Element): string {
  return el.getAttribute("data-src") || el.getAttribute("src") || "";
}

function imgHtml(el: Element): string {
  let s = `<img src="${attr(imgSrc(el))}"`;
  for (const a of ["alt", "width", "height"]) {
    const v = el.getAttribute(a);
    if (v) s += ` ${a}="${attr(v)}"`;
  }
  return s + ">";
}

/** Treść wewnątrz bloku HTML (komórka tabeli, `<div align>`): Markdown nie działa, więc znaczniki HTML. */
function inlineHtml(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) return text(node.textContent ?? "");
  if (!(node instanceof HTMLElement)) return "";
  const inner = () => Array.from(node.childNodes).map(inlineHtml).join("");
  switch (node.nodeName) {
    case "STRONG":
    case "B":
      return `<b>${inner()}</b>`;
    case "EM":
    case "I":
      return `<i>${inner()}</i>`;
    case "U":
      return `<u>${inner()}</u>`;
    case "S":
    case "DEL":
    case "STRIKE":
      return `<s>${inner()}</s>`;
    case "CODE":
      return `<code>${text(node.textContent ?? "")}</code>`;
    case "BR":
      return "<br>";
    case "IMG":
      return imgHtml(node);
    case "A":
      return `<a href="${attr(node.getAttribute("href") ?? "")}">${inner()}</a>`;
    case "SPAN": {
      const css = spanCss(node);
      return css ? `<span style="${css}">${inner()}</span>` : inner();
    }
    case "TABLE":
      return htmlTable(node as HTMLTableElement).replace(/\n/g, "");
    case "P": {
      const align = node.style.textAlign;
      return align ? `<div align="${attr(align)}">${inner()}</div>` : inner();
    }
    case "UL":
    case "OL":
      return `<${node.nodeName.toLowerCase()}>${Array.from(node.children)
        .map((li) => `<li>${Array.from(li.childNodes).map(inlineHtml).join("")}</li>`)
        .join("")}</${node.nodeName.toLowerCase()}>`;
    default:
      return inner();
  }
}

/** Zawartość komórki: akapity rozdzielone `<br>` (jak w Imporcie). */
function cellHtml(cell: HTMLElement): string {
  // Bloki (akapity TipTap) to osobne linie; węzły wierszowe obok siebie są jedną linią.
  const parts: string[] = [];
  let line: string | null = null;
  for (const n of Array.from(cell.childNodes)) {
    if (n instanceof HTMLElement && /^(P|DIV|TABLE|UL|OL|H[1-6]|PRE|BLOCKQUOTE)$/.test(n.nodeName)) {
      if (line !== null) parts.push(line);
      line = null;
      parts.push(inlineHtml(n));
    } else line = (line ?? "") + inlineHtml(n);
  }
  if (line !== null) parts.push(line);
  while (parts.length && parts[parts.length - 1] === "") parts.pop();
  return parts.join("<br>");
}

function cellBg(cell: HTMLElement): string {
  const bg = cell.getAttribute("data-background") || cell.style.backgroundColor || cell.style.background;
  return bg ? hex(bg) : "";
}

function htmlTable(table: HTMLTableElement): string {
  let s = "<table>";
  for (const row of Array.from(table.rows)) {
    s += "\n<tr>";
    for (const cell of Array.from(row.cells)) {
      const tag = cell.nodeName === "TH" ? "th" : "td";
      s += `<${tag}`;
      if (cell.colSpan > 1) s += ` colspan="${cell.colSpan}"`;
      if (cell.rowSpan > 1) s += ` rowspan="${cell.rowSpan}"`;
      const bg = cellBg(cell);
      if (bg) s += ` style="background:${attr(bg)}"`;
      s += `>${cellHtml(cell)}</${tag}>`;
    }
    s += "</tr>";
  }
  return s + "\n</table>";
}

/** Tabela, którą da się zapisać w GFM: wiersz nagłówka, bez scaleń i tła, w komórce jeden akapit. */
function isSimpleTable(table: HTMLTableElement): boolean {
  const rows = Array.from(table.rows);
  if (!rows.length || !Array.from(rows[0].cells).every((c) => c.nodeName === "TH")) return false;
  return rows.every((r, i) =>
    Array.from(r.cells).every(
      (c) =>
        (i === 0 || c.nodeName === "TD") &&
        c.colSpan === 1 &&
        c.rowSpan === 1 &&
        !cellBg(c) &&
        c.querySelectorAll("p").length <= 1 &&
        !c.querySelector("ul, ol, table, pre, blockquote, h1, h2, h3, h4, h5, h6, br, img[width]"),
    ),
  );
}

/** Ucieczki Markdownu tylko tam, gdzie tekst zmieniłby znaczenie; `[[Linki]]` i `#Tagi` zostają czytelne. */
function escapeMd(s: string): string {
  return s
    .replace(/\\/g, "\\\\")
    .replace(/[*`]/g, "\\$&")
    .replace(/~~/g, "\\~\\~")
    .replace(/(^|[^\p{L}\p{N}])_|_(?=[^\p{L}\p{N}]|$)/gu, (m) => m.replace("_", "\\_"))
    .replace(/\]\(/g, "\\](")
    .replace(/<(?=[A-Za-z/!?])/g, "&lt;")
    .replace(/ /g, "&nbsp;")
    .replace(/^(\s*)(#{1,6}\s)/gm, "$1\\$2")
    .replace(/^(\s*)([>+-]\s)/gm, "$1\\$2")
    .replace(/^(\s*\d+)([.)]\s)/gm, "$1\\$2")
    .replace(/^(\s*)(=+|-{3,})\s*$/gm, "$1\\$2");
}

function tableCellMd(cell: HTMLElement, td: TurndownService): string {
  return td.turndown(cell.innerHTML).replace(/\n+/g, " ").replace(/\|/g, "\\|").trim();
}

let service: TurndownService | null = null;

function turndown(): TurndownService {
  if (service) return service;
  const td = new TurndownService({
    headingStyle: "atx",
    hr: "---",
    bulletListMarker: "-",
    codeBlockStyle: "fenced",
    emDelimiter: "*",
    strongDelimiter: "**",
  });
  td.escape = escapeMd;
  // W Skarbcu pojedynczy koniec linii jest łamaniem (remark-breaks), więc bez dwóch spacji.
  td.addRule("br", { filter: "br", replacement: () => "\n" });
  td.addRule("underline", { filter: ["u"], replacement: (c) => (c ? `<u>${c}</u>` : "") });
  td.addRule("strike", { filter: ["s", "del", "strike"] as never, replacement: (c) => (c.trim() ? `~~${c}~~` : c) });
  td.addRule("span", {
    filter: (n) => n.nodeName === "SPAN",
    replacement: (c, n) => {
      const css = spanCss(n as HTMLElement);
      return css && c ? `<span style="${css}">${c}</span>` : c;
    },
  });
  td.addRule("mark", { filter: ["mark"], replacement: (c) => (c ? `<mark>${c}</mark>` : "") });
  td.addRule("alignedParagraph", {
    filter: (n) => n.nodeName === "P" && !!(n as HTMLElement).style.textAlign && (n as HTMLElement).style.textAlign !== "left",
    replacement: (_c, n) => `\n\n${inlineHtml(n)}\n\n`,
  });
  td.addRule("image", {
    filter: "img",
    replacement: (_c, n) => {
      const el = n as HTMLElement;
      if (el.getAttribute("width") || el.getAttribute("height")) return imgHtml(el);
      const src = imgSrc(el);
      const dest = /[\s()<>]/.test(src) ? `<${src}>` : src;
      return `![${(el.getAttribute("alt") ?? "").replace(/[[\]]/g, "\\$&")}](${dest})`;
    },
  });
  td.addRule("link", {
    filter: (n) => n.nodeName === "A" && !!n.getAttribute("href"),
    replacement: (c, n) => {
      const href = (n as HTMLElement).getAttribute("href") ?? "";
      // Adres wpisany wprost w tekst (GFM sam robi z niego link).
      if (/^(https?:\/\/|www\.)/i.test(href) && (c === href || c === escapeMd(href))) return href;
      const dest = /[\s()<>]/.test(href) ? `<${href}>` : href;
      return `[${c}](${dest})`;
    },
  });
  // `- punkt` i `1. punkt` z jedną spacją (Turndown domyślnie daje trzy).
  td.addRule("listItem", {
    filter: (n) => n.nodeName === "LI" && n.getAttribute("data-type") !== "taskItem",
    replacement: (c, n) => {
      const parent = n.parentNode as HTMLElement;
      let prefix = "- ";
      if (parent.nodeName === "OL") {
        const start = Number(parent.getAttribute("start") || 1);
        prefix = `${start + Array.prototype.indexOf.call(parent.children, n)}. `;
      }
      const pad = " ".repeat(prefix.length);
      const body = c.replace(/^\n+/, "").replace(/\n+$/, "\n").replace(/\n(?=.)/g, `\n${pad}`);
      return prefix + body + (n.nextSibling && !/\n$/.test(body) ? "\n" : "");
    },
  });
  td.addRule("taskList", {
    filter: (n) => n.nodeName === "UL" && n.getAttribute("data-type") === "taskList",
    replacement: (c) => `\n\n${c.replace(/^\n+|\n+$/g, "")}\n\n`,
  });
  td.addRule("taskItem", {
    filter: (n) => n.nodeName === "LI" && n.getAttribute("data-type") === "taskItem",
    replacement: (c, n) => {
      const checked = (n as HTMLElement).getAttribute("data-checked") === "true";
      const body = c.replace(/^\n+/, "").replace(/\n+$/, "\n").replace(/\n/gm, "\n    ");
      return `- [${checked ? "x" : " "}] ${body.trimEnd()}\n`;
    },
  });
  td.addRule("taskLabel", {
    filter: (n) => n.nodeName === "LABEL" && n.parentElement?.getAttribute("data-type") === "taskItem",
    replacement: () => "",
  });
  td.addRule("table", {
    filter: "table",
    replacement: (_c, n) => {
      const table = n as HTMLTableElement;
      if (!isSimpleTable(table)) return `\n\n${htmlTable(table)}\n\n`;
      const rows = Array.from(table.rows).map((r) => Array.from(r.cells).map((c) => tableCellMd(c, td)));
      const head = `| ${rows[0].join(" | ")} |`;
      const sep = `| ${rows[0].map(() => "---").join(" | ")} |`;
      const body = rows.slice(1).map((r) => `| ${r.join(" | ")} |`);
      return `\n\n${[head, sep, ...body].join("\n")}\n\n`;
    },
  });
  // Puste akapity (Enter w edytorze) nie zostawiają śladów.
  td.addRule("emptyParagraph", {
    filter: (n) => n.nodeName === "P" && !n.textContent?.trim() && !(n as HTMLElement).querySelector("img"),
    replacement: () => "\n\n",
  });
  service = td;
  return td;
}

/** Markdown z HTML edytora; `trailingNewline` jak w oryginale Notatki. */
export function editorHtmlToMd(html: string, trailingNewline: boolean): string {
  const md = turndown()
    .turndown(html)
    .replace(/[ \t]+$/gm, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  return md && trailingNewline ? md + "\n" : md;
}

// ---------- sprawdzenie wierności ----------

/** Postać kanoniczna wyrenderowanej Notatki: struktura, tekst i znaczące atrybuty. */
function canon(md: string): string {
  const doc = new DOMParser().parseFromString(`<body>${render(md)}</body>`, "text/html");
  const out: string[] = [];
  const tagName = (el: Element) => {
    const t = el.nodeName;
    if (t === "B") return "STRONG";
    if (t === "I") return "EM";
    if (t === "S" || t === "STRIKE") return "DEL";
    if (t === "TH") return "TD";
    return t;
  };
  const walk = (n: Node) => {
    if (n.nodeType === Node.TEXT_NODE) {
      const t = (n.textContent ?? "").replace(/[\s ]+/g, " ").trim();
      if (t) out.push(t);
      return;
    }
    if (!(n instanceof HTMLElement)) return;
    let t = tagName(n);
    const parts: string[] = [];
    let align = n.getAttribute("align") || n.style.textAlign;
    if (t === "DIV" && align) t = "P";
    if (align === "left") align = "";
    if (align) parts.push(`align=${align}`);
    for (const a of ["href", "colspan", "rowspan", "width", "height"]) {
      const v = n.getAttribute(a);
      if (v && !(a.endsWith("span") && v === "1")) parts.push(`${a}=${v}`);
    }
    if (t === "IMG") parts.push(`src=${n.getAttribute("src")}`);
    if (t === "INPUT") parts.push(n.hasAttribute("checked") ? "x" : "_");
    const css = t === "TD" ? cellBg(n) : spanCss(n);
    if (css) parts.push(css);
    // Akapity w elementach listy i komórkach zależą od zapisu (luźna/ścisła lista), nie od treści.
    const transparent = (t === "P" && !align && (n.parentElement?.nodeName === "LI" || n.parentElement?.nodeName === "TD")) ||
      (t === "SPAN" && !css) || t === "DIV" || t === "TBODY" || t === "THEAD";
    if (!transparent) out.push(`<${t}${parts.length ? " " + parts.join(" ") : ""}>`);
    n.childNodes.forEach(walk);
    if (!transparent) out.push(`</${t}>`);
  };
  doc.body.childNodes.forEach(walk);
  return out.join("");
}

/** Czy zapis z edytora wizualnego oznacza tę samą treść (po renderowaniu) co oryginał. */
export function sameRendering(a: string, b: string): boolean {
  return canon(a) === canon(b);
}
