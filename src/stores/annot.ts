/**
 * État des annotations, par onglet : liste courante, historique
 * (annuler / rétablir), sélection, modifications non enregistrées.
 */
import { create } from "zustand";
import i18n from "../i18n";
import { annotApi, quoteFor, type Annot, type FontFamily, type Tool } from "../lib/annot";
import { pageWords } from "../lib/pagedata";
import { asAppError } from "../lib/api";
import { toast } from "../components/Toast";
import { useApp } from "./app";

export interface DocAnnots {
  loaded: boolean;
  list: Annot[];
  /** Annotations lues dans le fichier (pour savoir quoi masquer dans le rendu). */
  originals: Annot[];
  past: Annot[][];
  future: Annot[][];
  selected: string | null;
  dirty: boolean;
  /** Révision du rendu des pages (change quand PDFium doit redessiner). */
  rev: number;
  saving: boolean;
}

interface Style {
  color: string;
  opacity: number;
  width: number;
}

export interface TextStyle {
  font: FontFamily;
  fontSize: number;
  bold: boolean;
  italic: boolean;
  align: number;
  color: string;
  fill: string | null;
  borderColor: string | null;
}

interface AnnotStore {
  docs: Record<string, DocAnnots>;
  tool: Tool;
  style: Style;
  /** Style des prochaines zones de texte : texte nu, noir, par défaut. */
  textStyle: TextStyle;
  setTextStyle: (patch: Partial<TextStyle>) => void;
  /** Opacité propre au surlignage (plus transparent par défaut). */
  highlightOpacity: number;
  /** Zone de texte en cours de saisie. */
  editing: string | null;

  ensure: (tabKey: string, docId: number) => Promise<void>;
  setTool: (t: Tool) => void;
  setStyle: (patch: Partial<Style>) => void;
  setHighlightOpacity: (v: number) => void;
  /** Modification enregistrée dans l'historique. */
  commit: (tabKey: string, fn: (list: Annot[]) => Annot[], select?: string | null) => void;
  /** Modification sans historique (glisser en cours). */
  preview: (tabKey: string, fn: (list: Annot[]) => Annot[]) => void;
  select: (tabKey: string, id: string | null) => void;
  setEditing: (id: string | null) => void;
  undo: (tabKey: string) => void;
  redo: (tabKey: string) => void;
  save: (tabKey: string) => Promise<boolean>;
  discard: (tabKey: string) => void;
}

/** Citations des surlignages relus depuis le fichier (le PDF ne stocke que leur position). */
async function fillQuotes(tabKey: string, docId: number) {
  const d = useAnnots.getState().docs[tabKey];
  if (!d) return;
  const missing = d.list.filter((a) => /highlight|underline|strike|squiggly/.test(a.kind) && !a.text);
  if (!missing.length) return;
  const quotes = new Map<string, string>();
  for (const page of new Set(missing.map((a) => a.page))) {
    const words = await pageWords(docId, page);
    for (const a of missing.filter((m) => m.page === page)) quotes.set(a.id, quoteFor(words, a.rects));
  }
  const fill = (l: Annot[]) => l.map((a) => (quotes.has(a.id) && !a.text ? { ...a, text: quotes.get(a.id)! } : a));
  useAnnots.setState((s) => {
    const cur = s.docs[tabKey];
    if (!cur) return s;
    return { docs: { ...s.docs, [tabKey]: { ...cur, list: fill(cur.list), originals: fill(cur.originals) } } };
  });
}

const empty = (): DocAnnots => ({ loaded: false, list: [], originals: [], past: [], future: [], selected: null, dirty: false, rev: 0, saving: false });

/** Synchronise avec le moteur les annotations d'origine à ne plus dessiner. */
const hiddenSent = new Map<string, string>();
function syncHidden(tabKey: string, docId: number, d: DocAnnots): boolean {
  const byPage = new Map<number, number[]>();
  for (const o of d.originals) {
    if (o.source === null) continue;
    const cur = d.list.find((a) => a.id === o.id);
    if (!cur || cur.geomDirty) {
      if (!byPage.has(o.page)) byPage.set(o.page, []);
      byPage.get(o.page)!.push(o.source);
    }
  }
  let changed = false;
  const pages = new Set([...byPage.keys(), ...[...hiddenSent.keys()].filter((k) => k.startsWith(`${tabKey}:`)).map((k) => +k.split(":")[1])]);
  for (const p of pages) {
    const key = `${tabKey}:${p}`;
    const val = (byPage.get(p) ?? []).sort((a, b) => a - b).join(",");
    if ((hiddenSent.get(key) ?? "") !== val) {
      void annotApi.hide(docId, p, byPage.get(p) ?? []).catch(() => {});
      if (val) hiddenSent.set(key, val);
      else hiddenSent.delete(key);
      changed = true;
    }
  }
  return changed;
}

