/**
 * État du module Formulaires, par onglet : champs (valeurs, propriétés,
 * apparence, position, libellés en cours d'édition), champs ajoutés ou
 * supprimés, sélection, placement d'un nouveau champ, enregistrement et
 * aplatissement.
 */
import { create } from "zustand";
import i18n from "../i18n";
import { asAppError, type DocInfo, type Rect } from "../lib/api";
import {
  captionPos,
  changed,
  defaultSize,
  defaultStyle,
  formsApi,
  layoutRadio,
  placeCaption,
  stateName,
  type CaptionPos,
  type FieldKind,
  type FieldStyle,
  type FormField,
  type FormPlan,
} from "../lib/forms";
import { dropDocument } from "../lib/render";
import { dropPageData } from "../lib/pagedata";
import { toast } from "../components/Toast";
import { useApp } from "./app";
import { useAnnots } from "./annot";
import { useEdit } from "./edit";

export interface DocForms {
  docId: number;
  /** Révision d'affichage de l'onglet au chargement : le fichier a pu être réécrit depuis. */
  rev: number;
  loaded: boolean;
  /** Champs tels qu'enregistrés dans le fichier. */
  orig: FormField[];
  /** Champs affichés (en cours d'édition). */
  fields: FormField[];
  deleted: string[];
  selected: string | null;
  /** Champ tout juste ajouté (halo de la maquette). */
  fresh: string | null;
  placing: FieldKind | null;
  dirty: boolean;
  saving: boolean;
  /** Aplatissement : copie de l'original pour « Annuler ». */
  flatBackup: string | null;
  flattening: boolean;
}

type FieldPatch = Partial<Pick<FormField, "label" | "required" | "readonly" | "multiline" | "maxLen" | "placeholder" | "value">>;

interface FormsStore {
  docs: Record<string, DocForms>;
  ensure: (tabKey: string, info: DocInfo, rev: number) => Promise<void>;
  reload: (tabKey: string) => Promise<void>;
  setValue: (tabKey: string, name: string, value: string) => void;
  setProps: (tabKey: string, name: string, p: FieldPatch) => void;
  setStyle: (tabKey: string, name: string, p: Partial<FieldStyle>) => void;
  /** Libellé visible : position, ou `null` pour le masquer. */
  setCaption: (tabKey: string, name: string, pos: CaptionPos | null) => void;
  setOptions: (tabKey: string, name: string, labels: string[]) => void;
  /** Déplace tout le champ (widgets et libellés). */
  move: (tabKey: string, name: string, dx: number, dy: number) => void;
  /** Redimensionne le premier widget (libellé recalé). */
  resize: (tabKey: string, name: string, rect: Rect) => void;
  select: (tabKey: string, name: string | null) => void;
  startPlacing: (tabKey: string, kind: FieldKind | null) => void;
  place: (tabKey: string, page: number, rect: Rect) => void;
  remove: (tabKey: string, name: string) => void;
  save: (tabKey: string) => Promise<boolean>;
  discard: (tabKey: string) => void;
  flatten: (tabKey: string) => Promise<void>;
  unflatten: (tabKey: string) => Promise<void>;
}

function isDirty(d: Pick<DocForms, "orig" | "fields" | "deleted">) {
  if (d.deleted.length || d.fields.some((f) => f.isNew)) return true;
  return d.fields.some((f) => {
    const o = d.orig.find((x) => x.name === f.name);
    return !o || changed(o, f);
  });
}

function uniqueLabel(fields: FormField[], base: string) {
  let label = base;
  let n = 2;
  while (fields.some((f) => f.label === label)) label = `${base} ${n++}`;
  return label;
}

/** Texte du libellé visible : libellé, suivi de « * » si le champ est obligatoire. */
const captionText = (f: Pick<FormField, "label" | "required">) => f.label + (f.required ? " *" : "");

