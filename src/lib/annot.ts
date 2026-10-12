/**
 * Annotations côté interface : types (miroir de `src-tauri/src/annot/model.rs`),
 * commandes, et fonctions de géométrie (sélection de mots, boîtes).
 */
import { invoke } from "@tauri-apps/api/core";
import type { DocInfo, Rect, TextWord } from "./api";

export type AnnotKind = "highlight" | "underline" | "strike" | "squiggly" | "note" | "text" | "ink" | "arrow" | "rect" | "ellipse" | "line" | "stamp";

export interface Reply {
  author: string;
  text: string;
  date: string | null;
  saved: boolean;
}

export interface Annot {
  id: string;
  page: number;
  kind: AnnotKind;
  color: string;
  opacity: number;
  width: number;
  rects: Rect[];
  rect: Rect | null;
  ink: [number, number][][];
  line: [number, number, number, number] | null;
  text: string;
  fontSize: number;
  font: FontFamily;
  bold: boolean;
  italic: boolean;
  /** 0 gauche, 1 centré, 2 droite. */
  align: number;
  fill: string | null;
  borderColor: string | null;
  contents: string;
  author: string;
  modified: string | null;
  replies: Reply[];
  resolved: boolean;
  savedResolved: boolean;
  source: number | null;
  geomDirty: boolean;
  metaDirty: boolean;
  locked: boolean;
}

export type Tool = "select" | "highlight" | "underline" | "strike" | "note" | "text" | "pen" | "arrow" | "rect" | "ellipse" | "line";

export const MARKUP_TOOLS: Tool[] = ["highlight", "underline", "strike"];

/** Outils et raccourcis (dans le module Annoter). */
export const TOOLS: { id: Tool; key: string; sep?: boolean }[] = [
  { id: "select", key: "V" },
  { id: "highlight", key: "H", sep: true },
  { id: "underline", key: "U" },
  { id: "strike", key: "S" },
  { id: "note", key: "N", sep: true },
  { id: "text", key: "T" },
  { id: "pen", key: "P" },
  { id: "arrow", key: "A", sep: true },
  { id: "rect", key: "R" },
  { id: "ellipse", key: "E" },
  { id: "line", key: "L" },
];

export const COLORS = [
  { name: "ambre", hex: "#F2B84B" },
  { name: "vert", hex: "#4FCB91" },
  { name: "indigo", hex: "#8B87FA" },
  { name: "rose", hex: "#F07A9A" },
];

/** Couleurs de base du texte (lisibles sur le papier). */
export const TEXT_COLORS = [
  { name: "noir", hex: "#1A1B1E" },
  { name: "bleu", hex: "#2563C9" },
  { name: "rouge", hex: "#D2453D" },
  { name: "vert", hex: "#1F9D6A" },
];

export const STAMPS = [
  { label: "APPROUVÉ", color: "#1F9D6A", text: "#5FD9A0" },
  { label: "CONFIDENTIEL", color: "#D2453D", text: "#FF8C84" },
  { label: "BROUILLON", color: "#5B57E8", text: "#A9A6FF" },
  { label: "À REVOIR", color: "#C7871A", text: "#F2C064" },
];

export const WIDTHS = [1, 1.5, 2, 3];

// ---------------------------------------------------------------- zones de texte

/** « sans », « serif », « mono » (polices standard du PDF) ou nom d'une police installée. */
export type FontFamily = string;

/** Polices écran métriquement proches des polices standard PDF (Helvetica, Times, Courier). */
export const FONT_CSS: Record<"sans" | "serif" | "mono", string> = {
  sans: "Arial, Helvetica, sans-serif",
  serif: "'Times New Roman', Times, serif",
  mono: "'Courier New', Courier, monospace",
};

/** Pile CSS d'une police (famille standard ou police installée). */
export function fontCss(font: string): string {
  if (font === "sans" || font === "serif" || font === "mono") return FONT_CSS[font];
  return `"${font.replace(/"/g, "")}", Arial, sans-serif`;
}

/** Marge intérieure et interligne (identiques à l'apparence PDF, voir ap.rs). */
export const TEXT_PAD = 4;
export const LINE_HEIGHT = 1.25;

let measureCtx: CanvasRenderingContext2D | null = null;

