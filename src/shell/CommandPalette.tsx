import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Icons, Modal, type IconName } from "../components";
import { useApp } from "../stores/app";
import { pickFilesToOpen } from "../lib/files";
import { requestCloseTab } from "./Unsaved";

interface Cmd {
  id: string;
  label: string;
  group: string;
  icon: IconName;
  run: () => void;
}

/** Palette de commandes (Ctrl+K) : recherche floue, flèches, Entrée. */
export function CommandPalette() {
  const { t } = useTranslation();
  const open = useApp((s) => s.paletteOpen);
  const set = useApp((s) => s.set);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      setQ("");
      setSel(0);
    }
  }, [open]);

  const cmds = useMemo<Cmd[]>(() => {
    if (!open) return [];
    const st = useApp.getState();
    const tab = st.tabs.find((x) => x.key === st.active);
    const g = (k: string) => t(`palette.groups.${k}`);
    const list: Cmd[] = [
      { id: "open", label: t("palette.cmds.open"), group: g("file"), icon: "file", run: () => void pickFilesToOpen().then(st.openFiles) },
      { id: "create", label: t("palette.cmds.create"), group: g("file"), icon: "plus", run: () => st.set({ create: { open: true, source: "blank", images: [] } }) },
      { id: "home", label: t("palette.cmds.home"), group: g("file"), icon: "home", run: st.goHome },
      { id: "print", label: t("palette.cmds.print"), group: g("file"), icon: "print", run: () => st.set({ printOpen: true }) },
    ];
    if (tab) {
      const up = (patch: Parameters<typeof st.updateTab>[1]) => st.updateTab(tab.key, patch);
      list.push(
        { id: "reading", label: t("palette.cmds.reading"), group: g("view"), icon: "focus", run: () => st.set({ focus: !st.focus }) },
        { id: "fullscreen", label: t("palette.cmds.fullscreen"), group: g("view"), icon: "maximize", run: () => void toggleFullscreen() },
        { id: "dark", label: t("palette.cmds.dark"), group: g("view"), icon: "moon", run: () => up({ dark: !tab.dark }) },
        { id: "continu", label: t("palette.cmds.continu"), group: g("view"), icon: "list", run: () => up({ view: "continu" }) },
        { id: "page", label: t("palette.cmds.page"), group: g("view"), icon: "file", run: () => up({ view: "page" }) },
        { id: "double", label: t("palette.cmds.double"), group: g("view"), icon: "read", run: () => up({ view: "double" }) },
        { id: "rotate", label: t("palette.cmds.rotate"), group: g("view"), icon: "rotate", run: () => up({ rot: tab.rot + 1 }) },
        { id: "fit", label: t("palette.cmds.fit"), group: g("view"), icon: "maximize", run: () => up({ fit: "width" }) },
        { id: "first", label: t("palette.cmds.first"), group: g("nav"), icon: "chevronUp", run: () => st.jumpTo(tab.key, 0) },
        { id: "last", label: t("palette.cmds.last"), group: g("nav"), icon: "chevronDown", run: () => st.jumpTo(tab.key, tab.info.pageCount - 1) },
        { id: "close", label: t("palette.cmds.closeTab"), group: g("file"), icon: "close", run: () => requestCloseTab(tab.key) },
      );
      // « 7 » ou « page 7 » : aller à la page.
      const m = q.trim().match(/^(?:p(?:age)?\.?\s*)?(\d{1,6})$/i);
      if (m) {
        const n = Math.min(tab.info.pageCount, Math.max(1, parseInt(m[1], 10)));
        list.unshift({ id: "goto", label: t("palette.cmds.goto", { n }), group: g("nav"), icon: "arrowRight", run: () => st.jumpTo(tab.key, n - 1) });
      }
    }
    list.push(
      { id: "settings", label: t("palette.cmds.settings"), group: g("tools"), icon: "settings", run: () => st.set({ settingsOpen: true }) },
      { id: "gallery", label: t("palette.cmds.gallery"), group: g("tools"), icon: "grid", run: () => st.set({ screen: "gallery" }) },
    );
    for (const r of st.recents.filter((r) => r.exists).slice(0, 8)) {
      list.push({ id: `r:${r.path}`, label: r.name, group: g("recent"), icon: "clock", run: () => void st.openPath(r.path) });
    }
    return list;
  }, [open, q, t]);

  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    if (!s) return cmds;
    return cmds.filter((c) => c.id === "goto" || c.label.toLowerCase().includes(s) || c.group.toLowerCase().includes(s));
  }, [cmds, q]);

  useEffect(() => setSel(0), [q]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-idx="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const close = () => set({ paletteOpen: false });
  const run = (c: Cmd) => {
    close();
    window.setTimeout(c.run, 0);
  };

  return (
    <Modal open={open} onClose={close} label={t("palette.label")} top width={560}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "16px 18px", borderBottom: "1px solid var(--border)" }}>
        <Icons.search style={{ color: "var(--text-faint)" }} />
        <input
          data-autofocus
          className="palette__input"
          value={q}
          placeholder={t("palette.placeholder")}
          aria-label={t("palette.label")}
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={filtered[sel] ? `cmd-${sel}` : undefined}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSel((s) => Math.min(filtered.length - 1, s + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setSel((s) => Math.max(0, s - 1));
            } else if (e.key === "Enter" && filtered[sel]) {
              e.preventDefault();
              run(filtered[sel]);
            }
          }}
        />
      </div>
      <div ref={listRef} id="palette-list" role="listbox" style={{ padding: 8, maxHeight: 380, overflow: "auto" }}>
        {filtered.map((c, i) => {
          const I = Icons[c.icon];
          return (
            <button
              key={c.id}
              id={`cmd-${i}`}
              data-idx={i}
              type="button"
              role="option"
              aria-selected={i === sel}
              className="cmd"
              style={{ animationDelay: `${Math.min(i, 14) * 25}ms` }}
              onMouseMove={() => setSel(i)}
              onClick={() => run(c)}
            >
              <span className="cmd__ic">
                <I />
              </span>
              <span className="cmd__label">{c.label}</span>
              <span className="muted">{c.group}</span>
            </button>
          );
        })}
        {!filtered.length && (
          <p className="muted" style={{ padding: 14 }}>
            {t("palette.empty")}
          </p>
        )}
      </div>
    </Modal>
  );
}

export async function toggleFullscreen() {
  const w = getCurrentWindow();
  try {
    await w.setFullscreen(!(await w.isFullscreen()));
  } catch {
    /* ignoré */
  }
}
