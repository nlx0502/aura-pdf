/**
 * État du module Éditer, par onglet : pages (ordre, rotations, insertions),
 * réglages (en-tête, numérotation, filigrane, rognage), zones à caviarder,
 * liens et opérations de contenu, avec annuler / rétablir.
 *
 * Rien n'est écrit avant « Enregistrer » : le cœur Rust reçoit alors le plan
 * complet et produit le nouveau fichier en une passe.
 */
import { create } from "zustand";
import i18n from "../i18n";
import { api, asAppError, type DocInfo, type Rect } from "../lib/api";
import {
  buildPlan,
  defaultSettings,
  editApi,
  initialPages,
  moveSelected,
  moveTo,
  newKey,
  type ContentOp,
  type EditPage,
  type EditSettings,
  type LinkEdit,
  type LinkInfo,
  type RedactHit,
} from "../lib/edit";
import { dropDocument } from "../lib/render";
import { dropPageData } from "../lib/pagedata";
import { toast } from "../components/Toast";
import { useApp } from "./app";
import { useAnnots } from "./annot";

export type EditMode = "pages" | "content";
export type RedactPattern = "iban" | "email" | "tel" | "card";

interface Snapshot {
  pages: EditPage[];
  settings: EditSettings;
  ops: ContentOp[];
  links: Record<number, LinkEdit[]>;
}

export interface DocEdit {
  loaded: boolean;
  /** Document chargé (l'identifiant change après chaque enregistrement). */
  docId: number;
  pages: EditPage[];
  settings: EditSettings;
  ops: ContentOp[];
  links: Record<number, LinkEdit[]>;
  origLinks: LinkInfo[][];
  past: Snapshot[];
  future: Snapshot[];
  /** Dernière modification fusionnable (saisie continue dans un champ). */
  lastTag: { tag: string; at: number } | null;
  sel: string[];
  last: string | null;
  leaving: string[];
  dirty: boolean;
  saving: boolean;
  /** Révision du rendu (le contenu a changé dans le moteur). */
  rev: number;
  mode: EditMode;
  /** Page (index d'origine) ouverte en mode Contenu. */
  contentPage: number;
  /** Fichiers ouverts pour l'insertion de pages : chemin → document. */
  imported: Record<string, DocInfo>;
  redact: { patterns: RedactPattern[]; terms: string; step: "idle" | "scan" | "done"; found: number; scanning: string[] };
}

interface EditStore {
  docs: Record<string, DocEdit>;
  ensure: (tabKey: string, info: DocInfo) => void;
  /** Modification annulable ; `tag` regroupe une saisie continue en une seule étape. */
  commit: (tabKey: string, fn: (d: DocEdit) => Partial<Snapshot>, tag?: string) => void;
  patch: (tabKey: string, p: Partial<DocEdit>) => void;
  setSettings: (tabKey: string, fn: (s: EditSettings) => EditSettings, tag?: string) => void;
  select: (tabKey: string, sel: string[], last?: string | null) => void;
  rotate: (tabKey: string, keys: string[], quarter?: number) => void;
  move: (tabKey: string, dir: -1 | 1) => void;
  moveTo: (tabKey: string, keys: string[], index: number) => void;
  remove: (tabKey: string, keys: string[]) => void;
  insertBlank: (tabKey: string) => void;
  insertFiles: (tabKey: string, paths: string[]) => Promise<void>;
  addRedact: (tabKey: string, origPage: number, rects: Rect[]) => void;
  clearRedact: (tabKey: string, pageKey: string, index?: number) => void;
  findRedactions: (tabKey: string) => Promise<void>;
  editLinks: (tabKey: string, origPage: number, fn: (l: LinkEdit[]) => LinkEdit[]) => void;
  applyContent: (tabKey: string, op: ContentOp) => Promise<boolean>;
  undo: (tabKey: string) => void;
  redo: (tabKey: string) => void;
  save: (tabKey: string) => Promise<boolean>;
  extract: (tabKey: string, keys: string[], out: string) => Promise<boolean>;
  discard: (tabKey: string) => void;
}

function fresh(info: DocInfo): DocEdit {
  return {
    loaded: true,
    docId: info.id,
    pages: initialPages(info),
    settings: defaultSettings(info.name),
    ops: [],
    links: {},
    origLinks: [],
    past: [],
    future: [],
    lastTag: null,
    sel: [],
    last: null,
    leaving: [],
    dirty: false,
    saving: false,
    rev: 0,
    mode: "pages",
    contentPage: 0,
    imported: {},
    redact: { patterns: ["iban", "email"], terms: "", step: "idle", found: 0, scanning: [] },
  };
}

function snap(d: DocEdit): Snapshot {
  return { pages: d.pages, settings: d.settings, ops: d.ops, links: d.links };
}

