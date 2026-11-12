// Wstawianie daty i godziny w edytorach (Ctrl+; data, Ctrl+Shift+; data i godzina, jak w Excelu).
const pad = (n: number) => String(n).padStart(2, "0");

/** `2026-11-12` (data lokalna). */
export function dateText(d = new Date()): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `2026-11-12 14:05`. */
export function dateTimeText(d = new Date()): string {
  return `${dateText(d)} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export const DATE_KEY = "Mod-;";
export const DATETIME_KEY = "Mod-Shift-;";
