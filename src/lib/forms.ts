/**
 * Formulaires AcroForm : champs d'un document (valeur, propriétés, apparence,
 * libellés visibles), enregistrement, aplatissement.
 */
import { invoke } from "@tauri-apps/api/core";
import type { DocInfo, Rect } from "./api";

export type FieldKind = "text" | "email" | "date" | "select" | "list" | "radio" | "check" | "sig" | "button";
/** Types que l'on peut ajouter. */
export const ADDABLE: FieldKind[] = ["text", "email", "date", "select", "check", "radio", "sig"];

export type CheckStyle = "check" | "cross" | "circle" | "square" | "star" | "diamond";
export type BorderStyle = "solid" | "dashed" | "underline";
export type FontFamily = "sans" | "serif" | "mono";
export type CaptionPos = "above" | "left" | "right";

export interface FieldStyle {
  background: string | null;
  border: string | null;
  borderWidth: number;
  borderStyle: BorderStyle;
  textColor: string;
  font: FontFamily;
  bold: boolean;
  /** 0 : automatique. */
  fontSize: number;
  align: 0 | 1 | 2;
  checkStyle: CheckStyle;
}

export interface Caption {
  text: string;
  rect: Rect;
  size: number;
  color: string;
  bold: boolean;
  page: number;
  index?: number | null;
}

export interface FieldOption {
  value: string;
  label: string;
}
export interface Widget {
  page: number;
  rect: Rect;
  on: string | null;
  caption: Caption | null;
  index?: number | null;
}
export interface FormField {
  name: string;
  isNew?: boolean;
  label: string;
  kind: FieldKind;
  value: string;
  options: FieldOption[];
  required: boolean;
  readonly: boolean;
  multiline: boolean;
  maxLen: number | null;
  placeholder: string;
  style: FieldStyle;
  widgets: Widget[];
  caption: Caption | null;
}

export interface FormPlan {
  upsert: FormField[];
  delete: string[];
}

export const formsApi = {
  list: (doc: number) => invoke<FormField[]>("forms_list", { doc }),
  save: (doc: number, plan: FormPlan) => invoke<DocInfo>("forms_save", { doc, plan }),
  flatten: (doc: number) => invoke<[DocInfo, string]>("forms_flatten", { doc }),
  restore: (doc: number, backup: string) => invoke<DocInfo>("forms_restore", { doc, backup }),
};

/** Icônes des types (maquette Forms.dc.html). */
export const KIND_ICON: Record<FieldKind, string> = {
  text: "M3 3.5h10 M8 3.5v10 M6 13.5h4",
  email: "M2 4h12v8H2z M2 4.5l6 4.5 6-4.5",
  date: "M2.5 3.5h11v10h-11z M2.5 6.5h11 M5.5 2v3 M10.5 2v3",
  select: "M2.5 4h11v8h-11z M9.5 7l1.5 1.5L12.5 7",
  list: "M2.5 4h11v8h-11z M9.5 7l1.5 1.5L12.5 7",
  radio: "M5 8a3 3 0 1 0 6 0 3 3 0 0 0-6 0z M7.5 8h1",
  check: "M2.5 2.5h11v11h-11z M5 8l2 2 4-4.5",
  sig: "M2 11.5c2-3 3.5-7 5-7s-1 7 1 7 2.5-3 3.5-3 1 2 2.5 2 M2 14h12",
  button: "M2.5 5h11v6h-11z",
};

/** Symboles des cases (équivalents des caractères ZapfDingbats écrits dans le PDF). */
export const CHECK_GLYPH: Record<CheckStyle, string> = { check: "✔", cross: "✖", circle: "●", square: "■", star: "★", diamond: "◆" };

export const FONT_CSS: Record<FontFamily, string> = {
  sans: "Helvetica, Arial, sans-serif",
  serif: "'Times New Roman', Times, serif",
  mono: "'Courier New', Courier, monospace",
};

export const CAPTION_COLOR = "#3a3b40";

export function defaultStyle(kind: FieldKind): FieldStyle {
  return {
    background: kind === "sig" ? null : "#ffffff",
    border: "#c9c7be",
    borderWidth: 1,
    borderStyle: kind === "sig" ? "dashed" : "solid",
    textColor: "#1a1b1e",
    font: "sans",
    bold: false,
    fontSize: 0,
    align: 0,
    checkStyle: kind === "radio" ? "circle" : "check",
  };
}