function tabInfo(tabKey: string) {
  return useApp.getState().tabs.find((t) => t.key === tabKey)?.info ?? null;
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const useEdit = create<EditStore>((set, get) => {
  const update = (tabKey: string, fn: (d: DocEdit) => Partial<DocEdit>) =>
    set((s) => {
      const cur = s.docs[tabKey];
      if (!cur) return s;
      return { docs: { ...s.docs, [tabKey]: { ...cur, ...fn(cur) } } };
    });

  /** Rétablit un état (annuler / rétablir) ; rejoue le contenu dans le moteur si besoin. */
  const restore = (tabKey: string, target: Snapshot, d: DocEdit) => {
    const opsChanged = target.ops !== d.ops;
    update(tabKey, () => ({ ...target, sel: d.sel.filter((k) => target.pages.some((p) => p.key === k)) }));
    if (opsChanged) {
      void editApi
        .contentApply(d.docId, target.ops, true)
        .then(() => update(tabKey, (x) => ({ rev: x.rev + 1 })))
        .catch((e) => toast.error(asAppError(e).message));
    }
  };

  return {
    docs: {},

    ensure: (tabKey, info) => {
      const cur = get().docs[tabKey];
      if (cur && cur.docId === info.id) return;
      set((s) => ({ docs: { ...s.docs, [tabKey]: fresh(info) } }));
      void editApi
        .links(info.id)
        .then((origLinks) => update(tabKey, () => ({ origLinks })))
        .catch(() => {});
    },

    commit: (tabKey, fn, tag) =>
      update(tabKey, (d) => {
        const now = Date.now();
        const merge = !!tag && d.lastTag?.tag === tag && now - d.lastTag.at < 1200;
        const next = fn(d);
        return {
          ...next,
          past: merge ? d.past : [...d.past.slice(-99), snap(d)],
          future: [],
          dirty: true,
          lastTag: tag ? { tag, at: now } : null,
        };
      }),

    patch: (tabKey, p) => update(tabKey, () => p),

    setSettings: (tabKey, fn, tag) => get().commit(tabKey, (d) => ({ settings: fn(d.settings) }), tag),

    select: (tabKey, sel, last) => update(tabKey, (d) => ({ sel, last: last === undefined ? d.last : last })),

    rotate: (tabKey, keys, quarter = 1) => {
      const set_ = new Set(keys);
      get().commit(tabKey, (d) => ({ pages: d.pages.map((p) => (set_.has(p.key) ? { ...p, rot: p.rot + quarter } : p)) }));
    },

    move: (tabKey, dir) => get().commit(tabKey, (d) => ({ pages: moveSelected(d.pages, d.sel, dir) })),

    moveTo: (tabKey, keys, index) => get().commit(tabKey, (d) => ({ pages: moveTo(d.pages, keys, index) })),

    remove: (tabKey, keys) => {
      const d = get().docs[tabKey];
      if (!d) return;
      if (keys.length >= d.pages.length) {
        toast.error(i18n.t("edit.keepOne"));
        return;
      }
      // Les pages se réduisent avant de disparaître.
      update(tabKey, () => ({ leaving: keys }));
      window.setTimeout(() => {
        get().commit(tabKey, (x) => ({ pages: x.pages.filter((p) => !keys.includes(p.key)) }));
        update(tabKey, (x) => ({ leaving: [], sel: x.sel.filter((k) => !keys.includes(k)) }));
        toast.success(i18n.t("edit.removed", { count: keys.length }));
      }, 260);
    },

    insertBlank: (tabKey) => {
      const d = get().docs[tabKey];
      if (!d) return;
      const lastSel = d.sel.length ? d.pages.findIndex((p) => p.key === d.sel[d.sel.length - 1]) : -1;
      const at = lastSel >= 0 ? lastSel + 1 : d.pages.length;
      const ref = d.pages[Math.max(0, at - 1)];
      const w = ref ? (ref.rot % 2 ? ref.h : ref.w) : 595.28;
      const h = ref ? (ref.rot % 2 ? ref.w : ref.h) : 841.89;
      const page: EditPage = { key: newKey(), src: { kind: "blank", w, h }, rot: 0, w, h, view: null, crop: false, redact: [] };
      get().commit(tabKey, (x) => {
        const pages = x.pages.slice();
        pages.splice(at, 0, page);
        return { pages };
      });
      update(tabKey, () => ({ sel: [page.key], last: page.key, mode: "pages" }));
    },

    insertFiles: async (tabKey, paths) => {
      const d = get().docs[tabKey];
      if (!d) return;
      const added: EditPage[] = [];
      const imported = { ...d.imported };
      for (const path of paths) {
        try {
          const info = imported[path] ?? (await api.openDocument(path));
          imported[path] = info;
          if (info.encrypted) {
            toast.error(i18n.t("edit.importLocked", { name: info.name }));
            continue;
          }
          info.pages.forEach((s, i) =>
            added.push({ key: newKey(), src: { kind: "file", path, index: i }, rot: 0, w: s.w, h: s.h, view: { doc: info.id, page: i }, crop: false, redact: [] }),
          );
        } catch (e) {
          toast.error(asAppError(e).message);
        }
      }
      if (!added.length) return;
      const cur = get().docs[tabKey]!;
      const lastSel = cur.sel.length ? cur.pages.findIndex((p) => p.key === cur.sel[cur.sel.length - 1]) : -1;
      const at = lastSel >= 0 ? lastSel + 1 : cur.pages.length;
      get().commit(tabKey, (x) => {
        const pages = x.pages.slice();
        pages.splice(at, 0, ...added);
        return { pages };
      });
      update(tabKey, () => ({ imported, sel: added.map((p) => p.key), last: added[added.length - 1].key, mode: "pages" }));
      toast.success(i18n.t("edit.inserted", { count: added.length }));
    },

    addRedact: (tabKey, origPage, rects) =>
      get().commit(tabKey, (d) => ({
        pages: d.pages.map((p) => (p.src.kind === "orig" && p.src.index === origPage ? { ...p, redact: [...p.redact, ...rects] } : p)),
      })),

    clearRedact: (tabKey, pageKey, index) =>
      get().commit(tabKey, (d) => ({
        pages: d.pages.map((p) => (p.key === pageKey ? { ...p, redact: index === undefined ? [] : p.redact.filter((_, i) => i !== index) } : p)),
      })),

    findRedactions: async (tabKey) => {
      const d = get().docs[tabKey];
      if (!d || d.redact.step === "scan") return;
      const terms = d.redact.terms
        .split(/[,;\n]/)
        .map((s) => s.trim())
        .filter(Boolean);
      if (!d.redact.patterns.length && !terms.length) return;
      update(tabKey, (x) => ({ redact: { ...x.redact, step: "scan", scanning: x.pages.map((p) => p.key) }, mode: "pages" }));
      const t0 = performance.now();
      let hits: RedactHit[] = [];
      try {
        hits = await editApi.redactFind(d.docId, d.redact.patterns, terms);
      } catch (e) {
        toast.error(asAppError(e).message);
        update(tabKey, (x) => ({ redact: { ...x.redact, step: "idle", scanning: [] } }));
        return;
      }
      const cur = get().docs[tabKey]!;
      const byPage = new Map<number, Rect[]>();
      for (const h of hits) byPage.set(h.page, [...(byPage.get(h.page) ?? []), ...h.rects.map((r) => ({ x: r.x - 1, y: r.y - 1, w: r.w + 2, h: r.h + 2 }))]);
      const keys = cur.pages.filter((p) => p.src.kind === "orig" && byPage.has(p.src.index)).map((p) => p.key);
      // Le faisceau balaie les pages concernées, puis les bandes apparaissent.
      update(tabKey, (x) => ({ redact: { ...x.redact, scanning: keys } }));
      await wait(Math.max(0, 1300 - (performance.now() - t0)));
      if (hits.length) {
        get().commit(tabKey, (x) => ({
          pages: x.pages.map((p) => {
            if (p.src.kind !== "orig") return p;
            const add = byPage.get(p.src.index);
            if (!add) return p;
            // Pas de doublon si la recherche est relancée.
            const fresh_ = add.filter((r) => !p.redact.some((q) => Math.abs(q.x - r.x) < 0.5 && Math.abs(q.y - r.y) < 0.5 && Math.abs(q.w - r.w) < 0.5));
            return { ...p, redact: [...p.redact, ...fresh_] };
          }),
        }));
      }
      update(tabKey, (x) => ({ redact: { ...x.redact, step: "done", found: hits.length, scanning: [] } }));
    },

    editLinks: (tabKey, origPage, fn) => get().commit(tabKey, (d) => ({ links: { ...d.links, [origPage]: fn(d.links[origPage] ?? []) } })),

    applyContent: async (tabKey, op) => {
      const d = get().docs[tabKey];
      if (!d) return false;
      try {
        await editApi.contentApply(d.docId, [op], false);
      } catch (e) {
        toast.error(asAppError(e).message);
        return false;
      }
      get().commit(tabKey, (x) => ({ ops: [...x.ops, op] }));
      update(tabKey, (x) => ({ rev: x.rev + 1 }));
      return true;
    },

    undo: (tabKey) => {
      const d = get().docs[tabKey];
      const prev = d?.past[d.past.length - 1];
      if (!d || !prev) return;
      update(tabKey, () => ({ past: d.past.slice(0, -1), future: [snap(d), ...d.future], dirty: d.past.length > 1, lastTag: null }));
      restore(tabKey, prev, d);
    },

    redo: (tabKey) => {
      const d = get().docs[tabKey];
      const next = d?.future[0];
      if (!d || !next) return;
      update(tabKey, () => ({ past: [...d.past, snap(d)], future: d.future.slice(1), dirty: true, lastTag: null }));
      restore(tabKey, next, d);
    },

    save: async (tabKey) => {
      const d = get().docs[tabKey];
      const info = tabInfo(tabKey);
      if (!d || !info) return false;
      if (!d.dirty) return true;
      // Les annotations en attente sont d'abord écrites dans le fichier.
      const annots = useAnnots.getState().docs[tabKey];
      if (annots?.dirty) {
        const ok = await useAnnots.getState().save(tabKey);
        if (!ok) return false;
        // Le document a été rechargé depuis le fichier : rejouer le contenu modifié.
        if (d.ops.length) {
          try {
            await editApi.contentApply(tabInfo(tabKey)!.id, d.ops, true);
          } catch (e) {
            toast.error(asAppError(e).message);
            return false;
          }
        }
      }
      const docId = tabInfo(tabKey)!.id;
      update(tabKey, () => ({ saving: true }));
      const links = new Map<number, LinkEdit[]>(Object.entries(d.links).map(([k, v]) => [Number(k), v]));
      const plan = buildPlan(d.pages, d.settings, new Set(d.sel), links, d.pages.some((p) => p.redact.length > 0));
      try {
        const res = await editApi.apply(docId, plan);
        if (!res) throw new Error("document non rouvert");
        dropDocument(docId);
        dropPageData(docId);
        for (const imp of Object.values(d.imported)) {
          void api.closeDocument(imp.id).catch(() => {});
          dropDocument(imp.id);
        }
        const app = useApp.getState();
        app.updateTab(tabKey, (t) => {
          const page = Math.min(t.page, res.pageCount - 1);
          return { info: res, page, jump: { page, seq: t.jump.seq + 1 }, search: { ...t.search, hits: [], query: "", active: -1 } };
        });
        // Les annotations sont relues depuis le nouveau fichier.
        useAnnots.getState().discard(tabKey);
        const mode = d.mode;
        const contentPage = Math.min(d.contentPage, res.pageCount - 1);
        set((s) => ({ docs: { ...s.docs, [tabKey]: { ...fresh(res), mode, contentPage } } }));
        void editApi
          .links(res.id)
          .then((origLinks) => update(tabKey, () => ({ origLinks })))
          .catch(() => {});
        toast.success(i18n.t("edit.saved"));
        return true;
      } catch (e) {
        update(tabKey, () => ({ saving: false }));
        toast.error(asAppError(e).message);
        return false;
      }
    },

    extract: async (tabKey, keys, out) => {
      const d = get().docs[tabKey];
      const info = tabInfo(tabKey);
      if (!d || !info) return false;
      const set_ = new Set(keys);
      const pages = d.pages.filter((p) => set_.has(p.key));
      const links = new Map<number, LinkEdit[]>(Object.entries(d.links).map(([k, v]) => [Number(k), v]));
      const plan = buildPlan(pages, d.settings, set_, links, pages.some((p) => p.redact.length > 0));
      try {
        await editApi.apply(info.id, plan, out);
        toast.show({
          kind: "success",
          message: i18n.t("edit.extracted", { count: pages.length }),
          actionLabel: i18n.t("edit.openExtract"),
          onAction: () => void useApp.getState().openPath(out),
          duration: 7000,
        });
        return true;
      } catch (e) {
        toast.error(asAppError(e).message);
        return false;
      }
    },

    discard: (tabKey) => {
      const d = get().docs[tabKey];
      if (!d) return;
      for (const imp of Object.values(d.imported)) {
        void api.closeDocument(imp.id).catch(() => {});
        dropDocument(imp.id);
      }
      // Le contenu modifié en mémoire est abandonné : on repart du fichier.
      if (d.ops.length) void editApi.contentApply(d.docId, [], true).catch(() => {});
      set((s) => {
        const docs = { ...s.docs };
        delete docs[tabKey];
        return { docs };
      });
    },
  };
});

export function useDocEdit(tabKey: string | undefined): DocEdit | null {
  return useEdit((s) => (tabKey ? (s.docs[tabKey] ?? null) : null));
}

/** Onglets dont les modifications d'édition ne sont pas enregistrées. */
export function dirtyEditTabs(): string[] {
  const docs = useEdit.getState().docs;
  return Object.keys(docs).filter((k) => docs[k].dirty);
}
