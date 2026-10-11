import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { invoke } from "@tauri-apps/api/core";
import { Button, IconButton, Icons, Menu, MenuItem, toast } from "../components";
import { useActiveTab, useApp, type ModuleId } from "../stores/app";
import { shortcut } from "../lib/platform";
import { asAppError } from "../lib/api";
import { WindowControls } from "./WindowControls";

const MODULES: { id: ModuleId; icon: keyof typeof Icons }[] = [
  { id: "lire", icon: "read" },
  { id: "annoter", icon: "annotate" },
  { id: "editer", icon: "edit" },
  { id: "signer", icon: "sign" },
  { id: "securite", icon: "security" },
];

type ToolId = "fusion" | "forms" | "compress" | "convert" | "print";

const TOOLS: { id: ToolId; icon: keyof typeof Icons; bg: string; fg: string }[] = [
  { id: "fusion", icon: "merge", bg: "var(--accent-soft)", fg: "var(--accent-text)" },
  { id: "forms", icon: "forms", bg: "var(--info-soft)", fg: "var(--info-text)" },
  { id: "compress", icon: "compress", bg: "var(--success-soft)", fg: "var(--success)" },
  { id: "convert", icon: "convert", bg: "var(--warning-soft)", fg: "var(--warning-text)" },
  { id: "print", icon: "print", bg: "var(--pink-soft)", fg: "var(--pink-text)" },
];

/** Logo Aura : retour à l'accueil. */
export function Logo({ width }: { width?: number }) {
  const { t } = useTranslation();
  const goHome = useApp((s) => s.goHome);
  return (
    <button type="button" className="logo" aria-label={t("app.homeLink")} onClick={goHome} style={{ width }}>
      <span className="logo__mark">
        <Icons.file />
      </span>
      <span className="logo__name">{t("app.brand")}</span>
    </button>
  );
}

/** En-tête partagé des écrans de document : modules, menu Outils, Imprimer, Exporter. */
export function AppBar() {
  const { t } = useTranslation();
  const module = useApp((s) => s.module);
  const setModule = useApp((s) => s.setModule);
  const set = useApp((s) => s.set);
  const tab = useActiveTab();
  const [toolsOpen, setToolsOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const toolsRef = useRef<HTMLButtonElement>(null);
  const exportRef = useRef<HTMLButtonElement>(null);
  const curTool = TOOLS.find((x) => x.id === module);
  const printKey = shortcut("P");

  const saveCopy = async () => {
    if (!tab) return;
    const dest = await save({
      defaultPath: tab.info.name.replace(/\.pdf$/i, "") + " (copie).pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!dest) return;
    try {
      await invoke("copy_file", { src: tab.info.path, dest });
      toast.success(t("exportMenu.copied"));
    } catch (e) {
      toast.error(asAppError(e).message);
    }
  };

  return (
    <header className="appbar" data-tauri-drag-region>
      <Logo />
      <nav className="appbar__nav" aria-label={t("nav.modules")} data-tauri-drag-region>
        {MODULES.map((m) => {
          const I = Icons[m.icon];
          return (
            <button
              key={m.id}
              type="button"
              className="navpill"
              aria-current={module === m.id ? "page" : undefined}
              onClick={() => setModule(m.id)}
            >
              <I />
              {t(`nav.${m.id}`)}
            </button>
          );
        })}
        <span className="appbar__sep" aria-hidden="true" />
        <button
          ref={toolsRef}
          type="button"
          className={`navpill navpill--tools ${curTool ? "is-cur" : ""}`}
          aria-expanded={toolsOpen}
          aria-haspopup="menu"
          onClick={() => setToolsOpen((o) => !o)}
        >
          <Icons.tools />
          {curTool ? t("nav.toolsWith", { tool: t(`tools.${curTool.id}.label`) }) : t("nav.tools")}
          <Icons.chevronDown size={14} className="i navpill__chev" style={{ transform: `rotate(${toolsOpen ? 180 : 0}deg)` }} />
        </button>
        <Menu anchor={toolsRef} open={toolsOpen} onClose={() => setToolsOpen(false)} label={t("nav.tools")}>
          <div className="menu__head">
            <strong>{t("nav.tools")}</strong>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("nav.toolsCount", { count: TOOLS.length })}
            </span>
          </div>
          {TOOLS.map((tool, i) => {
            const I = Icons[tool.icon];
            return (
              <MenuItem
                key={tool.id}
                icon={<I />}
                tileBg={tool.bg}
                tileFg={tool.fg}
                label={t(`tools.${tool.id}.label`)}
                desc={t(`tools.${tool.id}.desc`)}
                kbd={tool.id === "print" ? printKey : undefined}
                current={module === tool.id}
                delay={30 + i * 35}
                onSelect={() => (tool.id === "print" ? set({ printOpen: true }) : setModule(tool.id))}
              />
            );
          })}
        </Menu>
      </nav>
      <div className="appbar__actions">
        <IconButton label={t("nav.print")} shortcut={printKey} tipAlign="end" icon={<Icons.print />} onClick={() => set({ printOpen: true })} />
        <Button ref={exportRef} variant="primary" icon={<Icons.export />} aria-haspopup="menu" aria-expanded={exportOpen} onClick={() => setExportOpen((o) => !o)} disabled={!tab}>
          {t("nav.export")}
        </Button>
        <Menu anchor={exportRef} open={exportOpen} onClose={() => setExportOpen(false)} label={t("exportMenu.title")} align="end" width={320}>
          <MenuItem compact icon={<Icons.fileText />} tileBg="var(--accent-soft)" tileFg="var(--accent-text)" label={t("exportMenu.saveCopy")} desc={t("exportMenu.saveCopyDesc")} onSelect={() => void saveCopy()} />
          <MenuItem
            compact
            icon={<Icons.folderOpen />}
            tileBg="var(--bg-control-selected)"
            tileFg="var(--text-2)"
            label={t("exportMenu.reveal")}
            desc={t("exportMenu.revealDesc")}
            onSelect={() => tab && void revealItemInDir(tab.info.path)}
          />
          <MenuItem compact icon={<Icons.image />} tileBg="var(--pink-soft)" tileFg="var(--pink-text)" label={t("exportMenu.images")} desc={t("common.milestone", { n: 8 })} disabled onSelect={() => {}} />
          <MenuItem compact icon={<Icons.convert />} tileBg="var(--info-soft)" tileFg="var(--info-text)" label={t("exportMenu.word")} desc={t("common.milestone", { n: 8 })} disabled onSelect={() => {}} />
        </Menu>
        <WindowControls />
      </div>
    </header>
  );
}
