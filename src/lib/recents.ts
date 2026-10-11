/**
 * Préparation de la liste des documents récents de l'accueil :
 * filtrage (recherche, favoris, emplacement, étiquette) et regroupement par période.
 */
import type { RecentEntry } from "./api";
import { groupOf, type RecentGroup } from "./format";

export type RecentFilter = "all" | "signed" | "form" | "scan";
export type RecentView = "home" | "recent" | "fav";

export interface RecentQuery {
  q: string;
  view: RecentView;
  place: "docs" | "desk" | "dl" | null;
  filter: RecentFilter;
}

export function filterRecents(list: RecentEntry[], query: RecentQuery): RecentEntry[] {
  const q = query.q.trim().toLowerCase();
  return list.filter((r) => {
    if (q && !r.name.toLowerCase().includes(q) && !r.folder.toLowerCase().includes(q)) return false;
    if (query.view === "fav" && !r.favorite) return false;
    if (query.place && r.place !== query.place) return false;
    if (query.filter === "signed" && !r.signed) return false;
    if (query.filter === "form" && !r.form) return false;
    if (query.filter === "scan" && !r.scanned) return false;
    return true;
  });
}

export const GROUP_ORDER: RecentGroup[] = ["today", "yesterday", "week", "older"];

/** Regroupe (déjà trié du plus récent au plus ancien) en Aujourd'hui / Hier / Cette semaine / Plus ancien. */
export function groupRecents<T>(list: T[], dateOf: (t: T) => Date, now = new Date()): { group: RecentGroup; items: T[] }[] {
  const sorted = [...list].sort((a, b) => dateOf(b).getTime() - dateOf(a).getTime());
  const map = new Map<RecentGroup, T[]>();
  for (const item of sorted) {
    const g = groupOf(dateOf(item), now);
    if (!map.has(g)) map.set(g, []);
    map.get(g)!.push(item);
  }
  return GROUP_ORDER.filter((g) => map.has(g)).map((group) => ({ group, items: map.get(group)! }));
}

/** Étiquette principale d'un document (une seule affichée, comme dans les maquettes). */
export function tagOf(r: Pick<RecentEntry, "signed" | "form" | "scanned" | "annotated">): "signed" | "form" | "scan" | "annotated" | null {
  if (r.signed) return "signed";
  if (r.form) return "form";
  if (r.scanned) return "scan";
  if (r.annotated) return "annotated";
  return null;
}
