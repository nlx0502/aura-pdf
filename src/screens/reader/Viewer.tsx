import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api } from "../../lib/api";
import {
  anchorAt,
  anchorPoint,
  clampZoom,
  computeLayout,
  currentPageAt,
  fitPageZoom,
  fitWidthZoom,
  visiblePages,
  type Anchor,
  type Box,
} from "../../lib/layout";
import { isMod, isTyping } from "../../lib/platform";
import { useApp, type DocTab } from "../../stores/app";
import { PageView } from "./PageView";
import { AnnotLayer } from "../annotate/AnnotLayer";
import { useEdit } from "../../stores/edit";
import { useDocAnnots } from "../../stores/annot";

const ZOOM_ANIM_MS = 280;
const WHEEL_ANIM_MS = 110;
const ROT_ANIM_MS = 450;

function prefersReducedMotion() {
  const pref = document.documentElement.dataset.motion;
  if (pref === "reduce") return true;
  if (pref === "full") return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Fait tourner une ancre (fractions dans la page) de `q` quarts de tour horaires. */
function rotateAnchor(a: Anchor, q: number): Anchor {
  let { fx, fy } = a;
  const n = ((q % 4) + 4) % 4;
  for (let i = 0; i < n; i++) [fx, fy] = [1 - fy, fx];
  return { page: a.page, fx, fy };
}

interface Anim {
  kind: "zoom" | "rot";
  k: number;
  deg: number;
  ox: number;
  oy: number;
  ms: number;
}

/**
 * Visualiseur virtualisé : seules les pages proches de la fenêtre sont
 * montées. Le zoom et la rotation sont d'abord animés (transformation CSS
 * autour du point visé), puis la nouvelle disposition est appliquée en gardant
 * ce point immobile à l'écran.
 */
export function Viewer({ tab, active, annotate = false }: { tab: DocTab; active: boolean; annotate?: boolean }) {
  const annots = useDocAnnots(tab.key);
  // Révision du rendu : annotations, contenu modifié (Éditer), visibilité des calques.
  const editRev = useEdit((s) => (s.docs[tab.key]?.docId === tab.info.id ? s.docs[tab.key].rev : 0));
  const annotRev = (annots?.rev ?? 0) + editRev * 1_000 + (tab.viewRev ?? 0) * 1_000_000;
  const update = useApp((s) => s.updateTab);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [vp, setVp] = useState({ w: 0, h: 0 });
  const [scroll, setScroll] = useState({ top: 0, left: 0 });
  const [committed, setCommitted] = useState({ zoom: tab.zoom, rot: tab.rot });
  const [anim, setAnim] = useState<Anim | null>(null);
  const pending = useRef<{ anchor: Anchor; sx: number; sy: number; fromRot: number } | null>(null);
  const zoomOrigin = useRef<{ sx: number; sy: number; ms: number } | null>(null);
  const instantNext = useRef(true);
  const genRef = useRef(1);
  const genTimer = useRef(0);
  const commitTimer = useRef(0);
  const tabRef = useRef(tab);
  tabRef.current = tab;
  const doc = tab.info.id;

  const bumpGen = useCallback(() => {
    genRef.current += 1;
    window.clearTimeout(genTimer.current);
    genTimer.current = window.setTimeout(() => void api.setViewGen(doc, genRef.current).catch(() => {}), 60);
  }, [doc]);
  const getGen = useCallback(() => genRef.current, []);

  // ---- Taille de la fenêtre
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      instantNext.current = true;
      setVp({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    setVp({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  // ---- Modes « ajuster » : recalcul du zoom selon la fenêtre
  useEffect(() => {
    if (!vp.w || !tab.fit) return;
    const pages = tab.info.pages;
    const z =
      tab.fit === "width"
        ? fitWidthZoom(pages, tab.rot, tab.view, vp.w - 12)
        : fitPageZoom(pages[tab.page] ?? pages[0], tab.rot, vp.w, vp.h);
    if (Math.abs(z - tab.zoom) >= 1) update(tab.key, { zoom: z });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vp.w, vp.h, tab.fit, tab.view, tab.rot, tab.key]);

  const layout = useMemo(
    () => computeLayout({ pages: tab.info.pages, zoom: committed.zoom, rot: committed.rot, mode: tab.view, current: tab.page, viewportW: vp.w }),
    // En mode « page », la disposition dépend de la page courante.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [tab.info.pages, committed.zoom, committed.rot, tab.view, vp.w, tab.view === "page" ? tab.page : -1],
  );
  const layoutRef = useRef(layout);
  layoutRef.current = layout;

  // ---- Changement de zoom ou de rotation : animation puis application
  const commit = useCallback(() => {
    window.clearTimeout(commitTimer.current);
    setAnim(null);
    setCommitted({ zoom: tabRef.current.zoom, rot: tabRef.current.rot });
  }, []);

  useLayoutEffect(() => {
    if (tab.zoom === committed.zoom && tab.rot === committed.rot) return;
    const el = scrollRef.current;
    if (!el) return;
    const origin = zoomOrigin.current ?? { sx: el.clientWidth / 2, sy: el.clientHeight / 2, ms: ZOOM_ANIM_MS };
    zoomOrigin.current = null;
    if (!pending.current) {
      const ox = el.scrollLeft + origin.sx;
      const oy = el.scrollTop + origin.sy;
      pending.current = { anchor: anchorAt(layoutRef.current, ox, oy), sx: origin.sx, sy: origin.sy, fromRot: committed.rot };
    }
    const instant = instantNext.current || prefersReducedMotion() || !vp.w;
    instantNext.current = false;
    if (instant) {
      commit();
      return;
    }
    const ox = el.scrollLeft + pending.current.sx;
    const oy = el.scrollTop + pending.current.sy;
    const rotating = tab.rot !== committed.rot;
    const ms = rotating ? ROT_ANIM_MS : origin.ms;
    setAnim((prev) => ({
      kind: rotating ? "rot" : "zoom",
      k: tab.zoom / committed.zoom,
      deg: (tab.rot - committed.rot) * 90,
      ox: prev?.ox ?? ox,
      oy: prev?.oy ?? oy,
      ms,
    }));
    window.clearTimeout(commitTimer.current);
    commitTimer.current = window.setTimeout(commit, ms + 20);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.zoom, tab.rot]);

  // Après application : replacer le point visé au même endroit de l'écran.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    const pend = pending.current;
    if (!el || !pend) return;
    pending.current = null;
    const anchor = rotateAnchor(pend.anchor, committed.rot - pend.fromRot);
    const p = anchorPoint(layout, anchor);
    el.scrollLeft = p.x - pend.sx;
    el.scrollTop = p.y - pend.sy;
    setScroll({ top: el.scrollTop, left: el.scrollLeft });
    bumpGen();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [committed]);

  // ---- Saut vers une page (miniature, table des matières, résultat de recherche…)
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el || !vp.h) return;
    const box = layout.boxes[tab.jump.page];
    if (!box) return;
    if (tab.view === "page") {
      el.scrollTop = tab.jump.y !== undefined ? Math.max(0, box.y + tab.jump.y * layout.scale - vp.h * 0.35) : 0;
    } else {
      const y = tab.jump.y !== undefined ? box.y + tab.jump.y * layout.scale - vp.h * 0.35 : box.y - 16;
      el.scrollTop = Math.max(0, y);
    }
    setScroll({ top: el.scrollTop, left: el.scrollLeft });
    bumpGen();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.jump.seq, vp.h > 0, tab.view]);

  // ---- Défilement : page courante et génération de vue
  const raf = useRef(0);
  const settleTimer = useRef(0);
  const onScroll = () => {
    if (raf.current) return;
    raf.current = requestAnimationFrame(() => {
      raf.current = 0;
      const el = scrollRef.current;
      if (!el) return;
      setScroll({ top: el.scrollTop, left: el.scrollLeft });
      if (tab.view !== "page" && !pending.current) {
        const cur = currentPageAt(layoutRef.current, el.scrollTop, el.clientHeight);
        if (cur !== tabRef.current.page) update(tab.key, { page: cur });
      }
      window.clearTimeout(settleTimer.current);
      settleTimer.current = window.setTimeout(bumpGen, 120);
    });
  };

  // ---- Molette : Ctrl+molette zoome autour du pointeur ; en mode page, tourne les pages
  const pageFlipAt = useRef(0);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const t = tabRef.current;
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        const rect = el.getBoundingClientRect();
        const factor = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0022));
        const z = clampZoom(t.zoom * factor);
        if (z === t.zoom) return;
        zoomOrigin.current = { sx: e.clientX - rect.left, sy: e.clientY - rect.top, ms: WHEEL_ANIM_MS };
        update(t.key, { zoom: z, fit: null });
        return;
      }
      if (t.view === "page") {
        const now = performance.now();
        if (now - pageFlipAt.current < 350) return;
        const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 2;
        const atTop = el.scrollTop <= 0;
        if (e.deltaY > 0 && atBottom && t.page < t.info.pageCount - 1) {
          pageFlipAt.current = now;
          useApp.getState().jumpTo(t.key, t.page + 1);
        } else if (e.deltaY < 0 && atTop && t.page > 0) {
          pageFlipAt.current = now;
          useApp.getState().jumpTo(t.key, t.page - 1);
        }
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [update]);

  // ---- Clavier : navigation dans le document
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e) || e.altKey || document.querySelector(".scrim")) return;
      const t = tabRef.current;
      const el = scrollRef.current;
      if (!el) return;
      const jump = (p: number) => useApp.getState().jumpTo(t.key, p);
      const step = t.view === "double" ? 2 : 1;
      if (isMod(e)) {
        if (e.key === "Home") jump(0);
        else if (e.key === "End") jump(t.info.pageCount - 1);
        else return;
        e.preventDefault();
        return;
      }
      switch (e.key) {
        case "PageDown":
          if (t.view === "page") jump(t.page + step);
          else el.scrollBy({ top: el.clientHeight * 0.9 });
          break;
        case "PageUp":
          if (t.view === "page") jump(t.page - step);
          else el.scrollBy({ top: -el.clientHeight * 0.9 });
          break;
        case "ArrowRight":
          if (el.scrollWidth > el.clientWidth + 2) return;
          jump(t.page + step);
          break;
        case "ArrowLeft":
          if (el.scrollWidth > el.clientWidth + 2) return;
          jump(t.page - step);
          break;
        case "Home":
          jump(0);
          break;
        case "End":
          jump(t.info.pageCount - 1);
          break;
        case " ":
          if (t.view === "page") jump(t.page + (e.shiftKey ? -step : step));
          else el.scrollBy({ top: (e.shiftKey ? -1 : 1) * el.clientHeight * 0.85 });
          break;
        default:
          return;
      }
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);

  // Mémoriser la dernière page lue (reprise de lecture), sans écrire à chaque défilement.
  useEffect(() => {
    const timer = window.setTimeout(() => void api.recentsSetPage(tab.info.path, tab.page).catch(() => {}), 800);
    return () => window.clearTimeout(timer);
  }, [tab.page, tab.info.path]);

  useEffect(
    () => () => {
      window.clearTimeout(genTimer.current);
      window.clearTimeout(commitTimer.current);
      window.clearTimeout(settleTimer.current);
    },
    [],
  );

  // ---- Pages à monter
  const margin = vp.h * (anim?.kind === "zoom" && anim.k < 1 ? 1 / anim.k : 1);
  const mounted = visiblePages(layout, scroll.top, scroll.top + vp.h, margin);
  const hitsByPage = useMemo(() => {
    const m = new Map<number, typeof tab.search.hits>();
    for (const h of tab.search.hits) {
      if (!m.has(h.page)) m.set(h.page, []);
      m.get(h.page)!.push(h);
    }
    return m;
  }, [tab.search.hits]);
  const activeHit = tab.search.active >= 0 ? (tab.search.hits[tab.search.active] ?? null) : null;

  const layerStyle: React.CSSProperties = anim
    ? {
        transformOrigin: `${anim.ox}px ${anim.oy}px`,
        transform: anim.kind === "rot" ? `rotate(${anim.deg}deg)` : `scale(${anim.k})`,
        transition: `transform ${anim.ms}ms cubic-bezier(.2,.8,.2,1)`,
      }
    : { transform: "none", transition: "none" };

  const visibleBox = (b: Box): Box | null => {
    const x0 = Math.max(b.x, scroll.left);
    const y0 = Math.max(b.y, scroll.top);
    const x1 = Math.min(b.x + b.w, scroll.left + vp.w);
    const y1 = Math.min(b.y + b.h, scroll.top + vp.h);
    if (x1 <= x0 || y1 <= y0) return null;
    return { x: x0 - b.x, y: y0 - b.y, w: x1 - x0, h: y1 - y0 };
  };

  return (
    <div ref={scrollRef} className="viewer" onScroll={onScroll} tabIndex={-1}>
      <div className="viewer__content" style={{ width: layout.width, height: layout.height }}>
        <div className="viewer__layer" style={layerStyle}>
          {mounted.map((i) => (
            <PageView
              key={i}
              doc={doc}
              page={i}
              box={layout.boxes[i]}
              size={tab.info.pages[i]}
              scale={layout.scale}
              rot={committed.rot}
              dark={tab.dark}
              annots={tab.layers.annotations}
              forms={tab.layers.forms}
              settled={!anim}
              visible={visibleBox(layout.boxes[i])}
              getGen={getGen}
              hits={hitsByPage.get(i) ?? []}
              activeHit={activeHit && activeHit.page === i ? activeHit : null}
              onGoto={(p) => useApp.getState().jumpTo(tab.key, p)}
              rev={annotRev}
              overlay={
                annots?.loaded ? (
                  <AnnotLayer tabKey={tab.key} docId={doc} page={i} size={tab.info.pages[i]} scale={layout.scale} interactive={annotate && !anim} />
                ) : null
              }
            />
          ))}
        </div>
      </div>
    </div>
  );
}
