/**
 * Géométrie du visualiseur : position de chaque page selon le zoom, la
 * rotation et le mode d'affichage (continu, page, double). Fonctions pures,
 * testées dans `layout.test.ts`.
 */
import { PT_TO_PX } from "../theme/tokens";

export type ViewMode = "continu" | "page" | "double";

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Layout {
  width: number;
  height: number;
  /** Une boîte par page (pages non affichées en mode « page » : boîte vide). */
  boxes: Box[];
  /** Pages réellement disposées, dans l'ordre vertical. */
  shown: number[];
  scale: number;
}

export const PAD_TOP = 44;
export const PAD_BOTTOM = 104;
export const PAD_X = 24;
export const GAP = 28;

export interface LayoutInput {
  pages: { w: number; h: number }[];
  zoom: number; // pourcentage (100 = taille réelle à 96 ppp)
  rot: number; // quarts de tour
  mode: ViewMode;
  current: number; // page courante (mode « page »)
  viewportW: number;
}

function dims(p: { w: number; h: number }, scale: number, rot: number) {
  const odd = rot % 2 === 1;
  return { w: (odd ? p.h : p.w) * scale, h: (odd ? p.w : p.h) * scale };
}

export function computeLayout(input: LayoutInput): Layout {
  const { pages, zoom, rot, mode, viewportW } = input;
  const scale = (zoom / 100) * PT_TO_PX;
  const boxes: Box[] = pages.map(() => ({ x: 0, y: 0, w: 0, h: 0 }));
  const shown: number[] = [];
  let rows: number[][];
  if (mode === "page") {
    const c = Math.min(Math.max(0, input.current), pages.length - 1);
    rows = pages.length ? [[c]] : [];
  } else if (mode === "double") {
    rows = [];
    for (let i = 0; i < pages.length; i += 2) rows.push(i + 1 < pages.length ? [i, i + 1] : [i]);
  } else {
    rows = pages.map((_, i) => [i]);
  }

  // Largeur du contenu : la rangée la plus large.
  let maxRowW = 0;
  const rowDims = rows.map((row) => {
    const ds = row.map((i) => dims(pages[i], scale, rot));
    const w = ds.reduce((s, d) => s + d.w, 0) + GAP * (row.length - 1);
    const h = Math.max(...ds.map((d) => d.h));
    maxRowW = Math.max(maxRowW, w);
    return { ds, w, h };
  });
  const width = Math.max(viewportW, maxRowW + PAD_X * 2);

  let y = PAD_TOP;
  rows.forEach((row, r) => {
    const { ds, w, h } = rowDims[r];
    let x = (width - w) / 2;
    row.forEach((i, k) => {
      const d = ds[k];
      boxes[i] = { x, y: y + (h - d.h) / 2, w: d.w, h: d.h };
      shown.push(i);
      x += d.w + GAP;
    });
    y += h + GAP;
  });
  const height = rows.length ? y - GAP + PAD_BOTTOM : PAD_TOP + PAD_BOTTOM;
  return { width, height, boxes, shown, scale };
}

/** Pages visibles dans la fenêtre [top, bottom] (plus une marge), dans l'ordre vertical. */
export function visiblePages(layout: Layout, top: number, bottom: number, margin = 0): number[] {
  const { shown, boxes } = layout;
  if (!shown.length) return [];
  const lo = top - margin;
  const hi = bottom + margin;
  // Recherche dichotomique de la première page dont le bas dépasse `lo`.
  let a = 0;
  let b = shown.length - 1;
  while (a < b) {
    const m = (a + b) >> 1;
    const bx = boxes[shown[m]];
    if (bx.y + bx.h < lo) a = m + 1;
    else b = m;
  }
  const out: number[] = [];
  for (let k = Math.max(0, a - 1); k < shown.length; k++) {
    const bx = boxes[shown[k]];
    if (bx.y > hi) break;
    if (bx.y + bx.h >= lo) out.push(shown[k]);
  }
  return out;
}

