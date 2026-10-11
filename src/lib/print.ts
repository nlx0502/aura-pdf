/**
 * Impression : types partagés avec le cœur Rust (`src-tauri/src/print`) et accès
 * aux commandes. L'imposition est calculée par le cœur (source unique de vérité
 * pour l'aperçu et l'impression réelle).
 */
import { invoke } from "@tauri-apps/api/core";

export type Paper = "a4" | "a3" | "letter";
export type PrintMode = "size" | "poster" | "multi" | "booklet";
export type FitMode = "fit" | "actual" | "shrink" | "custom";
export type CommentsMode = "doc" | "docann" | "stamps";

export interface PrintSettings {
  copies: number;
  collate: boolean;
  range: "all" | "current" | "custom";
  current: number;
  custom: string;
  subset: "all" | "odd" | "even";
  reverse: boolean;
  mode: PrintMode;
  fit: FitMode;
  scale: number;
  perSheet: number;
  borders: boolean;
  bindRight: boolean;
  tile: number;
  cutMarks: boolean;
  paper: Paper;
  orient: "auto" | "portrait" | "landscape";
  duplex: boolean;
  shortEdge: boolean;
  gray: boolean;
  comments: CommentsMode;
  summary: boolean;
}

export const defaultPrintSettings: PrintSettings = {
  copies: 1,
  collate: true,
  range: "all",
  current: 0,
  custom: "1-4, 7",
  subset: "all",
  reverse: false,
  mode: "size",
  fit: "fit",
  scale: 100,
  perSheet: 4,
  borders: true,
  bindRight: false,
  tile: 200,
  cutMarks: true,
  paper: "a4",
  orient: "auto",
  duplex: false,
  shortEdge: false,
  gray: false,
  comments: "docann",
  summary: false,
};

export interface Placement {
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Sheet {
  w: number;
  h: number;
  items: Placement[];
  frames: [number, number, number, number][];
  marks: { x1: number; y1: number; x2: number; y2: number }[];
  blank: boolean;
  tile: [number, number, number, number] | null;
}

export interface PrintLayout {
  pages: number[];
  sheets: Sheet[];
  sides: number;
  physicalPerCopy: number;
  error: string | null;
}

export interface PrinterInfo {
  name: string;
  status: string;
  level: "ok" | "warn" | "error";
  isDefault: boolean;
  duplex: boolean;
  color: boolean;
}

export type PrintTarget = { kind: "pdf"; path: string } | { kind: "printer"; name: string };

export const printApi = {
  layout: (settings: PrintSettings, sizes: [number, number][], margin: number) => invoke<PrintLayout>("print_layout", { settings, sizes, margin }),
  printers: () => invoke<PrinterInfo[]>("list_printers"),
  margins: (name: string, paper: Paper, landscape: boolean) => invoke<{ margin: number; dpi: number }>("printer_margins", { name, paper, landscape }),
  start: (doc: number, job: number, settings: PrintSettings, margin: number, target: PrintTarget) =>
    invoke<void>("print_start", { doc, job, settings, margin, target }),
  cancel: (job: number) => invoke<void>("print_cancel", { job }),
};

/** Nombre de feuilles physiques imprimées (copies comprises). */
export function physicalSheets(layout: PrintLayout, settings: PrintSettings, summaryPages = 0): number {
  const sides = layout.sides + summaryPages;
  const perCopy = settings.duplex ? Math.ceil(sides / 2) : sides;
  return perCopy * Math.max(1, settings.copies);
}
