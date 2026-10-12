import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Icons, IconButton, Menu, MenuItem, Segmented } from "../../components";
import { formatSize, stripExt } from "../../lib/format";
import { paperName } from "../../lib/edit";
import { pickPdfs } from "../../lib/files";
import { isMod, isTyping, shortcut } from "../../lib/platform";
import { perms } from "../../lib/api";
import { useActiveTab, type DocTab } from "../../stores/app";
import { useDocEdit, useEdit, type DocEdit, type EditMode } from "../../stores/edit";
import { PagesGrid } from "./PagesGrid";
import { ContentView } from "./ContentView";
import { EditSide } from "./EditSide";
import { useContentSel } from "./contentSel";
import "./edit.css";

/** Module Éditer (maquette Organize.dc.html). */
export function Edit() {
  const tab = useActiveTab();
  const ensure = useEdit((s) => s.ensure);
  useEffect(() => {
    if (tab) ensure(tab.key, tab.info);
  }, [tab?.key, tab?.info.id, ensure]); // eslint-disable-line react-hooks/exhaustive-deps
  const d = useDocEdit(tab?.key);
  if (!tab || !d || d.docId !== tab.info.id) return <div className="workspace" />;
  return <EditInner tab={tab} d={d} />;
}

function EditInner({ tab, d }: { tab: DocTab; d: DocEdit }) {
  const { t } = useTranslation();
  const store = useEdit();
  // Pages : droit d'assemblage ou de modification ; contenu : droit de modification.
  const readOnly = !perms(tab.info).edit && !perms(tab.info).extract;
  const contentReadOnly = !perms(tab.info).edit;
  const insertRef = useRef<HTMLButtonElement>(null);
  const [insertOpen, setInsertOpen] = useState(false);

  // ---- Raccourcis du module
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e) || document.querySelector(".scrim, .menu")) return;
      const s = useEdit.getState();
      const cur = s.docs[tab.key];
      if (!cur) return;
      const k = e.key.toLowerCase();
      if (isMod(e)) {
        if (k === "z" && !e.shiftKey) s.undo(tab.key);
        else if (k === "y" || (k === "z" && e.shiftKey)) s.redo(tab.key);
        else if (k === "a" && cur.mode === "pages") s.select(tab.key, cur.pages.map((p) => p.key));
        else return;
        e.preventDefault();
        return;
      }
      if (cur.mode !== "pages" || readOnly) return;
      if ((e.key === "Delete" || e.key === "Backspace") && cur.sel.length) {
        e.preventDefault();
        s.remove(tab.key, cur.sel);
      } else if (e.key === "Escape" && cur.sel.length) {
        s.select(tab.key, []);
      } else if (k === "r" && cur.sel.length && !e.altKey) {
        e.preventDefault();
        s.rotate(tab.key, cur.sel, e.shiftKey ? -1 : 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tab.key, readOnly]);

  const setMode = (mode: EditMode) => {
    if (mode === "content") {
      // Page ouverte : dernière page sélectionnée du document d'origine.
      const pick = [...d.sel].reverse().map((k) => d.pages.find((p) => p.key === k)).find((p) => p?.src.kind === "orig");
      const idx = pick && pick.src.kind === "orig" ? pick.src.index : d.contentPage;
      store.patch(tab.key, { mode, contentPage: idx });
    } else {
      store.patch(tab.key, { mode });
      useContentSel.getState().clear();
    }
  };

  const first = tab.info.pages[0];
  const meta = t("edit.meta", { count: d.pages.length, format: first ? paperName(first.w, first.h) : "—", size: formatSize(tab.info.fileSize) });

  return (
    <div className="workspace edit">
      <main className="panel emain">
        <header className="emain__head">
          <div className="emain__title">
            <h1>{stripExt(tab.info.name)}</h1>
            <span className="muted tnum">{meta}</span>
          </div>
          <div className="emain__actions">
            <Segmented<EditMode>
              label={t("edit.mode")}
              value={d.mode}
              onChange={setMode}
              itemWidth={96}
              options={[
                { value: "pages", label: t("edit.pages") },
                { value: "content", label: t("edit.content") },
              ]}
            />
            <Button ref={insertRef} icon={<Icons.plus />} onClick={() => setInsertOpen((o) => !o)} disabled={readOnly} aria-expanded={insertOpen}>
              {t("edit.insert")}
            </Button>
            <Menu anchor={insertRef} open={insertOpen} onClose={() => setInsertOpen(false)} label={t("edit.insert")} width={320} align="end">
              <MenuItem
                icon={<Icons.file />}
                tileBg="var(--accent-soft)"
                tileFg="var(--accent-text)"
                label={t("edit.insertBlank")}
                desc={t("edit.insertBlankDesc")}
                onSelect={() => store.insertBlank(tab.key)}
              />
              <MenuItem
                icon={<Icons.filePlus />}
                tileBg="var(--info-soft)"
                tileFg="var(--info-text)"
                label={t("edit.insertPdf")}
                desc={t("edit.insertPdfDesc")}
                delay={40}
                onSelect={() => void pickPdfs(t("edit.pickPdf")).then((p) => {
                    if (p.length) void store.insertFiles(tab.key, p);
                  })}
              />
            </Menu>
            <span className="emain__sep" />
            <IconButton label={t("edit.undo")} shortcut={shortcut("Z")} icon={<Icons.undo />} disabled={!d.past.length} onClick={() => store.undo(tab.key)} />
            <IconButton label={t("edit.redo")} shortcut={shortcut("Y")} icon={<Icons.redo />} disabled={!d.future.length} onClick={() => store.redo(tab.key)} />
            <span className="edirty" data-dirty={d.dirty}>
              <span className="edirty__dot" />
              {d.dirty ? t("edit.unsaved") : t("edit.upToDate")}
            </span>
            <Button variant="primary" onClick={() => void store.save(tab.key)} disabled={!d.dirty || d.saving || readOnly} title={`${t("edit.save")} (${shortcut("S")})`}>
              {d.saving ? t("edit.saving") : t("edit.save")}
            </Button>
          </div>
        </header>
        {readOnly && <p className="enotice">{t("edit.readOnly")}</p>}
        {!readOnly && tab.info.signatureCount > 0 && <p className="enotice">{t("edit.signedWarn")}</p>}
        {d.mode === "pages" ? <PagesGrid tab={tab} d={d} readOnly={readOnly} /> : <ContentView tab={tab} d={d} readOnly={contentReadOnly} />}
      </main>
      <EditSide tab={tab} d={d} readOnly={contentReadOnly} />
    </div>
  );
}