/** Page « courante » : celle qui occupe le plus la moitié haute de la fenêtre. */
export function currentPageAt(layout: Layout, top: number, viewportH: number): number {
  const probe = top + viewportH * 0.35;
  const vis = visiblePages(layout, top, top + viewportH);
  if (!vis.length) return 0;
  let best = vis[0];
  let bestDist = Infinity;
  for (const i of vis) {
    const bx = layout.boxes[i];
    const dist = probe < bx.y ? bx.y - probe : probe > bx.y + bx.h ? probe - (bx.y + bx.h) : 0;
    if (dist < bestDist) {
      bestDist = dist;
      best = i;
    }
  }
  return best;
}

export interface Anchor {
  page: number;
  fx: number;
  fy: number;
}

/** Point de la fenêtre (en coordonnées de contenu) → position relative dans une page. */
export function anchorAt(layout: Layout, cx: number, cy: number): Anchor {
  const vis = visiblePages(layout, cy, cy, 400);
  let page = vis[0] ?? 0;
  for (const i of vis) {
    const b = layout.boxes[i];
    if (cy >= b.y - GAP / 2 && cy <= b.y + b.h + GAP / 2) {
      page = i;
      if (cx >= b.x && cx <= b.x + b.w) break;
    }
  }
  const b = layout.boxes[page] ?? { x: 0, y: 0, w: 1, h: 1 };
  return { page, fx: (cx - b.x) / (b.w || 1), fy: (cy - b.y) / (b.h || 1) };
}

/** Position de contenu d'une ancre dans une nouvelle disposition. */
export function anchorPoint(layout: Layout, a: Anchor): { x: number; y: number } {
  const b = layout.boxes[a.page] ?? { x: 0, y: 0, w: 0, h: 0 };
  return { x: b.x + a.fx * b.w, y: b.y + a.fy * b.h };
}

/** Zoom (en %) pour que la rangée la plus large tienne dans la largeur disponible. */
export function fitWidthZoom(pages: { w: number; h: number }[], rot: number, mode: ViewMode, viewportW: number): number {
  if (!pages.length) return 100;
  const odd = rot % 2 === 1;
  const widths = pages.map((p) => (odd ? p.h : p.w));
  let maxPt: number;
  let gaps = 0;
  if (mode === "double") {
    maxPt = 0;
    for (let i = 0; i < widths.length; i += 2) maxPt = Math.max(maxPt, widths[i] + (widths[i + 1] ?? 0));
    gaps = GAP;
  } else {
    maxPt = Math.max(...widths);
  }
  const avail = viewportW - PAD_X * 2 - gaps;
  return clampZoom((avail / (maxPt * PT_TO_PX)) * 100);
}

/** Zoom pour qu'une page entière tienne dans la fenêtre. */
export function fitPageZoom(page: { w: number; h: number }, rot: number, viewportW: number, viewportH: number): number {
  const odd = rot % 2 === 1;
  const w = (odd ? page.h : page.w) * PT_TO_PX;
  const h = (odd ? page.w : page.h) * PT_TO_PX;
  return clampZoom(Math.min((viewportW - PAD_X * 2) / w, (viewportH - PAD_TOP - 40) / h) * 100);
}

export const ZOOM_MIN = 10;
export const ZOOM_MAX = 800;
export const ZOOM_STEPS = [10, 25, 33, 50, 67, 75, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500, 600, 800];

export function clampZoom(z: number): number {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(z)));
}

export function nextZoom(z: number, dir: 1 | -1): number {
  if (dir > 0) return ZOOM_STEPS.find((s) => s > z + 0.5) ?? ZOOM_MAX;
  return [...ZOOM_STEPS].reverse().find((s) => s < z - 0.5) ?? ZOOM_MIN;
}

/** Applique une rotation d'affichage (quarts de tour horaires) à un rectangle de page. */
export function rotateRect(r: Box, pageW: number, pageH: number, rot: number): Box {
  switch (((rot % 4) + 4) % 4) {
    case 1:
      return { x: pageH - r.y - r.h, y: r.x, w: r.h, h: r.w };
    case 2:
      return { x: pageW - r.x - r.w, y: pageH - r.y - r.h, w: r.w, h: r.h };
    case 3:
      return { x: r.y, y: pageW - r.x - r.w, w: r.h, h: r.w };
    default:
      return r;
  }
}
