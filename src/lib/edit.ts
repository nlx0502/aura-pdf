/**
 * Module Éditer : types échangés avec le cœur Rust et fonctions pures
 * (sélection, déplacement de pages, plan d'enregistrement).
 */
import { invoke } from "@tauri-apps/api/core";
import type { DocInfo, Rect } from "./api";

// ---------------------------------------------------------------- types Rust

export type PageSrc = { kind: "orig"; index: number } | { kind: "blank"; w: number; h: number } | { kind: "file"; path: string; index: number };

export type LinkEdit = { op: "add"; rect: Rect; uri: string } | { op: "update"; index: number; uri: string } | { op: "delete"; index: number };

export interface PlanPage {
  src: PageSrc;
  rot: number;
  crop: boolean;
  redact: Rect[];
  links: LinkEdit[];
}

export interface Plan {
  pages: PlanPage[];
  header: { header: string; footer: string; size: number; color: string } | null;
  numbering: { format: string; position: NumPos; size: number; start: number; skipFirst: boolean; color: string } | null;
  watermark: { text: string; opacity: number; color: string; angle: number } | null;
  crop: { top: number; bottom: number; left: number; right: number } | null;
  cleanMetadata: boolean;
}

export interface LinkInfo {
  index: number;
  rect: Rect;
  uri: string | null;
  page: number | null;
}

export interface RedactHit {
  page: number;
  rects: Rect[];
  text: string;
  kind: string;
}

export interface ContentItem {
  kind: "text" | "image";
  ids: number[];
  rect: Rect;
  text: string;
  /** « sans », « serif », « mono » ou police installée. */
  font: string;
  bold: boolean;
  italic: boolean;
  size: number;
  color: string;
  align: number;
  lineHeight: number;
  px: [number, number] | null;
  baseline: number;
}

export interface TextSpec {
  text: string;
  rect: Rect;
  font: string;
  bold: boolean;
  italic: boolean;
  size: number;
  color: string;
  align: number;
  lineHeight: number;
  baseline?: number | null;
}

export type ContentOp =
  | { op: "replaceText"; page: number; ids: number[]; spec: TextSpec }
  | { op: "addText"; page: number; spec: TextSpec }
  | { op: "move"; page: number; ids: number[]; dx: number; dy: number }
  | { op: "delete"; page: number; ids: number[] }
  | { op: "imageReplace"; page: number; id: number; path: string }
  | { op: "imageRotate"; page: number; id: number }
  | { op: "imageResize"; page: number; id: number; rect: Rect }
  | { op: "imageCrop"; page: number; id: number; rect: Rect }
  | { op: "addImage"; page: number; rect: Rect; path: string };

export interface Layer {
  id: string;
  name: string;
  visible: boolean;
  locked: boolean;
  depth: number;
}

export const editApi = {
  apply: (doc: number, plan: Plan, out?: string) => invoke<DocInfo | null>("edit_apply", { doc, plan, out: out ?? null }),
  links: (doc: number) => invoke<LinkInfo[][]>("edit_links", { doc }),
  redactFind: (doc: number, patterns: string[], terms: string[]) => invoke<RedactHit[]>("redact_find", { doc, patterns, terms }),
  layers: (doc: number) => invoke<Layer[]>("layers_list", { doc }),
  setLayers: (doc: number, hidden: string[]) => invoke<void>("layers_set", { doc, hidden }),
  contentList: (doc: number, page: number) => invoke<ContentItem[]>("content_list", { doc, page }),
  contentApply: (doc: number, ops: ContentOp[], reset: boolean) => invoke<void>("content_apply", { doc, ops, reset }),
};

// ---------------------------------------------------------------- modèle de l'interface

export type NumPos = "tl" | "tc" | "tr" | "bl" | "bc" | "br";

/** Page dans l'éditeur (identifiant stable pour les animations et la sélection). */
export interface EditPage {
  key: string;
  src: PageSrc;
  /** Quarts de tour ajoutés. */
  rot: number;
  /** Taille affichée de la page source (rotation propre incluse), en points. */
  w: number;
  h: number;
  /** Document et page à afficher pour la miniature (null : page vierge). */
  view: { doc: number; page: number } | null;
  crop: boolean;
  redact: Rect[];
}

export interface EditSettings {
  header: { on: boolean; header: string; footer: string };
  numbering: { on: boolean; format: string; position: NumPos; skipFirst: boolean; start: number };
  watermark: { text: string; opacity: number; color: string };
  crop: { on: boolean; top: number; bottom: number; left: number; right: number; scope: "all" | "sel" };
}

export const NUM_FORMATS = ["{n}", "{n} / {total}", "Page {n} sur {total}"] as const;

export function defaultSettings(name: string): EditSettings {
  return {
    header: { on: false, header: name.replace(/\.pdf$/i, ""), footer: "" },
    numbering: { on: false, format: "{n} / {total}", position: "br", skipFirst: false, start: 1 },
    watermark: { text: "", opacity: 18, color: "#6E6AF6" },
    crop: { on: false, top: 12, bottom: 12, left: 18, right: 18, scope: "all" },
  };
}

export const MM = 72 / 25.4;

/**
 * Numéro affiché sur la page `i` (index 0) et dernier numéro, ou null pour une
 * page de garde non numérotée (mêmes règles que le cœur Rust).
 */
