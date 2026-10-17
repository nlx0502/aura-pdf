/**
 * État du module Signer : signature préparée (dessin, nom tapé, image),
 * certificat, emplacement choisi sur la page, rapport des signatures du
 * document, déroulement de la signature.
 */
import { create } from "zustand";
import i18n from "../i18n";
import { api, asAppError, type Rect } from "../lib/api";
import { DEFAULT_TSA, signApi, type CertInfo, type SigFont, type SignReport, type Visual } from "../lib/sign";
import { newJob } from "../lib/tools";
import { dropDocument } from "../lib/render";
import { dropPageData } from "../lib/pagedata";
import { toast } from "../components/Toast";
import { useApp } from "./app";
import { useAnnots } from "./annot";
import { useEdit } from "./edit";
import { useForms } from "./forms";

export type SigMode = "draw" | "type" | "import";

export type Certify = "0" | "1" | "2" | "3";

export interface Placement {
  page: number;
  rect: Rect;
  /** Champ de signature existant. */
  field: string | null;
}

interface SignStore {
  mode: SigMode;
  strokes: [number, number][][];
  typed: string;
  font: SigFont;
  image: string | null;
  ink: string;
  certOn: boolean;
  certPath: string | null;
  certPassword: string;
  cert: CertInfo | null;
  certError: string | null;
  tsa: boolean;
  tsaUrl: string;
  /** Certifier le document (DocMDP) : 0 non, sinon modifications permises 1 à 3. */
  certify: Certify;
  reason: string;
  placement: Placement | null;
  reports: Record<string, SignReport | null>;
  verifying: boolean;
  /** -1 : au repos ; sinon étape en cours (0 à 3) ; 4 : terminé. */
  step: number;
  job: number | null;
  /** Dernière signature posée (effet « encre ») et sa mention. */
  justSigned: Placement | null;
  lastCaption: string[];
  set: (p: Partial<SignStore>) => void;
  init: () => Promise<void>;
  checkCert: () => Promise<void>;
  loadReport: (tabKey: string, path: string, slow?: boolean) => Promise<void>;
  visual: () => Visual | null;
  sign: (tabKey: string) => Promise<void>;
  cancel: () => void;
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const useSign = create<SignStore>((set, get) => ({
  mode: "draw",
  strokes: [],
  typed: "",
  font: "caveat",
  image: null,
  ink: "#1F2A6B",
  certOn: false,
  certPath: null,
  certPassword: "",
  cert: null,
  certError: null,
  tsa: false,
  tsaUrl: DEFAULT_TSA,
  certify: "0",
  reason: "",
  placement: null,
  reports: {},
  verifying: false,
  step: -1,
  job: null,
  justSigned: null,
  lastCaption: [],
  set: (p) => set(p),

  init: async () => {
    const st = useApp.getState();
    if (!get().typed) set({ typed: st.prefs.userName || st.systemName || "" });
    try {
      const prefs = await api.prefsGet();
      const path = prefs.signCert as string | undefined;
      if (path && !get().certPath && (await api.fileExists(path))) set({ certPath: path, certOn: true });
      if (typeof prefs.signTsa === "string") set({ tsaUrl: prefs.signTsa as string });
    } catch {
      /* préférences facultatives */
    }
  },

  checkCert: async () => {
    const { certPath, certPassword } = get();
    if (!certPath) return;
    try {
      const cert = await signApi.certInspect(certPath, certPassword);
      set({ cert, certError: cert.expired ? i18n.t("sign.certExpired") : null });
      void api.prefsSet("signCert", certPath).catch(() => {});
    } catch (e) {
      set({ cert: null, certError: asAppError(e).message });
    }
  },

  loadReport: async (tabKey, path, slow) => {
    if (slow) set({ verifying: true });
    const t0 = performance.now();
    try {
      const r = await signApi.report(path);
      if (slow) await wait(Math.max(0, 900 - (performance.now() - t0)));
      set((s) => ({ reports: { ...s.reports, [tabKey]: r }, verifying: false }));
    } catch (e) {
      set((s) => ({ reports: { ...s.reports, [tabKey]: { signatures: [], empty: [] } }, verifying: false }));
      toast.error(asAppError(e).message);
    }
  },

  visual: () => {
    const s = get();
    if (s.mode === "draw") return s.strokes.some((x) => x.length) ? { kind: "draw", strokes: s.strokes } : null;
    if (s.mode === "type") return s.typed.trim() ? { kind: "type", text: s.typed.trim(), font: s.font } : null;
    return s.image ? { kind: "image", path: s.image } : null;
  },

  sign: async (tabKey) => {
    const s = get();
    const visual = s.visual();
    const tab = useApp.getState().tabs.find((t) => t.key === tabKey);
    if (!tab || !visual || !s.placement || s.step >= 0) return;
    if (s.certOn && (!s.certPath || !s.cert)) {
      toast.error(i18n.t("sign.needCert"));
      return;
    }
    // Les modifications en attente sont d'abord enregistrées : la signature porte sur le fichier.
    if (useForms.getState().docs[tabKey]?.dirty && !(await useForms.getState().save(tabKey))) return;
    if (useEdit.getState().docs[tabKey]?.dirty && !(await useEdit.getState().save(tabKey))) return;
    if (useAnnots.getState().docs[tabKey]?.dirty && !(await useAnnots.getState().save(tabKey))) return;
    const info = useApp.getState().tabs.find((t) => t.key === tabKey)!.info;
    const name = s.certOn && s.cert ? s.cert.name : s.typed.trim() || useApp.getState().prefs.userName || useApp.getState().systemName;
    const when = new Date().toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" });
    const placement = s.placement;
    const caption = s.certOn ? [i18n.t("sign.captionBy", { name }), i18n.t("sign.captionDate", { date: when })] : [];
    const job = newJob({
      progress: (p) => set({ step: p.step }),
      done: async (d) => {
        if (!d.ok) {
          set({ step: -1, job: null });
          if (d.cancelled) toast.info(i18n.t("sign.cancelled"));
          else toast.error(d.message ?? i18n.t("sign.failed"));
          return;
        }
        try {
          const fresh = await signApi.reload(info.id);
          dropDocument(info.id);
          dropPageData(info.id);
          useAnnots.getState().discard(tabKey);
          useEdit.getState().discard(tabKey);
          useApp.getState().updateTab(tabKey, (t) => ({ info: fresh, viewRev: (t.viewRev ?? 0) + 1 }));
        } catch (e) {
          toast.error(asAppError(e).message);
        }
        set({ step: 4, job: null, placement: null, justSigned: placement, lastCaption: caption });
        window.setTimeout(() => set({ justSigned: null }), 1600);
        toast.success(i18n.t(s.certOn ? "sign.doneDigital" : "sign.doneVisual"));
        await get().loadReport(tabKey, info.path);
        window.setTimeout(() => get().step === 4 && set({ step: -1 }), 2500);
      },
    });
    set({ step: 0, job });
    try {
      await signApi.start(job, info.path, {
        page: placement.page,
        rect: placement.rect,
        visual: {
          visual,
          color: s.ink,
          caption,
        },
        field: placement.field,
        cert: s.certOn && s.certPath ? { path: s.certPath, password: s.certPassword, tsa: s.tsa ? s.tsaUrl : null, reason: s.reason, location: "", pades: true, certify: s.certify === "0" ? null : Number(s.certify) } : null,
        author: name,
      });
    } catch (e) {
      set({ step: -1, job: null });
      toast.error(asAppError(e).message);
    }
  },

  cancel: () => {
    const j = get().job;
    if (j !== null) void import("../lib/tools").then((m) => m.toolsApi.cancel(j));
  },
}));
