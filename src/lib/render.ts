/**
 * Chargement des bitmaps de pages via le protocole interne `aura://`.
 *
 * - Le cœur Rust renvoie un bitmap RGBA brut précédé de sa taille ;
 * - les bitmaps décodés (ImageBitmap) sont gardés dans un cache LRU borné en mémoire ;
 * - une réponse 204 signifie que la demande est devenue obsolète (défilement,
 *   zoom) : l'appelant redemande si la page est toujours visible.
 */

const IS_WINDOWS = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);

/** Base des URL du protocole interne (Windows : http://aura.localhost/). */
export const PROTOCOL_BASE = IS_WINDOWS ? "http://aura.localhost" : "aura://localhost";

export function fileThumbUrl(path: string, width = 160): string {
  return `${PROTOCOL_BASE}/file-thumb?path=${encodeURIComponent(path)}&w=${Math.round(width)}`;
}

/** Aperçu d'une image locale (formats d'image uniquement, lecture seule). */
export function localImageUrl(path: string): string {
  return `${PROTOCOL_BASE}/image?path=${encodeURIComponent(path)}`;
}

export const Prio ={ VISIBLE: 0, NEAR: 1, THUMB: 2 } as const;

export interface RenderRequest {
  doc: number;
  page: number;
  /** Largeur cible en pixels physiques. */
  width: number;
  /** Rotation d'affichage en quarts de tour. */
  rot: number;
  annots: boolean;
  forms: boolean;
  prio: number;
  gen: number;
  /** Tuile (pixels du rendu complet) pour les très grands zooms. */
  tile?: { x: number; y: number; w: number; h: number };
  /** Révision des annotations (invalide le cache quand PDFium doit redessiner). */
  rev?: number;
}

export type RenderResult = { kind: "ok"; bitmap: ImageBitmap } | { kind: "stale" } | { kind: "error"; message: string };

// ---------------------------------------------------------------- cache LRU

const MAX_BYTES = 384 * 1024 * 1024;
let cacheBytes = 0;
const cache = new Map<string, { bmp: ImageBitmap; bytes: number }>();

function keyOf(r: Pick<RenderRequest, "doc" | "page" | "width" | "rot" | "annots" | "forms" | "tile" | "rev">) {
  const t = r.tile ? `:${r.tile.x},${r.tile.y},${r.tile.w},${r.tile.h}` : "";
  return `${r.doc}:${r.page}:${r.width}:${r.rot}:${r.annots ? 1 : 0}${r.forms ? 1 : 0}r${r.rev ?? 0}${t}`;
}

function cachePut(key: string, bmp: ImageBitmap) {
  const bytes = bmp.width * bmp.height * 4;
  const prev = cache.get(key);
  if (prev) {
    cacheBytes -= prev.bytes;
    cache.delete(key);
  }
  cache.set(key, { bmp, bytes });
  cacheBytes += bytes;
  // Évincer les plus anciens.
  for (const [k, v] of cache) {
    if (cacheBytes <= MAX_BYTES) break;
    cache.delete(k);
    cacheBytes -= v.bytes;
    v.bmp.close();
  }
}

function cacheGet(key: string): ImageBitmap | undefined {
  const v = cache.get(key);
  if (!v) return undefined;
  // Marquer comme récemment utilisé.
  cache.delete(key);
  cache.set(key, v);
  return v.bmp;
}

/** Meilleur bitmap déjà disponible pour une page (n'importe quelle résolution), pour l'affichage immédiat. */
export function bestCached(doc: number, page: number, rot: number, rev = 0): ImageBitmap | undefined {
  let best: ImageBitmap | undefined;
  for (const [k, v] of cache) {
    const parts = k.split(":");
    if (parts.length !== 5) continue; // tuiles exclues
    const [d, p, , r, f] = parts;
    if (+d === doc && +p === page && +r === rot && f.endsWith(`r${rev}`) && (!best || v.bmp.width > best.width)) best = v.bmp;
  }
  return best;
}

/** Libère les bitmaps d'un document fermé. */
export function dropDocument(doc: number) {
  for (const [k, v] of cache) {
    if (k.startsWith(`${doc}:`)) {
      cache.delete(k);
      cacheBytes -= v.bytes;
      v.bmp.close();
    }
  }
}

// ---------------------------------------------------------------- requêtes

const inflight = new Map<string, Promise<RenderResult>>();

export function cachedBitmap(r: RenderRequest): ImageBitmap | undefined {
  return cacheGet(keyOf(r));
}

export function renderPage(r: RenderRequest, signal?: AbortSignal): Promise<RenderResult> {
  const key = keyOf(r);
  const hit = cacheGet(key);
  if (hit) return Promise.resolve({ kind: "ok", bitmap: hit });
  const pending = inflight.get(key);
  if (pending) return pending;

  const url =
    `${PROTOCOL_BASE}/render?doc=${r.doc}&page=${r.page}&w=${Math.round(r.width)}` +
    `&rot=${r.rot}&annots=${r.annots ? 1 : 0}&forms=${r.forms ? 1 : 0}&prio=${r.prio}&gen=${r.gen}` +
    (r.tile ? `&tx=${r.tile.x}&ty=${r.tile.y}&tw=${r.tile.w}&th=${r.tile.h}` : "");

  const p = (async (): Promise<RenderResult> => {
    try {
      const res = await fetch(url, { signal });
      if (res.status === 204) return { kind: "stale" };
      if (!res.ok) return { kind: "error", message: await res.text() };
      const buf = await res.arrayBuffer();
      const view = new DataView(buf);
      const w = view.getUint32(0, true);
      const h = view.getUint32(4, true);
      const data = new Uint8ClampedArray(buf, 8, w * h * 4);
      const bitmap = await createImageBitmap(new ImageData(data, w, h));
      cachePut(key, bitmap);
      return { kind: "ok", bitmap };
    } catch (e) {
      if ((e as Error)?.name === "AbortError") return { kind: "stale" };
      return { kind: "error", message: String(e) };
    } finally {
      inflight.delete(key);
    }
  })();
  inflight.set(key, p);
  return p;
}
