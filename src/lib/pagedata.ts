/**
 * Données secondaires des pages (couche texte, liens), chargées à la demande
 * et mises en cache par document.
 */
import { api, type PageLink, type TextWord } from "./api";

const textCache = new Map<string, Promise<TextWord[]>>();
const linkCache = new Map<string, Promise<PageLink[]>>();

export function pageWords(doc: number, page: number): Promise<TextWord[]> {
  const k = `${doc}:${page}`;
  let p = textCache.get(k);
  if (!p) {
    p = api.pageText(doc, page).catch(() => []);
    textCache.set(k, p);
  }
  return p;
}

export function pageLinks(doc: number, page: number): Promise<PageLink[]> {
  const k = `${doc}:${page}`;
  let p = linkCache.get(k);
  if (!p) {
    p = api.pageLinks(doc, page).catch(() => []);
    linkCache.set(k, p);
  }
  return p;
}

export function dropPageData(doc: number) {
  for (const m of [textCache, linkCache]) {
    for (const k of m.keys()) if (k.startsWith(`${doc}:`)) m.delete(k);
  }
}

// ---------------------------------------------------------------- mesure du texte

let ctx: CanvasRenderingContext2D | null = null;

/** Largeur d'un texte à 100 px dans la police de la couche texte (mise en cache). */
const widthCache = new Map<string, number>();
export function measure100(text: string): number {
  const cached = widthCache.get(text);
  if (cached !== undefined) return cached;
  if (!ctx) {
    ctx = document.createElement("canvas").getContext("2d");
    if (ctx) ctx.font = "100px sans-serif";
  }
  const w = ctx?.measureText(text).width ?? text.length * 50;
  if (widthCache.size > 20_000) widthCache.clear();
  widthCache.set(text, w);
  return w;
}
