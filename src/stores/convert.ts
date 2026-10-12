/**
 * État de l'outil Convertir et OCR : sens et format, file de fichiers avec
 * progression, options, reconnaissance de texte en direct.
 */
import { create } from "zustand";
import i18n from "../i18n";
import { asAppError } from "../lib/api";
import { convertApi, parseDetail, targetOf, type ConvertInfo, type Direction, type FormatId, type Lang, type OcrLine } from "../lib/convert";
import { newJob, toolsApi, type Output } from "../lib/tools";
import { toast } from "../components/Toast";

export interface QueueFile {
  path: string;
  pct: number;
  status: "wait" | "run" | "done" | "error";
}

export interface OcrPage {
  page: number;
  lines: OcrLine[];
  conf: number;
  skipped?: boolean;
}

interface ConvertStore {
  tab: "conv" | "ocr";
  dir: Direction;
  fmt: FormatId;
  imgFmt: "png" | "jpeg";
  dpi: number;
  keepLayout: boolean;
  autoOcr: boolean;
  perPage: boolean;
  files: QueueFile[];
  running: boolean;
  job: number | null;
  outputs: Output[] | null;
  info: ConvertInfo | null;
  // OCR
  langs: Lang[];
  searchable: boolean;
  deskew: boolean;
  ocrDoc: number | null;
  ocrPage: number | null;
  ocrPages: OcrPage[];
  ocrTotal: number;
  ocrDone: number;
  ocrWriting: boolean;
  ocrOutput: Output | null;
  set: (p: Partial<ConvertStore>) => void;
  setDir: (dir: Direction) => void;
  addFiles: (paths: string[]) => void;
  removeFile: (path: string) => void;
  loadInfo: () => Promise<void>;
  run: () => Promise<void>;
  runOcr: (doc: number, pages: number) => Promise<void>;
  cancel: () => void;
}

export const useConvert = create<ConvertStore>((set, get) => ({
  tab: "conv",
  dir: "from",
  fmt: "word",
  imgFmt: "png",
  dpi: 300,
  keepLayout: true,
  autoOcr: true,
  perPage: false,
  files: [],
  running: false,
  job: null,
  outputs: null,
  info: null,
  langs: ["fra", "eng"],
  searchable: true,
  deskew: true,
  ocrDoc: null,
  ocrPage: null,
  ocrPages: [],
  ocrTotal: 0,
  ocrDone: 0,
  ocrWriting: false,
  ocrOutput: null,
  set: (p) => set(p),

  setDir: (dir) => set({ dir, fmt: dir === "from" ? "word" : "img", files: [], outputs: null }),

  addFiles: (paths) =>
    set((s) => ({
      files: [...s.files, ...paths.filter((p) => !s.files.some((f) => f.path === p)).map((path) => ({ path, pct: 0, status: "wait" as const }))],
      outputs: null,
    })),

  removeFile: (path) => set((s) => ({ files: s.files.filter((f) => f.path !== path) })),

  loadInfo: async () => {
    if (get().info) return;
    try {
      set({ info: await convertApi.info() });
    } catch {
      set({ info: { word: false, excel: false, powerpoint: false } });
    }
  },

  run: async () => {
    const s = get();
    if (s.running || !s.files.length) return;
    const files = s.files.map((f) => f.path);
    set({ running: true, outputs: null, files: s.files.map((f) => ({ ...f, pct: 0, status: "wait" })) });
    const job = newJob({
      progress: (p) =>
        set((st) => ({
          files: st.files.map((f, i) =>
            i < p.step ? { ...f, pct: 100, status: "done" } : i === p.step ? { ...f, status: "run", pct: p.total ? Math.round((p.done / p.total) * 100) : f.pct } : f,
          ),
        })),
      done: (d) => {
        if (d.ok) {
          set((st) => ({ running: false, job: null, outputs: d.outputs, files: st.files.map((f) => ({ ...f, pct: 100, status: "done" })) }));
        } else {
          set((st) => ({ running: false, job: null, files: st.files.map((f) => (f.status === "run" ? { ...f, status: "error" } : f)) }));
          if (d.cancelled) toast.info(i18n.t("convert.cancelled"));
          else toast.error(d.message ?? i18n.t("convert.failed"));
        }
      },
    });
    set({ job });
    try {
      await convertApi.start(job, files, { target: targetOf(s.dir, s.fmt, s.imgFmt), dpi: s.dpi, keepLayout: s.keepLayout, autoOcr: s.autoOcr, perPage: s.perPage, langs: s.langs });
    } catch (e) {
      set({ running: false, job: null });
      toast.error(asAppError(e).message);
    }
  },

  runOcr: async (doc, pages) => {
    const s = get();
    if (s.running) return;
    set({ running: true, ocrDoc: doc, ocrPage: 0, ocrPages: [], ocrTotal: pages, ocrDone: 0, ocrWriting: false, ocrOutput: null });
    const job = newJob({
      progress: (p) => {
        const d = parseDetail(p.detail);
        if (d.writing) {
          set({ ocrWriting: true, ocrDone: p.total });
          return;
        }
        if (d.start && d.page !== undefined) {
          set({ ocrPage: d.page });
          return;
        }
        if (d.page !== undefined) {
          const page: OcrPage = { page: d.page, lines: d.lines ?? [], conf: d.conf ?? 0, skipped: d.skipped };
          set((st) => ({ ocrPages: [...st.ocrPages.filter((x) => x.page !== d.page), page], ocrDone: p.done, ocrPage: d.page ?? st.ocrPage }));
        }
      },
      done: (d) => {
        if (d.ok) set({ running: false, job: null, ocrWriting: false, ocrOutput: d.outputs[0] ?? null });
        else {
          set({ running: false, job: null, ocrWriting: false });
          if (d.cancelled) toast.info(i18n.t("convert.cancelled"));
          else toast.error(d.message ?? i18n.t("convert.ocrFailed"));
        }
      },
    });
    set({ job });
    try {
      await convertApi.ocr(job, doc, { langs: s.langs, searchable: s.searchable, deskew: s.deskew, allPages: false });
    } catch (e) {
      set({ running: false, job: null });
      toast.error(asAppError(e).message);
    }
  },

  cancel: () => {
    const j = get().job;
    if (j !== null) void toolsApi.cancel(j);
  },
}));
