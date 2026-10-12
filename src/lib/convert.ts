/**
 * Outil Convertir et OCR : formats, API, analyse de la progression de l'OCR.
 */
import { invoke } from "@tauri-apps/api/core";
import { newJob, type Output } from "./tools";

export type Direction = "from" | "to";
export type FormatId = "img" | "word" | "excel" | "txt" | "office";

export interface FormatCard {
  id: FormatId;
  short: string;
  bg: string;
  fg: string;
  /** Extensions acceptées (vers un PDF). */
  ext?: string[];
}

/** Depuis un PDF (maquette Convert.dc.html). */
export const FROM: FormatCard[] = [
  { id: "img", short: "PNG", bg: "#E8E7FF", fg: "#4B47D6" },
  { id: "word", short: "DOCX", bg: "#DCEBFF", fg: "#2563C9" },
  { id: "excel", short: "XLSX", bg: "#D9F5E6", fg: "#178A55" },
  { id: "txt", short: "TXT", bg: "#ECEBE6", fg: "#55534D" },
];

/** Vers un PDF. */
export const TO: FormatCard[] = [
  { id: "img", short: "JPG", bg: "#E8E7FF", fg: "#4B47D6", ext: ["jpg", "jpeg", "png", "tif", "tiff", "bmp", "gif", "webp"] },
  { id: "word", short: "DOCX", bg: "#DCEBFF", fg: "#2563C9", ext: ["docx", "doc", "rtf", "odt"] },
  { id: "office", short: "XLSX", bg: "#D9F5E6", fg: "#178A55", ext: ["xlsx", "xls", "ods", "csv", "pptx", "ppt", "odp"] },
];

export const LANGS = ["fra", "eng", "deu", "nld"] as const;
export type Lang = (typeof LANGS)[number];

export interface ConvertSpec {
  target: "png" | "jpeg" | "docx" | "xlsx" | "txt" | "pdf";
  dpi: number;
  keepLayout: boolean;
  autoOcr: boolean;
  perPage: boolean;
  langs: string[];
}

export interface OcrSpec {
  langs: string[];
  searchable: boolean;
  deskew: boolean;
  allPages: boolean;
}

export interface ConvertInfo {
  word: boolean;
  excel: boolean;
  powerpoint: boolean;
}

export const convertApi = {
  info: () => invoke<ConvertInfo>("convert_info"),
  start: (job: number, files: string[], spec: ConvertSpec) => invoke<void>("convert_start", { job, files, spec }),
  ocr: (job: number, doc: number, spec: OcrSpec) => invoke<void>("ocr_start", { job, doc, spec }),
  scan: () => invoke<string | null>("scan_acquire"),
  document: (job: number, input: string, out: string) => invoke<void>("document_pdf", { job, input, out }),
  createOcr: (job: number, spec: unknown, out: string, langs: string[]) => invoke<void>("create_pdf_ocr", { job, spec, out, langs }),
  clipboard: (out: string) => invoke<void>("clipboard_pdf", { out }),
};

/** Cible de conversion selon la carte choisie. */
export function targetOf(dir: Direction, fmt: FormatId, imgFmt: "png" | "jpeg"): ConvertSpec["target"] {
  if (dir === "to") return "pdf";
  return fmt === "img" ? imgFmt : fmt === "word" ? "docx" : fmt === "excel" ? "xlsx" : "txt";
}

export const extOf = (path: string) => (path.match(/\.([^.\\/]+)$/)?.[1] ?? "").toLowerCase();

/** Couleur de la pastille d'un fichier selon son extension. */
export function badgeOf(path: string): { ext: string; c: string } {
  const e = extOf(path);
  if (e === "pdf") return { ext: "PDF", c: "#D2453D" };
  if (["doc", "docx", "rtf", "odt"].includes(e)) return { ext: e.toUpperCase(), c: "#2563C9" };
  if (["xls", "xlsx", "ods", "csv"].includes(e)) return { ext: e.toUpperCase(), c: "#178A55" };
  if (["ppt", "pptx", "odp"].includes(e)) return { ext: e.toUpperCase(), c: "#C4522B" };
  return { ext: e.toUpperCase().slice(0, 4) || "?", c: "#4B47D6" };
}

/** Ligne reconnue publiée par l'OCR : texte, boîte relative à la page, confiance. */
export interface OcrLine {
  t: string;
  b: [number, number, number, number];
  c: number;
}

/** Détail d'une progression OCR (JSON dans `detail`). */
export interface OcrDetail {
  page?: number;
  start?: boolean;
  skipped?: boolean;
  lines?: OcrLine[];
  conf?: number;
  skew?: number;
  writing?: boolean;
}

export function parseDetail(s: string): OcrDetail {
  try {
    return JSON.parse(s) as OcrDetail;
  } catch {
    return {};
  }
}

/** Lance une tâche et attend sa fin (fichiers produits). */
export async function runJob(start: (job: number) => Promise<unknown>, onProgress?: (done: number, total: number) => void): Promise<Output[]> {
  return new Promise((resolve, reject) => {
    const job = newJob({
      progress: (p) => onProgress?.(p.done, p.total),
      done: (d) => (d.ok ? resolve(d.outputs) : reject(new Error(d.cancelled ? "annulé" : (d.message ?? "échec")))),
    });
    start(job).catch(reject);
  });
}
