/**
 * Podsumowanie pliku Daily brief. Briefy generuje AI według stałego wzoru:
 * `# Tytuł`, akapit wstępu (i ewentualnie założenia), potem sekcje `## HH:MM–HH:MM (N min) · Nazwa — status`
 * oraz sekcje bez godziny (np. „Konflikty i uwagi”). Gdy plik ma inny układ, zostaje tytuł, wstęp i surowe sekcje.
 */

export type BriefMeeting = {
  kind: "meeting";
  start: string;
  end: string;
  duration: string | null;
  title: string;
  /** Dopisek po „—” w nagłówku (np. „odbyło się”, „PRZED TOBĄ”). */
  status: string | null;
  /** Pierwsza linia treści, gdy to miejsce albo link do spotkania. */
  where: string | null;
  agenda: string | null;
  /** Etykieta listy zadań (np. „Do przygotowania”) i jej punkty. */
  todoLabel: string | null;
  todo: string[];
  /** Cała treść sekcji (Markdown). */
  body: string;
};

export type BriefOther = { kind: "other"; title: string; body: string };
export type BriefSection = BriefMeeting | BriefOther;

export type ParsedBrief = {
  title: string;
  /** Pierwszy akapit wstępu. */
  lead: string;
  /** Pozostałe akapity wstępu (np. założenia). */
  extra: string[];
  sections: BriefSection[];
};

const MEETING = /^(\d{1,2}:\d{2})\s*[–—-]\s*(\d{1,2}:\d{2})\s*(?:\(([^)]*)\))?\s*·\s*(.+)$/;
const LABEL = /\*\*([^*]+?):\*\*/g;

function paragraphs(lines: string[]): string[] {
  return lines
    .join("\n")
    .split(/\n\s*\n/)
    .map((p) => p.trim())
    .filter((p) => p && !/^(-{3,}|\*{3,})$/.test(p));
}

/** Tekst po etykiecie `**Nazwa:**` w linii, do następnej pogrubionej etykiety. */
function labelled(line: string, name: RegExp): { label: string; text: string } | null {
  const marks = [...line.matchAll(LABEL)];
  for (let i = 0; i < marks.length; i++) {
    const m = marks[i];
    if (!name.test(m[1])) continue;
    const from = m.index! + m[0].length;
    const to = i + 1 < marks.length ? marks[i + 1].index! : line.length;
    return { label: m[1], text: line.slice(from, to).trim() };
  }
  return null;
}

function meeting(head: RegExpMatchArray, lines: string[]): BriefMeeting {
  const rest = head[4].trim();
  const dash = rest.lastIndexOf(" — ");
  const body = lines.join("\n").trim();
  const nonEmpty = lines.map((l) => l.trim()).filter(Boolean);
  const first = nonEmpty[0] ?? "";
  let agenda: string | null = null;
  let todoLabel: string | null = null;
  const todo: string[] = [];
  for (let i = 0; i < nonEmpty.length; i++) {
    const l = nonEmpty[i];
    const a: { label: string; text: string } | null = agenda === null ? labelled(l, /^agenda$/i) : null;
    if (a) agenda = a.text || null;
    const t: { label: string; text: string } | null = todoLabel === null ? labelled(l, /^do (zrobienia|przygotowania)/i) : null;
    if (t) {
      todoLabel = t.label;
      if (t.text) todo.push(t.text);
      for (let j = i + 1; j < nonEmpty.length && /^[-*]\s+/.test(nonEmpty[j]); j++) todo.push(nonEmpty[j].replace(/^[-*]\s+/, ""));
    }
  }
  return {
    kind: "meeting",
    start: head[1],
    end: head[2],
    duration: head[3] ?? null,
    title: dash > 0 ? rest.slice(0, dash).trim() : rest,
    status: dash > 0 ? rest.slice(dash + 3).trim() : null,
    where: first && !first.startsWith("**") && !first.startsWith("-") ? first : null,
    agenda,
    todoLabel,
    todo,
    body,
  };
}

export function parseBrief(md: string): ParsedBrief {
  const lines = md.replace(/\r\n?/g, "\n").replace(/^---\n[\s\S]*?\n---\n/, "").split("\n");
  let title = "";
  const intro: string[] = [];
  const sections: BriefSection[] = [];
  let cur: { head: string; lines: string[] } | null = null;
  const flush = () => {
    if (!cur) return;
    const body = cur.lines.join("\n").replace(/\n\s*(-{3,}|\*{3,})\s*$/, "").split("\n");
    const m = cur.head.match(MEETING);
    sections.push(m ? meeting(m, body) : { kind: "other", title: cur.head, body: body.join("\n").trim() });
  };
  for (const l of lines) {
    if (!title && !cur && /^#\s+/.test(l)) {
      title = l.replace(/^#\s+/, "").trim();
    } else if (/^##\s+/.test(l)) {
      flush();
      cur = { head: l.replace(/^##\s+/, "").trim(), lines: [] };
    } else if (cur) {
      cur.lines.push(l);
    } else {
      intro.push(l);
    }
  }
  flush();
  const ps = paragraphs(intro);
  return { title, lead: ps[0] ?? "", extra: ps.slice(1), sections };
}

/** Minuty od północy dla „HH:MM”. */
export function minutes(hhmm: string): number {
  const [h, m] = hhmm.split(":").map(Number);
  return h * 60 + m;
}

/** Spotkanie jeszcze się nie skończyło (względem `now`); bez czasu decyduje dopisek w nagłówku. */
export function isAhead(m: BriefMeeting, now: Date): boolean {
  const n = now.getHours() * 60 + now.getMinutes();
  return minutes(m.end) > n;
}

export function isNow(m: BriefMeeting, now: Date): boolean {
  const n = now.getHours() * 60 + now.getMinutes();
  return minutes(m.start) <= n && n < minutes(m.end);
}

/** „za 2 h 15 min”, „za 5 min”. */
export function untilLabel(hhmm: string, now: Date): string {
  const d = minutes(hhmm) - (now.getHours() * 60 + now.getMinutes());
  if (d <= 0) return "teraz";
  const h = Math.floor(d / 60);
  const m = d % 60;
  return `za ${h ? `${h} h` : ""}${h && m ? " " : ""}${m ? `${m} min` : ""}`;
}
