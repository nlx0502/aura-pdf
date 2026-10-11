import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useTranslation } from "react-i18next";
import { ToastHost, toast } from "./components";
import { Home } from "./screens/home/Home";
import { CreateDialog } from "./screens/home/CreateDialog";
import { Reader } from "./screens/reader/Reader";
import { initSearchEvents } from "./screens/reader/search";
import { Gallery } from "./screens/gallery/Gallery";
import { AppBar } from "./shell/AppBar";
import { CommandPalette, toggleFullscreen } from "./shell/CommandPalette";
import { PasswordDialog, SettingsDialog } from "./shell/Dialogs";
import { PrintDialog } from "./screens/print/PrintDialog";
import { ModulePlaceholder } from "./shell/ModulePlaceholder";
import { Annotate } from "./screens/annotate/Annotate";
import { requestCloseTab, saveTab, UnsavedDialog, useQuitGuard } from "./shell/Unsaved";
import { Edit } from "./screens/edit/Edit";
import { useEdit } from "./stores/edit";
import { useAnnots } from "./stores/annot";
import { useApp } from "./stores/app";
import { useDrag } from "./stores/drag";
import { pickFilesToOpen } from "./lib/files";
import { isMod, isTyping } from "./lib/platform";
import { clampZoom, nextZoom } from "./lib/layout";

/** Raccourcis clavier globaux (voir README). */
function useShortcuts() {
  const { t } = useTranslation();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const st = useApp.getState();
      const tab = st.tabs.find((x) => x.key === st.active);
      const dialogOpen = !!document.querySelector(".scrim");
      const k = e.key.toLowerCase();

      if (isMod(e)) {
        // Raccourcis du navigateur à neutraliser dans une application de bureau.
        if (["r", "u", "j", "g", "h", "l", "d"].includes(k) && !e.shiftKey) {
          e.preventDefault();
          return;
        }
        if (k === "o") {
          e.preventDefault();
          void pickFilesToOpen().then(st.openFiles);
        } else if (k === "n") {
          e.preventDefault();
          st.set({ create: { open: true, source: "blank", images: [] } });
        } else if (k === "p") {
          e.preventDefault();
          st.set({ printOpen: true });
        } else if (k === "k") {
          e.preventDefault();
          st.set({ paletteOpen: !st.paletteOpen });
        } else if (k === "s") {
          e.preventDefault();
          if (tab) {
            if (useEdit.getState().docs[tab.key]?.dirty || useAnnots.getState().docs[tab.key]?.dirty) void saveTab(tab.key);
            else toast.info(t("reader.nothingToSave"));
          }
        } else if (k === "w") {
          e.preventDefault();
          if (tab && st.screen === "reader") requestCloseTab(tab.key);
        } else if (k === "tab") {
          e.preventDefault();
          st.cycleTab(e.shiftKey ? -1 : 1);
        } else if (tab && st.screen === "reader" && (k === "=" || k === "+" || e.code === "NumpadAdd")) {
          e.preventDefault();
          st.updateTab(tab.key, { zoom: nextZoom(tab.zoom, 1), fit: null });
        } else if (tab && st.screen === "reader" && (k === "-" || e.code === "NumpadSubtract")) {
          e.preventDefault();
          st.updateTab(tab.key, { zoom: nextZoom(tab.zoom, -1), fit: null });
        } else if (tab && st.screen === "reader" && (k === "0" || e.code === "Numpad0")) {
          e.preventDefault();
          st.updateTab(tab.key, { fit: "width", zoom: clampZoom(tab.zoom) });
        }
        return;
      }
      if (e.key === "F11") {
        e.preventDefault();
        void toggleFullscreen();
        return;
      }
      if (e.key === "F5" || e.key === "F7" || e.key === "F12") {
        e.preventDefault();
        return;
      }
      if (isTyping(e) || dialogOpen) return;
      if (k === "f" && !e.altKey && st.screen === "reader" && tab) {
        e.preventDefault();
        st.set({ focus: !st.focus });
      } else if (e.key === "Escape" && st.focus) {
        st.set({ focus: false });
      }
    };
    window.addEventListener("keydown", onKey);
    // Pas de menu contextuel du navigateur, sauf dans les champs de saisie.
    const onContext = (e: MouseEvent) => {
      const el = e.target as HTMLElement;
      if (el.closest("input, textarea, .textlayer")) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", onContext);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("contextmenu", onContext);
    };
  }, [t]);
}

/** Fichiers glissés depuis l'Explorateur et fichiers transmis par une seconde instance. */
function useFileEvents() {
  useEffect(() => {
    const unlisteners: (() => void)[] = [];
    let disposed = false;
    // L'abonnement est asynchrone : s'il arrive après le démontage, on le retire aussitôt.
    const keep = (u: () => void) => (disposed ? u() : unlisteners.push(u));
    void listen<string[]>("open-files", (e) => void useApp.getState().openFiles(e.payload)).then(keep);
    void getCurrentWebview()
      .onDragDropEvent((e) => {
        const p = e.payload;
        const dpr = window.devicePixelRatio || 1;
        if (p.type === "enter" || p.type === "over") {
          useDrag.setState({ dragging: true, x: p.position.x / dpr, y: p.position.y / dpr });
        } else if (p.type === "drop") {
          useDrag.setState({ dragging: false });
          if (p.paths.length) void useApp.getState().openFiles(p.paths);
        } else {
          useDrag.setState({ dragging: false });
        }
      })
      .then(keep)
      .catch(() => {});
    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  }, []);
}

export default function App() {
  const screen = useApp((s) => s.screen);
  const module = useApp((s) => s.module);
  const hasTabs = useApp((s) => s.tabs.length > 0);
  const animations = useApp((s) => s.prefs.animations);

  useEffect(() => {
    initSearchEvents();
    void useApp.getState().init();
  }, []);
  useShortcuts();
  useFileEvents();
  useQuitGuard();

  useEffect(() => {
    const html = document.documentElement;
    if (animations === "off") html.dataset.motion = "reduce";
    else if (animations === "on") html.dataset.motion = "full";
    else delete html.dataset.motion;
  }, [animations]);

  let body: React.ReactNode;
  if (screen === "gallery") body = <Gallery />;
  else if (screen === "reader" && module === "annoter" && hasTabs)
    body = (
      <>
        <AppBar />
        <Annotate />
      </>
    );
  else if (screen === "reader" && module === "editer" && hasTabs)
    body = (
      <>
        <AppBar />
        <Edit />
      </>
    );
  else if (screen === "reader" && module !== "lire" && module !== "annoter" && module !== "editer")
    body = (
      <>
        <AppBar />
        <ModulePlaceholder module={module} />
      </>
    );
  else if (screen === "reader" && hasTabs)
    body = (
      <>
        <AppBar />
        <Reader />
      </>
    );
  else body = <Home />;

  return (
    <div className="app">
      {body}
      <CreateDialog />
      <PasswordDialog />
      <SettingsDialog />
      <PrintDialog />
      <CommandPalette />
      <UnsavedDialog />
      <ToastHost only="center" />
    </div>
  );
}
