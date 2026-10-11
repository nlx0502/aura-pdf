import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icons, IconButton, Segmented, TextField, ToastHost } from "../../components";
import { nextZoom } from "../../lib/layout";
import { isMod, shortcut } from "../../lib/platform";
import { useActiveTab, useApp, type DocTab } from "../../stores/app";
import { LeftPanel } from "./LeftPanel";
import { RightPanel } from "./RightPanel";
import { Viewer } from "./Viewer";
import { gotoHit, runSearch } from "./search";
import "./reader.css";
import { requestCloseTab } from "../../shell/Unsaved";
import { useEdit } from "../../stores/edit";
import { useAnnots } from "../../stores/annot";

function useNarrow() {
  const [narrow, setNarrow] = useState(() => window.innerWidth < 1280);
  useEffect(() => {
    const on = () => setNarrow(window.innerWidth < 1280);
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  return narrow;
}

/** Écran de lecture : navigation, document, informations. */
export function Reader() {
  const tab = useActiveTab();
  const focus = useApp((s) => s.focus);
  const leftOpen = useApp((s) => s.leftOpen);
  const rightOpen = useApp((s) => s.rightOpen);
  const set = useApp((s) => s.set);
  const narrow = useNarrow();
  // En passant sous 1280 px, les panneaux se replient (ils s'ouvrent ensuite en surimpression).
  useEffect(() => {
    if (narrow) set({ leftOpen: false, rightOpen: false });
  }, [narrow, set]);
  if (!tab) return null;

  const showLeft = !focus && (narrow ? leftOpen : true);
  const showRight = !focus && (narrow ? rightOpen : true);

  return (
    <div className={`workspace reader ${narrow ? "reader--narrow" : ""}`}>
      {showLeft && (
        <>
          {narrow && <button type="button" className="reader__scrim" aria-hidden="true" tabIndex={-1} onClick={() => set({ leftOpen: false })} />}
          <LeftPanel tab={tab} />
        </>
      )}
      <main className="panel reader__main">
        <TabsRow />
        <Toolbar tab={tab} narrow={narrow} />
        <Viewer key={tab.key} tab={tab} active />
        <PageNav tab={tab} />
        <ToastHost only="right" contained />
      </main>
      {showRight && (
        <>
          {narrow && <button type="button" className="reader__scrim reader__scrim--right" aria-hidden="true" tabIndex={-1} onClick={() => set({ rightOpen: false })} />}
          <RightPanel tab={tab} />
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- onglets

function TabsRow() {
  const { t } = useTranslation();
  const tabs = useApp((s) => s.tabs);
  const active = useApp((s) => s.active);
  const activate = useApp((s) => s.activate);
  const set = useApp((s) => s.set);
  const dirtyJson = useAnnots((s) => JSON.stringify(Object.fromEntries(Object.entries(s.docs).map(([k, v]) => [k, v.dirty]))));
  const editDirtyJson = useEdit((s) => JSON.stringify(Object.fromEntries(Object.entries(s.docs).map(([k, v]) => [k, v.dirty]))));
  const dirty = { ...(JSON.parse(dirtyJson) as Record<string, boolean>) };
  for (const [k, v] of Object.entries(JSON.parse(editDirtyJson) as Record<string, boolean>)) if (v) dirty[k] = true;
  return (
    <div className="tabs" role="tablist" aria-label={t("reader.openDocs")}>
      {tabs.map((tb) => {
        const sel = tb.key === active;
        return (
          <div key={tb.key} className={`tab rise-sm ${sel ? "tab--sel" : ""}`}>
            <button
              type="button"
              role="tab"
              aria-selected={sel}
              className="tab__btn"
              onClick={() => activate(tb.key)}
              onMouseDown={(e) => {
                if (e.button === 1) {
                  e.preventDefault();
                  requestCloseTab(tb.key);
                }
              }}
              title={tb.info.path}
            >
              <Icons.file className="i tab__icon" />
              <span className="ellipsis">{tb.info.name}</span>
              {dirty[tb.key] && <span className="tab__dirty" title={t("annot.unsaved")} aria-label={t("annot.unsaved")} />}
            </button>
            <button type="button" className="btn btn--ghost btn--icon tab__close" aria-label={t("reader.closeTab", { name: tb.info.name })} onClick={() => requestCloseTab(tb.key)}>
              <Icons.close size={12} />
            </button>
          </div>
        );
      })}
      <IconButton label={t("reader.newTab")} size="sm" icon={<Icons.plus />} onClick={() => set({ paletteOpen: true })} shortcut={shortcut("K")} />
    </div>
  );
}

// ---------------------------------------------------------------- barre d'outils

function Toolbar({ tab, narrow }: { tab: DocTab; narrow: boolean }) {
  const { t } = useTranslation();
  const update = useApp((s) => s.updateTab);
  const focus = useApp((s) => s.focus);
  const leftOpen = useApp((s) => s.leftOpen);
  const rightOpen = useApp((s) => s.rightOpen);
  const set = useApp((s) => s.set);
  const [q, setQ] = useState(tab.search.query);
  const searchRef = useRef<HTMLInputElement>(null);
  const debounce = useRef(0);

  useEffect(() => setQ(tab.search.query), [tab.key]); // eslint-disable-line react-hooks/exhaustive-deps

  // Ctrl+F : aller au champ de recherche.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isMod(e) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const onQ = (v: string) => {
    setQ(v);
    window.clearTimeout(debounce.current);
    debounce.current = window.setTimeout(() => runSearch(tab.key, v), 250);
  };

  const s = tab.search;
  const searching = s.query.trim().length >= 2;
  const hitLabel = searching ? (s.hits.length ? `${s.active + 1} / ${s.hits.length}` : s.done ? t("reader.found", { count: 0 }) : "…") : "";

  const panelsOn = narrow ? leftOpen : !focus;

  return (
    <div className="toolbar">
      <IconButton
        label={t("reader.showPanels")}
        on={panelsOn}
        aria-pressed={panelsOn}
        icon={<Icons.panelLeft />}
        onClick={() => (narrow ? set({ leftOpen: !leftOpen, rightOpen: false }) : set({ focus: !focus }))}
      />
      <Segmented
        label={t("reader.viewMode")}
        value={tab.view}
        onChange={(v) => update(tab.key, (tb) => ({ view: v, jump: { page: tb.page, seq: tb.jump.seq + 1 } }))}
        itemWidth={narrow ? 72 : 92}
        options={[
          { value: "continu", label: t("reader.views.continu") },
          { value: "page", label: t("reader.views.page") },
          { value: "double", label: t("reader.views.double") },
        ]}
      />
      <div className="zoombox">
        <IconButton size="sm" label={t("reader.zoomOut")} shortcut={shortcut("-")} icon={<Icons.minus />} onClick={() => update(tab.key, { zoom: nextZoom(tab.zoom, -1), fit: null })} />
        <button type="button" className="btn btn--ghost btn--sm tnum zoombox__val" title={t("reader.fitWidth")} onClick={() => update(tab.key, { fit: "width" })}>
          {Math.round(tab.zoom)} %
        </button>
        <IconButton size="sm" label={t("reader.zoomIn")} shortcut={shortcut("+")} icon={<Icons.plus />} onClick={() => update(tab.key, { zoom: nextZoom(tab.zoom, 1), fit: null })} />
      </div>
      <IconButton label={t("reader.rotate")} icon={<Icons.rotate style={{ transform: `rotate(${tab.rot * 90}deg)`, transition: "transform .5s cubic-bezier(.2,.8,.2,1)" }} />} onClick={() => update(tab.key, { rot: tab.rot + 1 })} />
      <div className="toolbar__search">
        <TextField
          ref={searchRef}
          pill
          small
          icon={<Icons.search />}
          placeholder={t("reader.searchPlaceholder")}
          aria-label={t("reader.searchPlaceholder")}
          value={q}
          onChange={(e) => onQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              window.clearTimeout(debounce.current);
              if (q !== tab.search.query) runSearch(tab.key, q);
              else gotoHit(tab.key, tab.search.active + (e.shiftKey ? -1 : 1));
            } else if (e.key === "Escape" && q) {
              e.stopPropagation();
              onQ("");
            }
          }}
          suffix={
            hitLabel ? (
              <span className="tnum toolbar__hits" style={{ color: s.hits.length ? "var(--search-hit-text)" : "var(--text-faint)" }}>
                {hitLabel}
              </span>
            ) : undefined
          }
          style={{ maxWidth: 360 }}
        />
      </div>
      <IconButton label={t("reader.darkMode")} on={tab.dark} aria-pressed={tab.dark} icon={<Icons.moon />} onClick={() => update(tab.key, { dark: !tab.dark })} />
      <IconButton label={t("reader.readingMode")} shortcut="F" on={focus} aria-pressed={focus} icon={<Icons.focus />} onClick={() => set({ focus: !focus })} />
      {narrow && <IconButton label={t("reader.info")} on={rightOpen} aria-pressed={rightOpen} icon={<Icons.panelRight />} onClick={() => set({ rightOpen: !rightOpen, leftOpen: false })} />}
      <IconButton label={t("reader.print")} shortcut={shortcut("P")} tipAlign="end" icon={<Icons.print />} onClick={() => set({ printOpen: true })} />
    </div>
  );
}

// ---------------------------------------------------------------- navigation flottante

function PageNav({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const jumpTo = useApp((s) => s.jumpTo);
  const [editing, setEditing] = useState<string | null>(null);
  const n = tab.info.pageCount;
  const step = tab.view === "double" ? 2 : 1;
  const progress = Math.round(((tab.page + 1) / n) * 100);
  return (
    <div className="pagenav">
      <IconButton size="sm" label={t("reader.prevPage")} tipSide="top" icon={<Icons.chevronUp />} disabled={tab.page <= 0} onClick={() => jumpTo(tab.key, tab.page - step)} />
      <span className="pagenav__label tnum">
        {t("reader.pageWord")}{" "}
        <input
          className="pagenav__input"
          aria-label={t("reader.pageInput")}
          value={editing ?? String(tab.page + 1)}
          style={{ width: `${Math.max(1, String(n).length) + 0.6}ch` }}
          onFocus={(e) => {
            setEditing(String(tab.page + 1));
            requestAnimationFrame(() => e.target.select());
          }}
          onChange={(e) => setEditing(e.target.value.replace(/\D/g, ""))}
          onBlur={() => setEditing(null)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              const v = parseInt(editing ?? "", 10);
              if (!Number.isNaN(v)) jumpTo(tab.key, Math.min(n, Math.max(1, v)) - 1);
              (e.target as HTMLInputElement).blur();
            } else if (e.key === "Escape") {
              setEditing(null);
              (e.target as HTMLInputElement).blur();
            }
          }}
        />{" "}
        {t("reader.of")} {n}
      </span>
      <IconButton size="sm" label={t("reader.nextPage")} tipSide="top" icon={<Icons.chevronDown />} disabled={tab.page >= n - 1} onClick={() => jumpTo(tab.key, tab.page + step)} />
      <span className="pagenav__bar" aria-hidden="true">
        <span style={{ width: `${progress}%` }} />
      </span>
    </div>
  );
}
