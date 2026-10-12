/**
 * État des outils Fusion / Division et Compresser (conservé quand on change
 * de module), et lancement des traitements dans le cœur Rust.
 */
import { create } from "zustand";
import i18n from "../i18n";
import { asAppError } from "../lib/api";
import { toast } from "../components/Toast";
import {
  estimate,
  newJob,
  parseRanges,
  partsFromCuts,
  PRESETS,
  toolsApi,
  type Analysis,
  type CompressOptions,
  type FileInfo,
  type Level,
  type MergeOptions,
  type Output,
} from "../lib/tools";

export type RunState =
  | { kind: "idle" }
  | { kind: "running"; job: number; step: number; done: number; total: number }
  | { kind: "done"; outputs: Output[] }
  | { kind: "error"; message: string };

const IMAGE_RE = /\.(png|jpe?g|gif|bmp|tiff?|webp)$/i;

export function baseName(p: string) {
  return p.split(/[\\/]/).pop() ?? p;
}
export function dirName(p: string) {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i > 0 ? p.slice(0, i) : p;
}
export function stemOf(p: string) {
  return baseName(p).replace(/\.[^.]+$/, "");
}

// ---------------------------------------------------------------- fusion et division

export interface MergeFile {
  id: number;
  path: string;
  name: string;
  pages: number;
  size: number;
  image: boolean;
}

export type SplitMethod = "ranges" | "size" | "marks";

interface MergeStore {
  tab: "merge" | "split";
  files: MergeFile[];
  outName: string;
  opts: MergeOptions;
  run: RunState;
  // Division
  source: FileInfo | null;
  method: SplitMethod;
  ranges: string;
  maxMb: number;
  sizeParts: [number, number][] | null;
  cuts: number[];
  set: (p: Partial<MergeStore>) => void;
  add: (paths: string[]) => Promise<void>;
  move: (id: number, to: number) => void;
  remove: (id: number) => void;
  setSource: (path: string) => Promise<void>;
  splitParts: () => { parts: [number, number][]; bad: boolean };
  runMerge: () => Promise<void>;
  runSplit: () => void;
  cancel: () => void;
}

let fileSeq = 0;

export const useMergeTool = create<MergeStore>((set, get) => {
  const track = (onDone: (outputs: Output[]) => void) =>
    newJob({
      progress: (p) => set((s) => (s.run.kind === "running" ? { run: { ...s.run, step: p.step, done: p.done, total: p.total } } : s)),
      done: (d) => {
        if (d.ok) {
          set({ run: { kind: "done", outputs: d.outputs } });
          onDone(d.outputs);
        } else if (d.cancelled) {
          set({ run: { kind: "idle" } });
          toast.info(i18n.t("tool.cancelled"));
        } else {
          set({ run: { kind: "error", message: d.message ?? "" } });
          toast.error(d.message ?? i18n.t("tool.failed"));
        }
      },
    });

  return {
    tab: "merge",
    files: [],
    outName: "",
    opts: { bookmarks: true, toc: false, a4: false },
    run: { kind: "idle" },
    source: null,
    method: "ranges",
    ranges: "",
    maxMb: 10,
    sizeParts: null,
    cuts: [],
    set: (p) => set(p),

    add: async (paths) => {
      const known = new Set(get().files.map((f) => f.path.toLowerCase()));
      const added: MergeFile[] = [];
      for (const path of paths) {
        if (known.has(path.toLowerCase())) continue;
        if (IMAGE_RE.test(path)) {
          added.push({ id: ++fileSeq, path, name: baseName(path), pages: 1, size: 0, image: true });
          continue;
        }
        try {
          const info = await toolsApi.inspect(path);
          if (info.encrypted) {
            toast.error(i18n.t("tool.locked", { name: info.name }));
            continue;
          }
          added.push({ id: ++fileSeq, path, name: info.name, pages: info.pages, size: info.size, image: false });
        } catch (e) {
          toast.error(asAppError(e).message);
        }
      }
      if (!added.length) return;
      set((s) => {
        // Nouveau contrôle : deux ajouts simultanés ne doivent pas créer de doublon.
        const present = new Set(s.files.map((f) => f.path.toLowerCase()));
        const fresh = added.filter((f) => !present.has(f.path.toLowerCase()));
        const files = [...s.files, ...fresh];
        return { files, outName: s.outName || `${stemOf(files[0].path)} (fusion)`, run: { kind: "idle" } };
      });
    },

    move: (id, to) =>
      set((s) => {
        const files = s.files.filter((f) => f.id !== id);
        const f = s.files.find((x) => x.id === id);
        if (!f) return s;
        files.splice(Math.max(0, Math.min(to, files.length)), 0, f);
        return { files, run: { kind: "idle" } };
      }),

    remove: (id) => set((s) => ({ files: s.files.filter((f) => f.id !== id), run: { kind: "idle" } })),

    setSource: async (path) => {
      try {
        const info = await toolsApi.inspect(path);
        if (info.encrypted) {
          toast.error(i18n.t("tool.locked", { name: info.name }));
          return;
        }
        const half = Math.max(1, Math.ceil(info.pages / 2));
        set({
          source: info,
          ranges: info.pages > 1 ? `1-${half}, ${half + 1}-${info.pages}` : "1",
          cuts: info.marks.filter((m) => m.depth === 0 && m.page > 0).map((m) => m.page + 1),
          sizeParts: null,
          run: { kind: "idle" },
          method: get().method === "marks" && !info.marks.length ? "ranges" : get().method,
        });
      } catch (e) {
        toast.error(asAppError(e).message);
      }
    },

    splitParts: () => {
      const s = get();
      const n = s.source?.pages ?? 0;
      if (!n) return { parts: [], bad: true };
      if (s.method === "ranges") return parseRanges(s.ranges, n);
      if (s.method === "size") return { parts: (s.sizeParts ?? []).map(([a, b]) => [a + 1, b + 1] as [number, number]), bad: !s.sizeParts };
      return { parts: partsFromCuts(s.cuts, n), bad: false };
    },

    runMerge: async () => {
      const s = get();
      if (s.files.length < 2 || s.run.kind === "running") return;
      const job = track(() => {});
      set({ run: { kind: "running", job, step: 0, done: 0, total: s.files.length } });
      try {
        await toolsApi.merge(job, s.files.map((f) => f.path), s.opts, dirName(s.files[0].path), s.outName || "Fusion");
      } catch (e) {
        set({ run: { kind: "error", message: asAppError(e).message } });
        toast.error(asAppError(e).message);
      }
    },

    runSplit: () => {
      const s = get();
      const src = s.source;
      const { parts, bad } = s.splitParts();
      if (!src || bad || !parts.length || s.run.kind === "running") return;
      const stem = stemOf(src.path);
      const names = parts.map(([a], i) => {
        if (s.method === "marks") {
          const m = src.marks.find((x) => x.page === a - 1 && x.depth === 0);
          if (m?.title) return `${stem} — ${m.title}`;
        }
        return `${stem} — partie ${i + 1}`;
      });
      const job = track(() => {});
      set({ run: { kind: "running", job, step: 0, done: 0, total: parts.length } });
      void toolsApi
        .split(job, src.path, parts.map(([a, b], i) => ({ start: a - 1, end: b - 1, name: names[i] })), dirName(src.path))
        .catch((e) => set({ run: { kind: "error", message: asAppError(e).message } }));
    },

    cancel: () => {
      const r = get().run;
      if (r.kind === "running") void toolsApi.cancel(r.job);
    },
  };
});