function docIdOf(tabKey: string): number | null {
  return useApp.getState().tabs.find((t) => t.key === tabKey)?.info.id ?? null;
}

export const useAnnots = create<AnnotStore>((set, get) => {
  const update = (tabKey: string, fn: (d: DocAnnots) => Partial<DocAnnots>) => {
    set((s) => {
      const cur = s.docs[tabKey] ?? empty();
      const next = { ...cur, ...fn(cur) };
      const id = docIdOf(tabKey);
      if (id !== null && next.loaded && syncHidden(tabKey, id, next)) next.rev = cur.rev + 1;
      return { docs: { ...s.docs, [tabKey]: next } };
    });
  };

  return {
    docs: {},
    tool: "select",
    style: { color: "#F2B84B", opacity: 1, width: 1.5 },
    textStyle: { font: "sans", fontSize: 12, bold: false, italic: false, align: 0, color: "#1A1B1E", fill: null, borderColor: null },
    setTextStyle: (patch) => set((s) => ({ textStyle: { ...s.textStyle, ...patch } })),
    highlightOpacity: 0.45,
    editing: null,

    ensure: async (tabKey, docId) => {
      const cur = get().docs[tabKey];
      if (cur?.loaded) return;
      try {
        const list = await annotApi.load(docId);
        set((s) => ({ docs: { ...s.docs, [tabKey]: { ...empty(), loaded: true, list, originals: list } } }));
        void fillQuotes(tabKey, docId);
      } catch (e) {
        toast.error(asAppError(e).message);
        set((s) => ({ docs: { ...s.docs, [tabKey]: { ...empty(), loaded: true } } }));
      }
    },

    setTool: (tool) => set({ tool }),
    setStyle: (patch) => set((s) => ({ style: { ...s.style, ...patch } })),
    setHighlightOpacity: (v) => set({ highlightOpacity: v }),

    commit: (tabKey, fn, select) =>
      update(tabKey, (d) => ({
        past: [...d.past.slice(-99), d.list],
        future: [],
        list: fn(d.list),
        dirty: true,
        selected: select === undefined ? d.selected : select,
      })),

    preview: (tabKey, fn) => update(tabKey, (d) => ({ list: fn(d.list) })),

    select: (tabKey, id) => update(tabKey, () => ({ selected: id })),
    setEditing: (editing) => set({ editing }),

    undo: (tabKey) =>
      update(tabKey, (d) => {
        const prev = d.past[d.past.length - 1];
        if (!prev) return {};
        return { list: prev, past: d.past.slice(0, -1), future: [d.list, ...d.future], dirty: true, selected: null };
      }),

    redo: (tabKey) =>
      update(tabKey, (d) => {
        const next = d.future[0];
        if (!next) return {};
        return { list: next, past: [...d.past, d.list], future: d.future.slice(1), dirty: true, selected: null };
      }),

    save: async (tabKey) => {
      const d = get().docs[tabKey];
      const tab = useApp.getState().tabs.find((t) => t.key === tabKey);
      if (!d || !tab || !d.dirty) return true;
      update(tabKey, () => ({ saving: true }));
      try {
        const res = await annotApi.save(tab.info.id, d.list);
        // Après rechargement, plus rien n'est masqué côté moteur.
        for (const k of [...hiddenSent.keys()]) if (k.startsWith(`${tabKey}:`)) hiddenSent.delete(k);
        set((s) => ({
          docs: {
            ...s.docs,
            [tabKey]: { ...d, list: res.annots, originals: res.annots, past: [], future: [], dirty: false, selected: null, rev: d.rev + 1, saving: false },
          },
        }));
        useApp.getState().updateTab(tabKey, { info: res.info });
        void fillQuotes(tabKey, tab.info.id);
        toast.success(i18n.t("annot.saved"));
        return true;
      } catch (e) {
        update(tabKey, () => ({ saving: false }));
        toast.error(asAppError(e).message);
        return false;
      }
    },

    discard: (tabKey) => {
      for (const k of [...hiddenSent.keys()]) if (k.startsWith(`${tabKey}:`)) hiddenSent.delete(k);
      set((s) => {
        const docs = { ...s.docs };
        delete docs[tabKey];
        return { docs };
      });
    },
  };
});

export function useDocAnnots(tabKey: string | undefined): DocAnnots | null {
  return useAnnots((s) => (tabKey ? (s.docs[tabKey] ?? null) : null));
}

/** Onglets dont les annotations ne sont pas enregistrées. */
export function dirtyTabs(): string[] {
  const docs = useAnnots.getState().docs;
  return Object.keys(docs).filter((k) => docs[k].dirty);
}