export function pageNumber(i: number, pages: number, start: number, skipFirst: boolean): { n: number; total: number } | null {
  const first = skipFirst ? 1 : 0;
  if (i < first) return null;
  return { n: start + i - first, total: start + Math.max(0, pages - first - 1) };
}

export function fillNumber(format: string, n: number, total: number) {
  return format.replace("{n}", String(n)).replace("{total}", String(total));
}

let keySeq = 0;
export function newKey() {
  return `p${++keySeq}`;
}

export function initialPages(info: DocInfo): EditPage[] {
  return info.pages.map((s, i) => ({
    key: newKey(),
    src: { kind: "orig", index: i },
    rot: 0,
    w: s.w,
    h: s.h,
    view: { doc: info.id, page: i },
    crop: false,
    redact: [],
  }));
}

/**
 * Sélection après un clic : Maj étend depuis la dernière page cliquée,
 * sinon le clic ajoute ou retire la page.
 */
export function toggleSelection(order: string[], sel: string[], last: string | null, key: string, shift: boolean): string[] {
  if (shift && last && order.includes(last)) {
    const a = order.indexOf(last);
    const b = order.indexOf(key);
    const range = order.slice(Math.min(a, b), Math.max(a, b) + 1);
    return Array.from(new Set([...sel, ...range]));
  }
  return sel.includes(key) ? sel.filter((k) => k !== key) : [...sel, key];
}

/** Déplace les pages sélectionnées d'un cran (-1 avant, +1 après), en bloc. */
export function moveSelected<T extends { key: string }>(pages: T[], sel: string[], dir: -1 | 1): T[] {
  const arr = pages.slice();
  const set = new Set(sel);
  const idx = arr.map((p, i) => (set.has(p.key) ? i : -1)).filter((i) => i >= 0);
  const order = dir < 0 ? idx : idx.slice().reverse();
  for (const i of order) {
    const j = i + dir;
    if (j < 0 || j >= arr.length || set.has(arr[j].key)) continue;
    [arr[i], arr[j]] = [arr[j], arr[i]];
  }
  return arr;
}

/** Place les pages `keys` (dans leur ordre actuel) avant l'index `to` de la liste d'origine. */
export function moveTo<T extends { key: string }>(pages: T[], keys: string[], to: number): T[] {
  const set = new Set(keys);
  const moving = pages.filter((p) => set.has(p.key));
  const before = pages.slice(0, to).filter((p) => !set.has(p.key)).length;
  const rest = pages.filter((p) => !set.has(p.key));
  return [...rest.slice(0, before), ...moving, ...rest.slice(before)];
}

/** Taille affichée d'une page après les quarts de tour ajoutés. */
export function shownSize(p: Pick<EditPage, "w" | "h" | "rot">) {
  return ((p.rot % 4) + 4) % 2 === 1 ? { w: p.h, h: p.w } : { w: p.w, h: p.h };
}

export function buildPlan(pages: EditPage[], s: EditSettings, sel: Set<string>, links: Map<number, LinkEdit[]>, clean = false): Plan {
  const cropOn = s.crop.on && (s.crop.top + s.crop.bottom + s.crop.left + s.crop.right > 0);
  return {
    pages: pages.map((p) => ({
      src: p.src,
      rot: p.rot,
      crop: cropOn && (s.crop.scope === "all" || sel.has(p.key) || p.crop),
      redact: p.redact,
      // Un lien tracé sans adresse n'est pas enregistré.
      links: p.src.kind === "orig" ? (links.get(p.src.index) ?? []).filter((e) => e.op !== "add" || e.uri.trim() !== "") : [],
    })),
    header: s.header.on && (s.header.header.trim() || s.header.footer.trim()) ? { header: s.header.header, footer: s.header.footer, size: 9, color: "#5A5B60" } : null,
    numbering: s.numbering.on ? { format: s.numbering.format, position: s.numbering.position, size: 9, start: s.numbering.start, skipFirst: s.numbering.skipFirst, color: "#3A3B40" } : null,
    watermark: s.watermark.text.trim() ? { text: s.watermark.text.trim(), opacity: s.watermark.opacity / 100, color: s.watermark.color, angle: 35 } : null,
    crop: cropOn ? { top: s.crop.top * MM, bottom: s.crop.bottom * MM, left: s.crop.left * MM, right: s.crop.right * MM } : null,
    cleanMetadata: clean,
  };
}

/** Rectangle normalisé à partir de deux coins. */
export function rectFrom(x1: number, y1: number, x2: number, y2: number): Rect {
  return { x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) };
}

const PAPERS: [string, number, number][] = [
  ["A4", 595.28, 841.89],
  ["A3", 841.89, 1190.55],
  ["A5", 419.53, 595.28],
  ["Lettre US", 612, 792],
  ["Légal US", 612, 1008],
];

/** Nom du format d'une page (« A4 ») ou ses dimensions en millimètres. */
export function paperName(w: number, h: number): string {
  const [a, b] = [Math.min(w, h), Math.max(w, h)];
  const hit = PAPERS.find(([, pw, ph]) => Math.abs(pw - a) < 3 && Math.abs(ph - b) < 3);
  if (hit) return hit[0];
  return `${Math.round(w / MM)} × ${Math.round(h / MM)} mm`;
}