function groupBox(f: FormField): Rect {
  const rs = f.widgets.map((w) => w.rect);
  const x = Math.min(...rs.map((r) => r.x));
  const y = Math.min(...rs.map((r) => r.y));
  return { x, y, w: Math.max(...rs.map((r) => r.x + r.w)) - x, h: Math.max(...rs.map((r) => r.y + r.h)) - y };
}

/** Libellé recalé après un changement (texte, obligatoire, taille du champ). */
function refreshCaption(f: FormField): FormField {
  if (!f.caption || !f.widgets[0]) return f;
  const w = f.widgets[0];
  // Groupe radio : le libellé du groupe se place au-dessus de l'ensemble des boutons.
  const box = f.kind === "radio" ? groupBox(f) : w.rect;
  const pos = f.kind === "radio" ? "above" : captionPos(f.caption, w.rect);
  return { ...f, caption: placeCaption(captionText(f), w.page, box, pos, f.caption.size, f.caption.bold, f.caption.color) };
}

/** Applique un nouveau DocInfo (fichier réécrit) à l'onglet. */
function refreshTab(tabKey: string, fresh: DocInfo) {
  dropDocument(fresh.id);
  dropPageData(fresh.id);
  useApp.getState().updateTab(tabKey, (t) => ({ info: fresh, viewRev: (t.viewRev ?? 0) + 1 }));
}

let seq = 0;

