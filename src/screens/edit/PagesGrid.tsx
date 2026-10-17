import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, IconButton, Icons } from "../../components";
import { fillNumber, MM, pageNumber, shownSize, toggleSelection, type EditPage, type EditSettings } from "../../lib/edit";
import { savePdfAs } from "../../lib/files";
import { stripExt } from "../../lib/format";
import { Prio, renderPage } from "../../lib/render";
import type { DocTab } from "../../stores/app";
import { useEdit, type DocEdit } from "../../stores/edit";

/** Animations autorisées (réglage de l'application ou de Windows). */
export function motionOK() {
  const pref = document.documentElement.dataset.motion;
  if (pref === "reduce") return false;
  if (pref === "full") return true;
  return !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

interface Drag {
  keys: string[];
  x: number;
  y: number;
  /** Index d'insertion dans la liste des pages. */
  to: number | null;
}

/** Grille des pages : sélection, glisser-déposer, barre d'actions. */
export function PagesGrid({ tab, d, readOnly }: { tab: DocTab; d: DocEdit; readOnly: boolean }) {
  const { t } = useTranslation();
  const store = useEdit();
  const gridRef = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const order = d.pages.map((p) => p.key);
  const selSet = useMemo(() => new Set(d.sel), [d.sel]);

  // ---- Réorganisation animée (FLIP) : chaque carte glisse vers sa nouvelle place.
  const pos = useRef(new Map<string, { x: number; y: number }>());
  const orderKey = order.join(",");
  useLayoutEffect(() => {
    const grid = gridRef.current;
    if (!grid) return;
    const anim = motionOK();
    grid.querySelectorAll<HTMLElement>("[data-key]").forEach((el) => {
      const k = el.dataset.key!;
      const now = { x: el.offsetLeft, y: el.offsetTop };
      const prev = pos.current.get(k);
      if (anim && prev && (prev.x !== now.x || prev.y !== now.y)) {
        el.animate([{ transform: `translate(${prev.x - now.x}px, ${prev.y - now.y}px)` }, { transform: "none" }], { duration: 380, easing: "cubic-bezier(.2,.8,.2,1)" });
      }
      pos.current.set(k, now);
    });
  }, [orderKey]);

  // ---- Glisser-déposer
  const startDrag = (e: React.PointerEvent, key: string) => {
    if (readOnly || e.button !== 0) return;
    const x0 = e.clientX;
    const y0 = e.clientY;
    let active = false;
    const keys = selSet.has(key) ? order.filter((k) => selSet.has(k)) : [key];
    const target = (cx: number, cy: number): number | null => {
      const cards = Array.from(gridRef.current?.querySelectorAll<HTMLElement>("[data-key]") ?? []);
      let best: { i: number; dist: number; after: boolean } | null = null;
      cards.forEach((el, i) => {
        const r = el.getBoundingClientRect();
        const mx = r.left + r.width / 2;
        const my = r.top + r.height / 2;
        const dist = Math.hypot(cx - mx, (cy - my) * 1.5);
        if (!best || dist < best.dist) best = { i, dist, after: cx > mx };
      });
      if (!best) return null;
      const b = best as { i: number; after: boolean };
      return b.i + (b.after ? 1 : 0);
    };
    const move = (ev: PointerEvent) => {
      if (!active && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 6) return;
      active = true;
      setDrag({ keys, x: ev.clientX, y: ev.clientY, to: target(ev.clientX, ev.clientY) });
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (active) {
        const to = target(ev.clientX, ev.clientY);
        setDrag(null);
        if (to !== null) store.moveTo(tab.key, keys, to);
        // Le clic qui suit le relâchement ne doit pas changer la sélection.
        const stop = (c: MouseEvent) => {
          c.stopPropagation();
          c.preventDefault();
        };
        window.addEventListener("click", stop, { capture: true, once: true });
        window.setTimeout(() => window.removeEventListener("click", stop, { capture: true }), 0);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const onToggle = (key: string, shift: boolean) => store.select(tab.key, toggleSelection(order, d.sel, d.last, key, shift), key);
  const openContent = (p: EditPage) => {
    if (p.src.kind !== "orig") return;
    store.patch(tab.key, { mode: "content", contentPage: p.src.index, sel: [p.key], last: p.key });
  };

  const extract = async () => {
    // (Scénarios de test : chemin imposé, sans dialogue.)
    const testPath = import.meta.env.DEV ? (window as unknown as { __auraSavePath?: string }).__auraSavePath : undefined;
    const out = testPath ?? (await savePdfAs(t("edit.extractTitle"), t("edit.extractName", { name: stripExt(tab.info.name) })));
    if (out) await store.extract(tab.key, d.sel, out);
  };

  const all = d.sel.length === d.pages.length;
  const dropIndex = drag?.to ?? null;

  return (
    <>
      <p className="egrid__hint muted">{t("edit.gridHint")}</p>
      <div ref={gridRef} className={`egrid ${drag ? "is-dragging" : ""}`} role="listbox" aria-multiselectable aria-label={t("edit.pages")}>
        {d.pages.map((p, i) => (
          <PageCard
            key={p.key}
            p={p}
            index={i}
            total={d.pages.length}
            settings={d.settings}
            selected={selSet.has(p.key)}
            leaving={d.leaving.includes(p.key)}
            scanning={d.redact.scanning.includes(p.key)}
            dragged={!!drag && drag.keys.includes(p.key)}
            dropBefore={dropIndex === i}
            dropAfter={dropIndex === d.pages.length && i === d.pages.length - 1}
            cropSel={selSet.has(p.key)}
            rev={p.view && p.view.doc === tab.info.id ? d.rev : 0}
            onToggle={onToggle}
            onOpen={openContent}
            onPointerDown={startDrag}
          />
        ))}
      </div>
      {drag && (
        <div className="edrag" style={{ left: drag.x, top: drag.y }} aria-hidden="true">
          <span className="edrag__stack" data-count={drag.keys.length}>
            {drag.keys.length}
          </span>
        </div>
      )}
      {d.sel.length > 0 && !readOnly && (
        <div className="eselbar" role="toolbar" aria-label={t("edit.selected", { count: d.sel.length })}>
          <span className="eselbar__count tnum">{t("edit.selected", { count: d.sel.length })}</span>
          <Button variant="ghost" icon={<Icons.rotate />} onClick={() => store.rotate(tab.key, d.sel)}>
            {t("edit.rotate")}
          </Button>
          <IconButton label={t("edit.moveBefore")} icon={<Icons.chevronLeft />} tipSide="top" onClick={() => store.move(tab.key, -1)} />
          <IconButton label={t("edit.moveAfter")} icon={<Icons.chevronRight />} tipSide="top" onClick={() => store.move(tab.key, 1)} />
          <Button variant="ghost" icon={<Icons.extract />} onClick={() => void extract()}>
            {t("edit.extract")}
          </Button>
          <Button variant="ghost" className="eselbar__danger" icon={<Icons.trash />} onClick={() => store.remove(tab.key, d.sel)}>
            {t("edit.remove")}
          </Button>
          <span className="eselbar__sep" />
          <Button variant="ghost" onClick={() => store.select(tab.key, all ? [] : order)}>
            {all ? t("edit.selectNone") : t("edit.selectAll")}
          </Button>
          <IconButton label={t("edit.clearSel")} icon={<Icons.close />} tipSide="top" tipAlign="end" onClick={() => store.select(tab.key, [])} />
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------- carte de page

interface CardProps {
  p: EditPage;
  index: number;
  total: number;
  settings: EditSettings;
  selected: boolean;
  leaving: boolean;
  scanning: boolean;
  dragged: boolean;
  dropBefore: boolean;
  dropAfter: boolean;
  cropSel: boolean;
  rev: number;
  onToggle: (key: string, shift: boolean) => void;
  onOpen: (p: EditPage) => void;
  onPointerDown: (e: React.PointerEvent, key: string) => void;
}

const PageCard = memo(function PageCard(c: CardProps) {
  const { t } = useTranslation();
  const { p, settings: s } = c;
  const shown = shownSize(p);
  const max = Math.max(shown.w, shown.h);
  const rotDeg = p.rot * 90;
  const rotLabel = ((rotDeg % 360) + 360) % 360;
  const cropOn = s.crop.on && (s.crop.scope === "all" || c.cropSel || p.crop);
  return (
    <div
      className={`ecard ${c.leaving ? "is-leaving" : ""} ${c.dragged ? "is-dragged" : ""} ${c.dropBefore ? "drop-before" : ""} ${c.dropAfter ? "drop-after" : ""}`}
      data-key={p.key}
      style={{ animationDelay: `${Math.min(c.index, 12) * 35}ms` }}
    >
      <button
        type="button"
        className="ecard__pg"
        role="option"
        aria-selected={c.selected}
        aria-label={t("edit.pageLabel", { n: c.index + 1 })}
        onClick={(e) => c.onToggle(p.key, e.shiftKey)}
        onDoubleClick={() => c.onOpen(p)}
        onPointerDown={(e) => c.onPointerDown(e, p.key)}
      >
        <span className="ecard__box" aria-hidden="true">
          <Icons.check />
        </span>
        <span
          className="ecard__sheet"
          style={{ width: `${(p.w / max) * 82}%`, height: `${(p.h / max) * 82}%`, transform: `translate(-50%, -50%) rotate(${rotDeg}deg)` }}
        >
          {p.view ? <LazyThumb doc={p.view.doc} page={p.view.page} rev={c.rev} /> : <span className="ecard__blank">{t("edit.blank")}</span>}
          {p.redact.map((r, i) => (
            <span
              key={i}
              className="ecard__mark"
              style={{ left: `${(r.x / p.w) * 100}%`, top: `${(r.y / p.h) * 100}%`, width: `${(r.w / p.w) * 100}%`, height: `${(r.h / p.h) * 100}%`, animationDelay: `${(i % 6) * 0.08}s` }}
            />
          ))}
          {c.scanning && <span className="ecard__scan" />}
        </span>
        <span className="ecard__over" style={{ width: `${(shown.w / max) * 82}%`, height: `${(shown.h / max) * 82}%` }} aria-hidden="true">
          <Overlays settings={s} n={c.index + 1} total={c.total} w={shown.w} h={shown.h} crop={cropOn} />
        </span>
      </button>
      <span className="ecard__label tnum" data-selected={c.selected}>
        {c.index + 1}
        {rotLabel !== 0 && <span className="echip echip--warn rise">{rotLabel}°</span>}
        {p.redact.length > 0 && <span className="echip echip--danger rise">{t("edit.redactedTag")}</span>}
        {p.src.kind !== "orig" && <span className="echip echip--accent rise">{t("edit.insertedTag")}</span>}
      </span>
    </div>
  );
});

let measureCtx: CanvasRenderingContext2D | null = null;
/** Largeur d'un texte en Helvetica gras pour un corps de 1 point. */
function boldWidth1(text: string) {
  measureCtx ??= document.createElement("canvas").getContext("2d");
  if (!measureCtx) return text.length * 0.6;
  measureCtx.font = "bold 100px Helvetica, Arial, sans-serif";
  return measureCtx.measureText(text).width / 100;
}

/** Aperçu en direct des ajouts, dans le sens d'affichage final (mêmes règles que le cœur Rust). */
export function Overlays({ settings: s, n, total, w, h, crop }: { settings: EditSettings; n: number; total: number; w: number; h: number; crop: boolean }) {
  const pct = (v: number, of: number) => `${(v / of) * 100}%`;
  // Corps minimal lisible sur les petites miniatures (comme la maquette).
  const fs = (pt: number) => `max(5px, ${(pt / w) * 100}cqw)`;
  const wm = s.watermark.text.trim();
  let wmSize = 0;
  if (wm) {
    const a = (35 * Math.PI) / 180;
    const span = Math.min(w * Math.cos(a) + h * Math.sin(a), Math.hypot(w, h));
    wmSize = Math.min(160, Math.max(12, (span * 0.75) / Math.max(0.01, boldWidth1(wm))));
  }
  const pos = s.numbering.position;
  const num = pageNumber(n - 1, total, s.numbering.start, s.numbering.skipFirst);
  const numStyle: React.CSSProperties = {
    fontSize: fs(9),
    ...(pos[0] === "t" ? { top: pct(28, h) } : { bottom: pct(26, h) }),
    ...(pos[1] === "l" ? { left: pct(40, w) } : pos[1] === "r" ? { right: pct(40, w) } : { left: 0, right: 0, textAlign: "center" }),
  };
  return (
    <>
      {wm && (
        <span className="eov__wm">
          <span style={{ fontSize: fs(wmSize), color: hexA(s.watermark.color, s.watermark.opacity / 100) }}>{wm}</span>
        </span>
      )}
      {s.header.on && s.header.header.trim() && (
        <span className="eov__line rise" style={{ top: pct(28, h), fontSize: fs(9) }}>
          {fillNumber(s.header.header.trim(), n, total)}
        </span>
      )}
      {s.header.on && s.header.footer.trim() && (
        <span className="eov__line rise" style={{ bottom: pct(26, h), fontSize: fs(9) }}>
          {fillNumber(s.header.footer.trim(), n, total)}
        </span>
      )}
      {s.numbering.on && num && (
        <span className="eov__num" style={numStyle}>
          {fillNumber(s.numbering.format, num.n, num.total)}
        </span>
      )}
      {crop && (
        <span
          className="eov__crop"
          style={{ top: pct(s.crop.top * MM, h), bottom: pct(s.crop.bottom * MM, h), left: pct(s.crop.left * MM, w), right: pct(s.crop.right * MM, w) }}
        />
      )}
    </>
  );
}

export function hexA(hex: string, a: number) {
  const v = hex.replace("#", "");
  const r = parseInt(v.slice(0, 2), 16) || 0;
  const g = parseInt(v.slice(2, 4), 16) || 0;
  const b = parseInt(v.slice(4, 6), 16) || 0;
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

/** Miniature rendue seulement quand elle devient visible (documents de plusieurs centaines de pages). */
const LazyThumb = memo(function LazyThumb({ doc, page, rev }: { doc: number; page: number; rev: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [visible, setVisible] = useState(false);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && setVisible(true), { rootMargin: "300px" });
    io.observe(el);
    return () => io.disconnect();
  }, []);
  useEffect(() => {
    if (!visible) return;
    let alive = true;
    const el = ref.current;
    const w = Math.round(Math.min(360, (el?.parentElement?.clientWidth ?? 140) * (window.devicePixelRatio || 1)) / 16) * 16 || 160;
    void renderPage({ doc, page, width: w, rot: 0, annots: true, forms: true, prio: Prio.THUMB, gen: 0, rev }).then((res) => {
      if (!alive || res.kind !== "ok" || !ref.current) return;
      ref.current.width = res.bitmap.width;
      ref.current.height = res.bitmap.height;
      ref.current.getContext("2d")?.drawImage(res.bitmap, 0, 0);
      setReady(true);
    });
    return () => {
      alive = false;
    };
  }, [visible, doc, page, rev]);
  return (
    <>
      {!ready && <span className="skeleton" style={{ position: "absolute", inset: 0 }} />}
      <canvas ref={ref} className="ecard__canvas" style={{ opacity: ready ? 1 : 0 }} />
    </>
  );
});
