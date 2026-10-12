/**
 * Outils (fusion, division, compression) : accès au cœur Rust, suivi des
 * tâches et calculs purs (plages de pages, estimation de la taille).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface Mark {
  title: string;
  page: number;
  depth: number;
}

export interface FileInfo {
  path: string;
  name: string;
  size: number;
  pages: number;
  encrypted: boolean;
  marks: Mark[];
}

export interface ImageInfo {
  bytes: number;
  w: number;
  h: number;
  comps: number;
  jpeg: boolean;
  ppi: number | null;
  fixed: boolean;
}

export interface Analysis {
  total: number;
  pages: number;
  images: number;
  fonts: number;
  content: number;
  meta: number;
  thumbs: number;
  attach: number;
  uncompressed: number;
  objects: number;
  imageList: ImageInfo[];
  fontList: { bytes: number; standard: boolean; subsettable: boolean }[];
}

export interface Output {
  path: string;
  size: number;
  pages: number;
}

export interface MergeOptions {
  bookmarks: boolean;
  toc: boolean;
  a4: boolean;
}

export interface CompressOptions {
  dpi: number;
  quality: number;
  gray: boolean;
  subset: boolean;
  unembed: boolean;
  meta: boolean;
  thumbs: boolean;
  attach: boolean;
  compact: boolean;
}

export interface SplitPart {
  start: number;
  end: number;
  name: string;
}

export const toolsApi = {
  inspect: (path: string) => invoke<FileInfo>("tool_inspect", { path }),
  analyze: (path: string) => invoke<Analysis>("tool_analyze", { path }),
  bySize: (path: string, maxBytes: number) => invoke<[number, number][]>("split_by_size", { path, maxBytes }),
  merge: (job: number, inputs: string[], opts: MergeOptions, outDir: string, outName: string) => invoke<string>("merge_start", { job, inputs, opts, outDir, outName }),
  split: (job: number, path: string, parts: SplitPart[], outDir: string) => invoke<void>("split_start", { job, path, parts, outDir }),
  compress: (job: number, path: string, opts: CompressOptions) => invoke<string>("compress_start", { job, path, opts }),
  cancel: (job: number) => invoke<void>("tool_cancel", { job }),
};

// ---------------------------------------------------------------- suivi des tâches

export interface Progress {
  step: number;
  done: number;
  total: number;
  detail: string;
}
export type Done = { ok: true; outputs: Output[] } | { ok: false; cancelled?: boolean; message?: string };

interface Handlers {
  progress: (p: Progress) => void;
  done: (d: Done) => void;
}
const handlers = new Map<number, Handlers>();
let listening = false;
let seq = Date.now() % 100000;

function ensureListening() {
  if (listening) return;
  listening = true;
  void listen<Progress & { job: number }>("tool:progress", (e) => handlers.get(e.payload.job)?.progress(e.payload));
  void listen<Done & { job: number }>("tool:done", (e) => {
    const h = handlers.get(e.payload.job);
    handlers.delete(e.payload.job);
    h?.done(e.payload);
  });
}

/** Nouvelle tâche : identifiant et abonnement à sa progression. */
export function newJob(h: Handlers): number {
  ensureListening();
  const id = ++seq;
  handlers.set(id, h);
  return id;
}

export function forgetJob(id: number) {
  handlers.delete(id);
}

// ---------------------------------------------------------------- division

/** Couleurs des parties (maquette Merge). */
export const PART_COLORS = ["#8B87FA", "#4FCB91", "#F2B84B", "#F07A9A", "#5BC0EB", "#C49BF5"];

/**
 * Plages saisies (« 1-4, 5-10, 11 ») → parties (pages numérotées à partir de 1).
 * `bad` si une plage est invalide ou hors du document.
 */
export function parseRanges(text: string, pages: number): { parts: [number, number][]; bad: boolean } {
  const parts: [number, number][] = [];
  let bad = false;
  for (const raw of text.split(/[,;]/).map((s) => s.trim()).filter(Boolean)) {
    const m = raw.match(/^(\d+)\s*[-–]\s*(\d+)$/) ?? raw.match(/^(\d+)$/);
    if (!m) {
      bad = true;
      continue;
    }
    const a = Number(m[1]);
    const b = Number(m[2] ?? m[1]);
    if (a < 1 || b > pages || a > b) {
      bad = true;
      continue;
    }
    parts.push([a, b]);
  }
  return { parts, bad: bad || !parts.length };
}

/** Coupures avant certaines pages (signets) → parties consécutives. */
export function partsFromCuts(cuts: number[], pages: number): [number, number][] {
  const starts = Array.from(new Set([1, ...cuts.filter((c) => c > 1 && c <= pages)])).sort((a, b) => a - b);
  return starts.map((s, i) => [s, (starts[i + 1] ?? pages + 1) - 1]);
}

// ---------------------------------------------------------------- compression

export type Level = "light" | "rec" | "strong" | "custom";

export const PRESETS: Record<Exclude<Level, "custom">, CompressOptions> = {
  light: { dpi: 300, quality: 90, gray: false, subset: true, unembed: false, meta: false, thumbs: false, attach: false, compact: true },
  rec: { dpi: 150, quality: 75, gray: false, subset: true, unembed: false, meta: false, thumbs: true, attach: false, compact: true },
  strong: { dpi: 96, quality: 55, gray: false, subset: true, unembed: true, meta: true, thumbs: true, attach: true, compact: true },
};

export interface Estimate {
  images: number;
  fonts: number;
  content: number;
  meta: number;
  total: number;
}

/** Bits par pixel d'un JPEG couleur selon la qualité (ordre de grandeur mesuré). */
function jpegBpp(q: number) {
  return 0.6 + 3.4 * Math.pow(q / 100, 3);
}

/**
 * Taille estimée après compression, par catégorie, à partir de l'analyse réelle
 * du fichier (mêmes règles que le cœur Rust : rééchantillonnage selon la taille
 * affichée, réécriture seulement si le gain est réel).
 */
export function estimate(a: Analysis, o: CompressOptions): Estimate {
  let images = a.images - a.imageList.reduce((s, i) => s + i.bytes, 0);
  for (const img of a.imageList) {
    if (img.fixed) {
      images += img.bytes;
      continue;
    }
    const scale = img.ppi && img.ppi > o.dpi * 1.15 ? o.dpi / img.ppi : 1;
    const px = img.w * img.h * scale * scale;
    const gray = o.gray || img.comps === 1;
    const est = (px * jpegBpp(o.quality) * (gray ? 0.55 : 1)) / 8 + 300;
    images += scale >= 0.98 && est > img.bytes * 0.92 && !(o.gray && img.comps > 1) ? img.bytes : Math.min(img.bytes, est);
  }
  let fonts = a.fonts - a.fontList.reduce((s, f) => s + f.bytes, 0);
  for (const f of a.fontList) {
    if (o.unembed && f.standard) continue;
    fonts += o.subset && f.subsettable ? f.bytes * 0.35 : f.bytes;
  }
  const content = Math.max(a.content * 0.2, (a.content - a.uncompressed * 0.7) * (o.compact ? 0.88 : 1));
  const other = Math.max(0, a.meta - a.thumbs - a.attach);
  const meta = (o.meta ? Math.min(other, 400) : other) + (o.thumbs ? 0 : a.thumbs) + (o.attach ? 0 : a.attach);
  const r = { images, fonts, content, meta };
  return { ...r, total: Math.min(a.total, images + fonts + content + meta) };
}