/** Découpe le texte en lignes comme le fera l'apparence PDF. */
export function layoutText(a: Pick<Annot, "text" | "font" | "bold" | "italic" | "fontSize">, width: number): string[] {
  if (!measureCtx) measureCtx = document.createElement("canvas").getContext("2d");
  const ctx = measureCtx!;
  ctx.font = `${a.italic ? "italic " : ""}${a.bold ? "bold " : ""}${a.fontSize}px ${fontCss(a.font)}`;
  const max = Math.max(10, width - 2 * TEXT_PAD);
  const lines: string[] = [];
  for (const para of a.text.split("\n")) {
    let line = "";
    for (const word of para.split(" ")) {
      const cand = line ? `${line} ${word}` : word;
      if (line && ctx.measureText(cand).width > max) {
        lines.push(line);
        line = word;
      } else line = cand;
    }
    lines.push(line);
  }
  return lines;
}

/** Hauteur nécessaire à une zone de texte. */
export function textBoxHeight(a: Pick<Annot, "text" | "font" | "bold" | "italic" | "fontSize">, width: number): number {
  const n = Math.max(1, layoutText(a, width).length);
  return Math.ceil(2 * TEXT_PAD + n * a.fontSize * LINE_HEIGHT);
}

export function newId(): string {
  return `aura-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function blankAnnot(patch: Partial<Annot> & Pick<Annot, "page" | "kind">): Annot {
  return {
    id: newId(),
    color: "#F2B84B",
    opacity: 1,
    width: 1.5,
    rects: [],
    rect: null,
    ink: [],
    line: null,
    text: "",
    fontSize: 12,
    font: "sans",
    bold: false,
    italic: false,
    align: 0,
    fill: null,
    borderColor: null,
    contents: "",
    author: "",
    modified: null,
    replies: [],
    resolved: false,
    savedResolved: false,
    source: null,
    geomDirty: true,
    metaDirty: false,
    locked: false,
    ...patch,
  };
}

/** Rgb « #RRGGBB » + alpha → rgba(). */
export function rgba(hex: string, a: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${a})`;
}

// ---------------------------------------------------------------- géométrie

export function union(a: Rect, b: Rect): Rect {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
}

/** Boîte englobante d'une annotation (points d'affichage). */
export function bounds(a: Annot): Rect {
  switch (a.kind) {
    case "highlight":
    case "underline":
    case "strike":
    case "squiggly":
      return a.rects.reduce(union, a.rects[0] ?? { x: 0, y: 0, w: 0, h: 0 });
    case "ink": {
      const pts = a.ink.flat();
      if (!pts.length) return { x: 0, y: 0, w: 0, h: 0 };
      const xs = pts.map((p) => p[0]);
      const ys = pts.map((p) => p[1]);
      return { x: Math.min(...xs), y: Math.min(...ys), w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys) };
    }
    case "arrow":
    case "line": {
      const l = a.line ?? [0, 0, 0, 0];
      return { x: Math.min(l[0], l[2]), y: Math.min(l[1], l[3]), w: Math.abs(l[2] - l[0]), h: Math.abs(l[3] - l[1]) };
    }
    default:
      return a.rect ?? { x: 0, y: 0, w: 20, h: 20 };
  }
}

/** Déplace une annotation de (dx, dy). */
export function translate(a: Annot, dx: number, dy: number): Partial<Annot> {
  const mv = (r: Rect) => ({ ...r, x: r.x + dx, y: r.y + dy });
  return {
    rects: a.rects.map(mv),
    rect: a.rect ? mv(a.rect) : null,
    ink: a.ink.map((s) => s.map(([x, y]) => [x + dx, y + dy] as [number, number])),
    line: a.line ? [a.line[0] + dx, a.line[1] + dy, a.line[2] + dx, a.line[3] + dy] : null,
  };
}

/** Index du mot le plus proche d'un point (ou -1). */
export function nearestWord(words: TextWord[], x: number, y: number): number {
  let best = -1;
  let bestD = Infinity;
  words.forEach((w, i) => {
    const dx = x < w.x ? w.x - x : x > w.x + w.w ? x - (w.x + w.w) : 0;
    const dy = y < w.y ? w.y - y : y > w.y + w.h ? y - (w.y + w.h) : 0;
    // La distance verticale compte davantage : on reste sur la ligne visée.
    const d = dx + dy * 3;
    if (d < bestD) {
      bestD = d;
      best = i;
    }
  });
  return bestD < 60 ? best : -1;
}

