import { api } from "./api";

const pad = (n: number) => String(n).padStart(2, "0");

/** Wklejony zrzut ekranu ma w schowku nazwę „image.png”: dostaje nazwę z datą. */
function fileName(f: File, pasted: boolean): string {
  const generic = !f.name || /^image\.\w+$/i.test(f.name);
  if (!pasted || !generic) return f.name || "plik";
  const d = new Date();
  const ext = (f.type.split("/")[1] || "png").replace("jpeg", "jpg").replace(/[^a-z0-9]/gi, "");
  return `zrzut-${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}.${ext}`;
}

/** Pliki z przeciągnięcia albo schowka (bez katalogów). */
export function filesOf(dt: DataTransfer | null): File[] {
  if (!dt) return [];
  return Array.from(dt.files).filter((f) => f.size > 0 || f.type !== "");
}

/** Zapisuje pliki jako Załączniki Notatki i zwraca Markdown do wstawienia (każdy w osobnej linii). */
export async function attachFiles(notePath: string, files: File[], pasted: boolean): Promise<string> {
  const lines: string[] = [];
  for (const f of files) {
    const a = await api.addAttachment(notePath, fileName(f, pasted), f);
    const label = a.name.replace(/[[\]]/g, "");
    lines.push(a.image ? `![${label}](${a.link})` : `[${label}](${a.link})`);
  }
  return lines.join("\n");
}
