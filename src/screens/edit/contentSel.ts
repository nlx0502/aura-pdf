/**
 * Sélection du mode Contenu, partagée entre la page et le panneau de droite.
 *
 * Les objets d'une page changent d'index à chaque modification : un bloc
 * sélectionné est donc retrouvé par sa position (le plus grand recouvrement).
 */
import { create } from "zustand";
import type { Rect } from "../../lib/api";
import type { ContentItem } from "../../lib/edit";

export type ContentTool = "text" | "image" | "link" | "redact";

export type ContentSel = { kind: "text" | "image"; rect: Rect } | { kind: "link"; id: string } | null;

interface State {
  tool: ContentTool;
  sel: ContentSel;
  /** Bloc de texte en cours de saisie directe sur la page. */
  editing: boolean;
  /** Rognage d'image en cours (rectangle en points d'affichage). */
  crop: Rect | null;
  /** Éléments de la page affichée (relus après chaque modification). */
  items: ContentItem[];
  set: (p: Partial<State>) => void;
  clear: () => void;
}

export const useContentSel = create<State>((set) => ({
  tool: "text",
  sel: null,
  editing: false,
  crop: null,
  items: [],
  set: (p) => set(p),
  clear: () => set({ sel: null, editing: false, crop: null }),
}));

function overlap(a: Rect, b: Rect) {
  const w = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
  const h = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
  return w > 0 && h > 0 ? w * h : 0;
}

/** Élément correspondant à la sélection (le plus grand recouvrement du même type). */
export function resolveItem(items: ContentItem[], sel: ContentSel): ContentItem | null {
  if (!sel || sel.kind === "link") return null;
  let best: ContentItem | null = null;
  let score = 0;
  for (const it of items) {
    if (it.kind !== sel.kind) continue;
    const s = overlap(it.rect, sel.rect);
    if (s > score) {
      score = s;
      best = it;
    }
  }
  return best;
}