/** Rectangles (un par ligne) couvrant les mots i..j inclus. */
export function wordRects(words: TextWord[], i: number, j: number): Rect[] {
  const [a, b] = i <= j ? [i, j] : [j, i];
  const out: Rect[] = [];
  for (let k = a; k <= b; k++) {
    const w = words[k];
    if (!w) continue;
    const last = out[out.length - 1];
    if (last && Math.abs(w.y - last.y) < last.h * 0.5 && w.x >= last.x - 1) {
      const x1 = Math.max(last.x + last.w, w.x + w.w);
      const y0 = Math.min(last.y, w.y);
      const y1 = Math.max(last.y + last.h, w.y + w.h);
      last.w = x1 - last.x;
      last.y = y0;
      last.h = y1 - y0;
    } else {
      out.push({ x: w.x, y: w.y, w: w.w, h: w.h });
    }
  }
  return out;
}

/** Texte des mots dont le centre est dans l'un des rectangles. */
export function quoteFor(words: TextWord[], rects: Rect[]): string {
  const inside = (w: TextWord) => {
    const cx = w.x + w.w / 2;
    const cy = w.y + w.h / 2;
    return rects.some((r) => cx >= r.x - 1 && cx <= r.x + r.w + 1 && cy >= r.y - 2 && cy <= r.y + r.h + 2);
  };
  return words
    .filter(inside)
    .map((w) => w.t)
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}

/** Simplifie un tracé (Ramer–Douglas–Peucker) pour alléger le PDF. */
export function simplify(pts: [number, number][], tol = 0.6): [number, number][] {
  if (pts.length < 3) return pts;
  const d2 = (p: [number, number], a: [number, number], b: [number, number]) => {
    const [x, y] = p;
    const [x1, y1] = a;
    const [x2, y2] = b;
    const dx = x2 - x1;
    const dy = y2 - y1;
    const len = dx * dx + dy * dy || 1;
    const t = Math.max(0, Math.min(1, ((x - x1) * dx + (y - y1) * dy) / len));
    const px = x1 + t * dx - x;
    const py = y1 + t * dy - y;
    return px * px + py * py;
  };
  let maxD = 0;
  let idx = 0;
  for (let i = 1; i < pts.length - 1; i++) {
    const d = d2(pts[i], pts[0], pts[pts.length - 1]);
    if (d > maxD) {
      maxD = d;
      idx = i;
    }
  }
  if (maxD <= tol * tol) return [pts[0], pts[pts.length - 1]];
  return [...simplify(pts.slice(0, idx + 1), tol).slice(0, -1), ...simplify(pts.slice(idx), tol)];
}

/** Pointe de flèche (identique au calcul Rust de l'apparence). */
export function arrowHead(l: [number, number, number, number], width: number): [number, number][] {
  const a = Math.atan2(l[3] - l[1], l[2] - l[0]);
  const len = 9 + 3 * width;
  const s = 0.5;
  return [
    [l[2] - len * Math.cos(a - s), l[3] - len * Math.sin(a - s)],
    [l[2], l[3]],
    [l[2] - len * Math.cos(a + s), l[3] - len * Math.sin(a + s)],
  ];
}

/** Taille d'un tampon (même formule approchée que le cœur Rust). */
export function stampSize(label: string): { w: number; h: number } {
  const fs = 14;
  // Largeur moyenne Helvetica gras en capitales ≈ 0,72 em.
  return { w: label.length * fs * 0.72 * 1.06 + fs * 0.12 * (label.length - 1) + 30, h: fs + 18 };
}

export const annotApi = {
  load: (doc: number) => invoke<Annot[]>("annot_load", { doc }),
  hide: (doc: number, page: number, indices: number[]) => invoke<void>("annot_hide", { doc, page, indices }),
  save: (doc: number, annots: Annot[]) => invoke<{ info: DocInfo; annots: Annot[] }>("annot_save", { doc, annots }),
};