export const useForms = create<FormsStore>((set, get) => {
  const patch = (tabKey: string, f: (d: DocForms) => Partial<DocForms>) =>
    set((s) => {
      const d = s.docs[tabKey];
      if (!d) return s;
      const next = { ...d, ...f(d) };
      next.dirty = isDirty(next);
      return { docs: { ...s.docs, [tabKey]: next } };
    });
  const update = (tabKey: string, name: string, fn: (f: FormField) => FormField) => patch(tabKey, (d) => ({ fields: d.fields.map((f) => (f.name === name ? fn(f) : f)) }));

  return {
    docs: {},

    ensure: async (tabKey, info, rev) => {
      const d = get().docs[tabKey];
      if (d && d.docId === info.id && (d.rev === rev || d.dirty || d.saving || d.flattening || !d.loaded)) return;
      set((s) => ({
        docs: {
          ...s.docs,
          [tabKey]: { docId: info.id, rev, loaded: false, orig: [], fields: [], deleted: [], selected: null, fresh: null, placing: null, dirty: false, saving: false, flatBackup: d?.flatBackup ?? null, flattening: false },
        },
      }));
      await get().reload(tabKey);
    },

    reload: async (tabKey) => {
      const tab = useApp.getState().tabs.find((t) => t.key === tabKey);
      if (!tab) return;
      try {
        const fields = await formsApi.list(tab.info.id);
        set((s) => {
          const d = s.docs[tabKey];
          if (!d) return s;
          return {
            docs: {
              ...s.docs,
              [tabKey]: { ...d, docId: tab.info.id, rev: tab.viewRev ?? 0, loaded: true, orig: fields, fields, deleted: [], dirty: false, selected: d.selected && fields.some((f) => f.name === d.selected) ? d.selected : (fields[0]?.name ?? null) },
            },
          };
        });
      } catch (e) {
        toast.error(asAppError(e).message);
        set((s) => (s.docs[tabKey] ? { docs: { ...s.docs, [tabKey]: { ...s.docs[tabKey], loaded: true } } } : s));
      }
    },

    setValue: (tabKey, name, value) => update(tabKey, name, (f) => ({ ...f, value })),

    setProps: (tabKey, name, p) =>
      update(tabKey, name, (f) => {
        const next = { ...f, ...p };
        return p.label !== undefined || p.required !== undefined ? refreshCaption(next) : next;
      }),

    setStyle: (tabKey, name, p) => update(tabKey, name, (f) => ({ ...f, style: { ...f.style, ...p } })),

    setCaption: (tabKey, name, pos) =>
      update(tabKey, name, (f) => {
        if (!pos || !f.widgets[0]) return { ...f, caption: null };
        const w = f.widgets[0];
        const box = f.kind === "radio" ? groupBox(f) : w.rect;
        return { ...f, caption: placeCaption(captionText(f), w.page, box, pos, f.caption?.size ?? 10, f.caption?.bold ?? true, f.caption?.color) };
      }),

    setOptions: (tabKey, name, labels) =>
      update(tabKey, name, (f) => {
        const clean = labels.map((l) => l.trim()).filter(Boolean);
        if (f.kind === "radio") {
          const options = clean.map((label, i) => ({ value: stateName(label, i), label }));
          const first = f.widgets[0];
          if (!first || !options.length) return f;
          const widgets = layoutRadio(options, first.page, first.rect, first.rect.w, first.caption?.size ?? 10);
          const value = options.some((o) => o.value === f.value) ? f.value : "";
          return refreshCaption({ ...f, options, widgets, value });
        }
        const options = clean.map((v) => ({ value: v, label: v }));
        return { ...f, options, value: options.some((o) => o.value === f.value) ? f.value : "" };
      }),

    move: (tabKey, name, dx, dy) =>
      update(tabKey, name, (f) => {
        const sh = <T extends { rect: Rect }>(o: T): T => ({ ...o, rect: { ...o.rect, x: o.rect.x + dx, y: o.rect.y + dy } });
        return { ...f, widgets: f.widgets.map((w) => ({ ...sh(w), caption: w.caption ? sh(w.caption) : null })), caption: f.caption ? sh(f.caption) : null };
      }),

    resize: (tabKey, name, rect) =>
      update(tabKey, name, (f) => {
        if (!f.widgets[0]) return f;
        const widgets = [{ ...f.widgets[0], rect }, ...f.widgets.slice(1)];
        return refreshCaption({ ...f, widgets });
      }),

    select: (tabKey, name) => patch(tabKey, () => ({ selected: name })),

    startPlacing: (tabKey, kind) => patch(tabKey, () => ({ placing: kind })),

    place: (tabKey, page, rect) => {
      const d = get().docs[tabKey];
      if (!d?.placing) return;
      const kind = d.placing;
      const name = `aura-new-${++seq}`;
      const base = kind === "sig" ? i18n.t("forms.newSig") : i18n.t("forms.newField", { kind: i18n.t(`forms.kind.${kind}`).toLowerCase() });
      const label = uniqueLabel(d.fields, base);
      let field: FormField = {
        name,
        isNew: true,
        label,
        kind,
        value: "",
        options: [],
        required: false,
        readonly: false,
        multiline: false,
        maxLen: null,
        placeholder: "",
        style: defaultStyle(kind),
        widgets: [{ page, rect, on: kind === "check" ? "Oui" : null, caption: null }],
        caption: null,
      };
      if (kind === "select") field.options = ["Option 1", "Option 2"].map((v) => ({ value: v, label: v }));
      if (kind === "radio") {
        const labels = [i18n.t("forms.optionN", { n: 1 }), i18n.t("forms.optionN", { n: 2 })];
        field.options = labels.map((l, i) => ({ value: stateName(l, i), label: l }));
        field.widgets = layoutRadio(field.options, page, { x: rect.x, y: rect.y }, defaultSize("radio").h);
      }
      field = { ...field, caption: placeCaption(captionText(field), page, kind === "radio" ? groupBox(field) : field.widgets[0].rect, kind === "check" ? "right" : "above") };
      patch(tabKey, (x) => ({ fields: [...x.fields, field], selected: name, fresh: name, placing: null }));
      window.setTimeout(() => patch(tabKey, (x) => (x.fresh === name ? { fresh: null } : {})), 1200);
    },

    remove: (tabKey, name) =>
      patch(tabKey, (d) => {
        const f = d.fields.find((x) => x.name === name);
        return {
          fields: d.fields.filter((x) => x.name !== name),
          deleted: !f || f.isNew ? d.deleted : [...d.deleted, name],
          selected: d.selected === name ? null : d.selected,
        };
      }),

    save: async (tabKey) => {
      const d = get().docs[tabKey];
      const tab = useApp.getState().tabs.find((t) => t.key === tabKey);
      if (!d || !tab) return false;
      if (!d.dirty) return true;
      // Les autres modifications en attente passent d'abord : le formulaire s'enregistre dans le fichier.
      if (useEdit.getState().docs[tabKey]?.dirty && !(await useEdit.getState().save(tabKey))) return false;
      if (useAnnots.getState().docs[tabKey]?.dirty && !(await useAnnots.getState().save(tabKey))) return false;
      const plan: FormPlan = {
        upsert: d.fields.filter((f) => {
          if (f.isNew) return true;
          const o = d.orig.find((x) => x.name === f.name);
          return !o || changed(o, f);
        }),
        delete: d.deleted,
      };
      patch(tabKey, () => ({ saving: true }));
      try {
        const cur = useApp.getState().tabs.find((t) => t.key === tabKey)!.info;
        const fresh = await formsApi.save(cur.id, plan);
        refreshTab(tabKey, fresh);
        // Les noms des nouveaux champs sont choisis par le moteur : la sélection suit le libellé.
        const selLabel = d.fields.find((f) => f.name === d.selected)?.label;
        await get().reload(tabKey);
        if (selLabel) patch(tabKey, (x) => ({ selected: x.fields.find((f) => f.label === selLabel)?.name ?? x.selected }));
        patch(tabKey, () => ({ saving: false }));
        toast.success(i18n.t("forms.saved"));
        return true;
      } catch (e) {
        patch(tabKey, () => ({ saving: false }));
        toast.error(asAppError(e).message);
        return false;
      }
    },

    discard: (tabKey) =>
      set((s) => {
        const d = s.docs[tabKey];
        if (!d) return s;
        return { docs: { ...s.docs, [tabKey]: { ...d, fields: d.orig, deleted: [], dirty: false, placing: null, selected: d.selected && d.orig.some((f) => f.name === d.selected) ? d.selected : null } } };
      }),

    flatten: async (tabKey) => {
      if (!(await get().save(tabKey))) return;
      const tab = useApp.getState().tabs.find((t) => t.key === tabKey);
      if (!tab) return;
      patch(tabKey, () => ({ flattening: true }));
      // Les champs se figent l'un après l'autre (maquette), puis le fichier est réécrit.
      const t0 = performance.now();
      try {
        const [fresh, backup] = await formsApi.flatten(tab.info.id);
        const n = get().docs[tabKey]?.fields.length ?? 0;
        await new Promise((r) => setTimeout(r, Math.max(0, Math.min(1400, 300 + n * 70) - (performance.now() - t0))));
        refreshTab(tabKey, fresh);
        patch(tabKey, () => ({ flatBackup: backup, flattening: false }));
        await get().reload(tabKey);
        toast.success(i18n.t("forms.flattened"));
      } catch (e) {
        patch(tabKey, () => ({ flattening: false }));
        toast.error(asAppError(e).message);
      }
    },

    unflatten: async (tabKey) => {
      const d = get().docs[tabKey];
      const tab = useApp.getState().tabs.find((t) => t.key === tabKey);
      if (!d?.flatBackup || !tab) return;
      try {
        const fresh = await formsApi.restore(tab.info.id, d.flatBackup);
        refreshTab(tabKey, fresh);
        patch(tabKey, () => ({ flatBackup: null }));
        await get().reload(tabKey);
      } catch (e) {
        toast.error(asAppError(e).message);
      }
    },
  };
});

export const useDocForms = (tabKey: string | undefined) => useForms((s) => (tabKey ? s.docs[tabKey] : undefined));

export const dirtyFormTabs = () =>
  Object.entries(useForms.getState().docs)
    .filter(([, d]) => d.dirty)
    .map(([k]) => k);
