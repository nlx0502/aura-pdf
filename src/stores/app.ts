/**
 * État global de l'application : écran affiché, onglets de documents,
 * préférences, documents récents.
 */
import { create } from "zustand";
import i18n from "../i18n";
import { api, asAppError, type DocInfo, type RecentEntry, type SearchHit, type UserMark } from "../lib/api";
import { dropDocument } from "../lib/render";
import { dropPageData } from "../lib/pagedata";
import type { ViewMode } from "../lib/layout";
import { toast } from "../components/Toast";

export type Screen = "home" | "reader" | "gallery";
export type ModuleId = "lire" | "annoter" | "editer" | "signer" | "securite" | "fusion" | "forms" | "compress" | "convert";
export type LeftPanel = "pages" | "toc" | "marks" | "files" | "layers";
export type AnimPref = "system" | "on" | "off";

export interface SearchState {
  query: string;
  token: number;
  hits: SearchHit[];
  scanned: number;
  done: boolean;
  /** Index du résultat sélectionné. */
  active: number;
}

export interface DocTab {
  key: string;
  info: DocInfo;
  /** Page courante (index 0). */
  page: number;
  zoom: number;
  fit: "width" | "page" | null;
  view: ViewMode;
  rot: number;
  dark: boolean;
  layers: { annotations: boolean; forms: boolean };
  search: SearchState;
  marks: UserMark[];
  /** Révision de l'affichage (visibilité des calques du document modifiée). */
  viewRev?: number;
  /** Demande de défilement vers une page (incrémentée à chaque saut). */
  jump: { page: number; seq: number; y?: number };
}

export interface Prefs {
  animations: AnimPref;
  userName: string;
  darkPaper: boolean;
}

interface PasswordPrompt {
  path: string;
  name: string;
  error?: string;
}

interface AppStore {
  screen: Screen;
  module: ModuleId;
  tabs: DocTab[];
  active: string | null;
  focus: boolean;
  leftPanel: LeftPanel;
  leftOpen: boolean;
  rightOpen: boolean;
  paletteOpen: boolean;
  settingsOpen: boolean;
  printOpen: boolean;
  create: { open: boolean; source: "blank" | "image" | "doc" | "scan" | "clip"; images: string[] };
  password: PasswordPrompt | null;
  recents: RecentEntry[];
  prefs: Prefs;
  systemName: string;

  init: () => Promise<void>;
  setScreen: (s: Screen) => void;
  setModule: (m: ModuleId) => void;
  goHome: () => void;
  openFiles: (paths: string[]) => Promise<void>;
  openPath: (path: string, password?: string) => Promise<boolean>;
  closeTab: (key: string) => void;
  activate: (key: string) => void;
  cycleTab: (dir: 1 | -1) => void;
  updateTab: (key: string, patch: Partial<DocTab> | ((t: DocTab) => Partial<DocTab>)) => void;
  jumpTo: (key: string, page: number, y?: number) => void;
  refreshRecents: () => Promise<void>;
  setPref: <K extends keyof Prefs>(k: K, v: Prefs[K]) => void;
  set: (patch: Partial<AppStore>) => void;
}

const PDF_RE = /\.pdf$/i;
const IMAGE_RE = /\.(png|jpe?g|gif|bmp|tiff?|webp)$/i;
const OFFICE_RE = /\.(docx?|xlsx?|pptx?|odt|ods|odp|rtf)$/i;

function baseName(p: string) {
  return p.split(/[\\/]/).pop() ?? p;
}

export function emptySearch(): SearchState {
  return { query: "", token: 0, hits: [], scanned: 0, done: true, active: -1 };
}

let tabSeq = 0;

