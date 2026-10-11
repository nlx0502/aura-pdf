/**
 * Formatage en français : tailles de fichiers, dates relatives, regroupement
 * des documents récents. Fonctions pures, testées dans `format.test.ts`.
 */

const nf1 = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });
const nf0 = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 0 });

/** « 2,4 Mo », « 380 Ko », « 96 Ko ». */
export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "—";
  if (bytes < 1024) return `${nf0.format(bytes)} o`;
  if (bytes < 1024 * 1024) return `${nf0.format(Math.max(1, Math.round(bytes / 1024)))} Ko`;
  if (bytes < 1024 ** 3) return `${nf1.format(bytes / 1024 ** 2)} Mo`;
  return `${nf1.format(bytes / 1024 ** 3)} Go`;
}

export type RecentGroup = "today" | "yesterday" | "week" | "older";

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** Groupe d'une date d'ouverture : Aujourd'hui, Hier, Cette semaine (7 derniers jours), Plus ancien. */
export function groupOf(date: Date, now = new Date()): RecentGroup {
  const day = 86_400_000;
  const diffDays = Math.round((startOfDay(now) - startOfDay(date)) / day);
  if (diffDays <= 0) return "today";
  if (diffDays === 1) return "yesterday";
  if (diffDays < 7) return "week";
  return "older";
}

const timeFmt = new Intl.DateTimeFormat("fr-FR", { hour: "2-digit", minute: "2-digit" });
const weekdayFmt = new Intl.DateTimeFormat("fr-FR", { weekday: "long" });
const shortDateFmt = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "short" });
const longDateFmt = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "long", year: "numeric" });

function capitalize(s: string) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** « À l'instant », « Il y a 5 min », « Il y a 2 h », « Hier, 17:40 », « Lundi », « 28 sept. ». */
export function formatWhen(date: Date, now = new Date()): string {
  const diffMin = Math.floor((now.getTime() - date.getTime()) / 60_000);
  const g = groupOf(date, now);
  if (g === "today") {
    if (diffMin < 1) return "À l’instant";
    if (diffMin < 60) return `Il y a ${diffMin} min`;
    return `Il y a ${Math.floor(diffMin / 60)} h`;
  }
  if (g === "yesterday") return `Hier, ${timeFmt.format(date)}`;
  if (g === "week") return capitalize(weekdayFmt.format(date));
  const sameYear = date.getFullYear() === now.getFullYear();
  return sameYear ? shortDateFmt.format(date) : longDateFmt.format(date);
}

/** Version courte pour le panneau « Récents » du lecteur. */
export function formatWhenShort(date: Date, now = new Date()): string {
  const g = groupOf(date, now);
  const diffDays = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
  if (g === "today") return formatWhen(date, now).replace("À l’instant", "Maintenant");
  if (g === "yesterday") return "Hier";
  if (diffDays < 7) return `Il y a ${diffDays} j`;
  if (diffDays < 31) return `Il y a ${Math.floor(diffDays / 7)} sem.`;
  return shortDateFmt.format(date);
}

/** Date PDF (« D:20250114093000+01'00' ») → texte français, ou null. */
export function formatPdfDate(raw?: string | null): string | null {
  if (!raw) return null;
  const m = raw.match(/^(?:D:)?(\d{4})(\d{2})?(\d{2})?(\d{2})?(\d{2})?/);
  if (!m) return null;
  const [, y, mo = "01", d = "01", h = "00", mi = "00"] = m;
  const date = new Date(+y, +mo - 1, +d, +h, +mi);
  if (Number.isNaN(date.getTime())) return null;
  return longDateFmt.format(date);
}

/** Pourcentage de lecture : page (index 0) sur total. */
export function readingPct(lastPage: number, pageCount: number): number {
  if (pageCount <= 0) return 0;
  return Math.round(((lastPage + 1) / pageCount) * 100);
}

/** Retire l'extension d'un nom de fichier. */
export function stripExt(name: string): string {
  return name.replace(/\.[^.\\/]+$/, "");
}