// ---------------------------------------------------------------- compression

interface CompressStore {
  source: FileInfo | null;
  analysis: Analysis | null;
  analyzing: boolean;
  level: Level;
  opts: CompressOptions;
  run: RunState;
  /** Analyse du fichier produit (rapport avant / après réel). */
  result: Analysis | null;
  setSource: (path: string) => Promise<void>;
  setLevel: (l: Level) => void;
  setOpt: (patch: Partial<CompressOptions>) => void;
  start: () => void;
  cancel: () => void;
}

export const useCompressTool = create<CompressStore>((set, get) => ({
  source: null,
  analysis: null,
  analyzing: false,
  level: "rec",
  opts: { ...PRESETS.rec },
  run: { kind: "idle" },
  result: null,

  setSource: async (path) => {
    set({ analyzing: true, result: null, run: { kind: "idle" } });
    try {
      const [info, analysis] = await Promise.all([toolsApi.inspect(path), toolsApi.analyze(path)]);
      set({ source: info, analysis, analyzing: false });
    } catch (e) {
      set({ analyzing: false });
      toast.error(asAppError(e).message);
    }
  },

  setLevel: (level) => set((s) => ({ level, opts: level === "custom" ? s.opts : { ...PRESETS[level] }, run: { kind: "idle" }, result: null })),
  setOpt: (patch) => set((s) => ({ opts: { ...s.opts, ...patch }, level: "custom", run: { kind: "idle" }, result: null })),

  start: () => {
    const s = get();
    if (!s.source || s.run.kind === "running") return;
    const job = newJob({
      progress: (p) => set((x) => (x.run.kind === "running" ? { run: { ...x.run, step: p.step, done: p.done, total: p.total } } : x)),
      done: (d) => {
        if (d.ok) {
          set({ run: { kind: "done", outputs: d.outputs } });
          void toolsApi
            .analyze(d.outputs[0].path)
            .then((result) => set({ result }))
            .catch(() => {});
        } else if (d.cancelled) {
          set({ run: { kind: "idle" } });
          toast.info(i18n.t("tool.cancelled"));
        } else {
          set({ run: { kind: "error", message: d.message ?? "" } });
          toast.error(d.message ?? i18n.t("tool.failed"));
        }
      },
    });
    set({ run: { kind: "running", job, step: 0, done: 0, total: 1 }, result: null });
    void toolsApi.compress(job, s.source.path, s.opts).catch((e) => {
      set({ run: { kind: "error", message: asAppError(e).message } });
      toast.error(asAppError(e).message);
    });
  },

  cancel: () => {
    const r = get().run;
    if (r.kind === "running") void toolsApi.cancel(r.job);
  },
}));

/** Estimation courante (ou null tant que l'analyse n'est pas prête). */
export function currentEstimate(s: Pick<CompressStore, "analysis" | "opts">) {
  return s.analysis ? estimate(s.analysis, s.opts) : null;
}