export const useApp = create<AppStore>((set, get) => ({
  screen: "home",
  module: "lire",
  tabs: [],
  active: null,
  focus: false,
  leftPanel: "pages",
  leftOpen: true,
  rightOpen: true,
  paletteOpen: false,
  settingsOpen: false,
  printOpen: false,
  create: { open: false, source: "blank", images: [] },
  password: null,
  recents: [],
  prefs: { animations: "system", userName: "", darkPaper: false },
  systemName: "",

  init: async () => {
    try {
      const [prefs, user] = await Promise.all([api.prefsGet(), api.userInfo()]);
      set({
        prefs: { ...get().prefs, ...(prefs as Partial<Prefs>) },
        systemName: user.displayName,
      });
    } catch {
      /* préférences par défaut */
    }
    await get().refreshRecents();
    const files = await api.takeStartupFiles().catch(() => []);
    if (files.length) await get().openFiles(files);
  },

  setScreen: (screen) => set({ screen }),
  setModule: (module) => set({ module, screen: get().tabs.length ? "reader" : get().screen }),
  goHome: () => {
    set({ screen: "home", focus: false });
    void get().refreshRecents();
  },

  openFiles: async (paths) => {
    const images = paths.filter((p) => IMAGE_RE.test(p));
    for (const p of paths.filter((p) => PDF_RE.test(p))) await get().openPath(p);
    if (images.length) set({ create: { open: true, source: "image", images } });
    if (paths.some((p) => OFFICE_RE.test(p))) toast.info(i18n.t("errors.unsupported"), { duration: 7000 });
    const other = paths.filter((p) => !PDF_RE.test(p) && !IMAGE_RE.test(p) && !OFFICE_RE.test(p));
    // Fichier sans extension connue : tenter l'ouverture comme PDF.
    for (const p of other) await get().openPath(p);
  },

  openPath: async (path, password) => {
    // Deux demandes simultanées pour le même fichier : une seule ouverture.
    const k = path.toLowerCase();
    const running = opening.get(k);
    if (running) return running;
    const p = openPathInner(path, password).finally(() => opening.delete(k));
    opening.set(k, p);
    return p;
  },

  closeTab: (key) => {
    closeTabImpl(key);
  },

  activate: (key) => set({ active: key, screen: "reader" }),

  cycleTab: (dir) => {
    const { tabs, active } = get();
    if (tabs.length < 2) return;
    const i = tabs.findIndex((t) => t.key === active);
    const n = (i + dir + tabs.length) % tabs.length;
    set({ active: tabs[n].key, screen: "reader" });
  },

  updateTab: (key, patch) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.key === key ? { ...t, ...(typeof patch === "function" ? patch(t) : patch) } : t)),
    })),

  jumpTo: (key, page, y) =>
    get().updateTab(key, (t) => {
      const p = Math.max(0, Math.min(page, t.info.pageCount - 1));
      return { page: p, jump: { page: p, seq: t.jump.seq + 1, y } };
    }),

  refreshRecents: async () => {
    try {
      set({ recents: await api.recentsList() });
    } catch {
      /* ignoré */
    }
  },

  setPref: (k, v) => {
    set((s) => ({ prefs: { ...s.prefs, [k]: v } }));
    void api.prefsSet(k, v).catch(() => {});
  },

  set: (patch) => set(patch),
}));

const opening = new Map<string, Promise<boolean>>();

async function openPathInner(path: string, password?: string): Promise<boolean> {
  const get = useApp.getState;
  const set = useApp.setState;
  {
    const existing = get().tabs.find((t) => t.info.path.toLowerCase() === path.toLowerCase());
    if (existing) {
      set({ active: existing.key, screen: "reader", module: "lire" });
      return true;
    }
    const name = baseName(path);
    let info: DocInfo;
    try {
      info = await api.openDocument(path, password);
    } catch (e) {
      const err = asAppError(e);
      if (err.code === "password_required" || err.code === "wrong_password") {
        set({ password: { path, name, error: err.code === "wrong_password" ? err.message : undefined } });
        return false;
      }
      toast.error(`${i18n.t("errors.openTitle", { name })} — ${err.message}`);
      return false;
    }
    set({ password: null });

    let lastPage = 0;
    let marks: UserMark[] = [];
    try {
      lastPage = await api.recentsTouch({
        path: info.path,
        pageCount: info.pageCount,
        fileSize: info.fileSize,
        signed: info.signatureCount > 0,
        form: info.hasForm,
        scanned: info.scanned,
      });
      const recents = await api.recentsList();
      marks = recents.find((r) => r.path.toLowerCase() === info.path.toLowerCase())?.marks ?? [];
      set({ recents });
    } catch {
      /* les récents sont facultatifs */
    }

    const key = `t${++tabSeq}`;
    const tab: DocTab = {
      key,
      info,
      page: Math.min(lastPage, info.pageCount - 1),
      zoom: 100,
      fit: "width",
      view: "continu",
      rot: 0,
      dark: get().prefs.darkPaper,
      layers: { annotations: true, forms: true },
      search: emptySearch(),
      marks,
      jump: { page: Math.min(lastPage, info.pageCount - 1), seq: 1 },
    };
    set((s) => ({ tabs: [...s.tabs, tab], active: key, screen: "reader", module: "lire" }));

    if (lastPage > 0) {
      toast.show({
        kind: "resume",
        message: i18n.t("reader.resumed"),
        actionLabel: i18n.t("reader.backToStart"),
        onAction: () => get().jumpTo(key, 0),
        position: "right",
        duration: 7000,
      });
    }
    return true;
  }
}

function closeTabImpl(key: string) {
  const get = useApp.getState;
  const set = useApp.setState;
  const tab = get().tabs.find((t) => t.key === key);
  if (!tab) return;
  void api.recentsSetPage(tab.info.path, tab.page).catch(() => {});
  void api.closeDocument(tab.info.id).catch(() => {});
  dropDocument(tab.info.id);
  dropPageData(tab.info.id);
  set((s) => {
    const idx = s.tabs.findIndex((t) => t.key === key);
    const tabs = s.tabs.filter((t) => t.key !== key);
    const active = s.active === key ? (tabs[Math.min(idx, tabs.length - 1)]?.key ?? null) : s.active;
    return { tabs, active, screen: tabs.length ? s.screen : "home", focus: tabs.length ? s.focus : false };
  });
  if (!get().tabs.length) void get().refreshRecents();
}

/** Onglet actif (ou null). */
export function useActiveTab(): DocTab | null {
  return useApp((s) => s.tabs.find((t) => t.key === s.active) ?? null);
}