/** Taille par défaut d'un nouveau champ (points). */
export function defaultSize(kind: FieldKind): { w: number; h: number } {
  if (kind === "check") return { w: 14, h: 14 };
  if (kind === "radio") return { w: 220, h: 14 };
  if (kind === "sig") return { w: 200, h: 60 };
  return { w: 220, h: 22 };
}

/** Taille de police affichée (automatique : selon la hauteur, comme le PDF). */
export const fontSizeOf = (s: FieldStyle, h: number, multiline = false) => (s.fontSize > 0 ? s.fontSize : multiline ? 10 : Math.max(4, Math.min(12, h * 0.62)));

let canvas: HTMLCanvasElement | null = null;
/** Largeur d'un texte en Helvetica (Arial a les mêmes métriques). */
export function textWidth(text: string, size: number, bold: boolean): number {
  if (typeof document === "undefined") return text.length * size * 0.55;
  canvas ??= document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return text.length * size * 0.55;
  ctx.font = `${bold ? "bold " : ""}${size}px Helvetica, Arial, sans-serif`;
  return ctx.measureText(text).width;
}

const CAPTION_GAP = 4;

/** Libellé visible placé autour d'un rectangle. */
export function placeCaption(text: string, page: number, r: Rect, pos: CaptionPos, size = 10, bold = true, color = CAPTION_COLOR): Caption {
  const w = Math.ceil(textWidth(text, size, bold)) + 2;
  const h = Math.ceil(size * 1.3);
  const rect =
    pos === "above"
      ? { x: r.x, y: r.y - h - 2, w, h }
      : pos === "left"
        ? { x: r.x - w - CAPTION_GAP * 2, y: r.y + (r.h - h) / 2, w, h }
        : { x: r.x + r.w + CAPTION_GAP * 2, y: r.y + (r.h - h) / 2, w, h };
  return { text, rect, size, color, bold, page };
}

/** Position d'un libellé par rapport à son champ. */
export function captionPos(c: Caption, r: Rect): CaptionPos {
  if (c.rect.y + c.rect.h <= r.y + 2) return "above";
  return c.rect.x >= r.x + r.w - 1 ? "right" : "left";
}

/** Champ rempli (une signature compte quand elle est apposée). */
export const isFilled = (f: Pick<FormField, "kind" | "value">) => f.kind !== "button" && !!f.value;

/** Champs comptés dans la progression (les boutons d'action sont ignorés). */
export const countable = (fields: FormField[]) => fields.filter((f) => f.kind !== "button");

export function progress(fields: FormField[]) {
  const list = countable(fields);
  const filled = list.filter(isFilled).length;
  const missing = list.filter((f) => f.required && !isFilled(f)).length;
  return { total: list.length, filled, missing, pct: list.length ? Math.round((filled / list.length) * 100) : 0 };
}

/** jj/mm/aaaa ↔ aaaa-mm-jj (champ date HTML). */
export function toIsoDate(v: string): string {
  const m = v.trim().match(/^(\d{1,2})[/.-](\d{1,2})[/.-](\d{4})$/);
  return m ? `${m[3]}-${m[2].padStart(2, "0")}-${m[1].padStart(2, "0")}` : "";
}
export function fromIsoDate(v: string): string {
  const m = v.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  return m ? `${m[3]}/${m[2]}/${m[1]}` : "";
}

export const validEmail = (v: string) => !v || /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v);

/** Nom d'état PDF sûr pour une option de bouton radio. */
export const stateName = (label: string, i: number) =>
  label
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^A-Za-z0-9]+/g, "")
    .slice(0, 30) || `Choix${i + 1}`;

/** Disposition des boutons d'un groupe radio : une ligne, libellé à droite de chaque bouton. */
export function layoutRadio(options: FieldOption[], page: number, origin: { x: number; y: number }, size: number, captionSize = 10): Widget[] {
  let x = origin.x;
  return options.map((o) => {
    const rect = { x, y: origin.y, w: size, h: size };
    const caption = placeCaption(o.label, page, rect, "right", captionSize, false);
    x = caption.rect.x + caption.rect.w + 18;
    return { page, rect, on: o.value, caption };
  });
}

/** Champ modifié par rapport à sa version enregistrée. */
export function changed(a: FormField, b: FormField): boolean {
  const strip = (f: FormField) => JSON.stringify({ ...f, widgets: f.widgets.map((w) => ({ ...w, index: null, caption: w.caption ? { ...w.caption, index: null } : null })), caption: f.caption ? { ...f.caption, index: null } : null });
  return strip(a) !== strip(b);
}
