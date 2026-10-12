import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Chip, IconButton, Icons } from "../../components";
import type { Rect } from "../../lib/api";
import { fontCss } from "../../lib/annot";
import { editApi, rectFrom, type ContentItem, type ContentOp, type TextSpec } from "../../lib/edit";
import { pickImage } from "../../lib/files";
import { localImageUrl, Prio, renderPage } from "../../lib/render";
import type { DocTab } from "../../stores/app";
import { useEdit, type DocEdit } from "../../stores/edit";
import { resolveItem, useContentSel, type ContentTool } from "./contentSel";
import { Overlays } from "./PagesGrid";

export interface LinkView {
  id: string;
  rect: Rect;
  uri: string | null;
  page: number | null;
}

/** Liens affichés d'une page : ceux du fichier, modifiés, plus les nouveaux. */
export function linkList(d: DocEdit, page: number): LinkView[] {
  const edits = d.links[page] ?? [];
  const out: LinkView[] = [];
  for (const l of d.origLinks[page] ?? []) {
    if (edits.some((e) => e.op === "delete" && e.index === l.index)) continue;
    const upd = [...edits].reverse().find((e) => e.op === "update" && e.index === l.index);
    out.push({ id: `o${l.index}`, rect: l.rect, uri: upd && upd.op === "update" ? upd.uri : l.uri, page: upd ? null : l.page });
  }
  edits.forEach((e, i) => {
    if (e.op === "add") out.push({ id: `a${i}`, rect: e.rect, uri: e.uri, page: null });
  });
  return out;
}

/** Description d'un bloc existant, prête à être modifiée. */
export function specOf(it: ContentItem): TextSpec {
  return {
    text: it.text,
    // Légère marge : la police de remplacement peut être un peu plus large que l'originale.
    rect: { ...it.rect, w: it.rect.w * 1.03 },
    font: it.font,
    bold: it.bold,
    italic: it.italic,
    size: it.size,
    color: it.color,
    align: it.align,
    lineHeight: it.lineHeight,
    baseline: it.baseline,
  };
}

const TOOLS: { id: ContentTool; label: string }[] = [
  { id: "text", label: "edit.toolText" },
  { id: "image", label: "edit.toolImage" },
  { id: "link", label: "edit.toolLink" },
  { id: "redact", label: "edit.toolRedact" },
];

type Gesture =
  | { kind: "draw"; x0: number; y0: number; x: number; y: number }
  | { kind: "move"; dx: number; dy: number }
  | { kind: "resize"; rect: Rect };

export function ContentView({ tab, d, readOnly }: { tab: DocTab; d: DocEdit; readOnly: boolean }) {
  const { t } = useTranslation();
  const store = useEdit();
  const cs = useContentSel();
  const { tool, sel, items, editing, crop } = cs;
  const info = tab.info;
  const cp = Math.min(d.contentPage, info.pageCount - 1);
  const size = info.pages[cp];
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  const [avail, setAvail] = useState(700);
  const [gesture, setGesture] = useState<Gesture | null>(null);

  // Ordre de navigation : pages d'origine dans l'ordre du plan.
  const origOrder = d.pages.filter((p) => p.src.kind === "orig").map((p) => (p.src.kind === "orig" ? p.src.index : 0));
  const pos = origOrder.indexOf(cp);
  const editPage = d.pages.find((p) => p.src.kind === "orig" && p.src.index === cp);
  const planIndex = editPage ? d.pages.indexOf(editPage) : -1;

  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setAvail(el.clientWidth));
    ro.observe(el);
    setAvail(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const W = Math.max(280, Math.min(680, avail - 48));
  const scale = W / size.w;
  const H = size.h * scale;

  // ---- Rendu de la page
  useEffect(() => {
    let alive = true;
    const dpr = window.devicePixelRatio || 1;
    void renderPage({ doc: info.id, page: cp, width: Math.round(W * dpr), rot: 0, annots: true, forms: true, prio: Prio.VISIBLE, gen: 2 ** 50, rev: d.rev }).then((res) => {
      const c = canvasRef.current;
      if (!alive || res.kind !== "ok" || !c) return;
      c.width = res.bitmap.width;
      c.height = res.bitmap.height;
      c.getContext("2d")?.drawImage(res.bitmap, 0, 0);
    });
    return () => {
      alive = false;
    };
  }, [info.id, cp, W, d.rev]);

  // ---- Éléments de la page (relus après chaque modification)
  useEffect(() => {
    let alive = true;
    void editApi
      .contentList(info.id, cp)
      .then((list) => alive && useContentSel.getState().set({ items: list }))
      .catch(() => alive && useContentSel.getState().set({ items: [] }));
    return () => {
      alive = false;
    };
  }, [info.id, cp, d.rev]);
  useEffect(() => () => useContentSel.getState().clear(), [cp]);

  const item = resolveItem(items, sel);
  const links = linkList(d, cp);
  const apply = (op: ContentOp) => store.applyContent(tab.key, op);
  const toPt = (e: { clientX: number; clientY: number }) => {
    const r = layerRef.current!.getBoundingClientRect();
    return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
  };
  const go = (dir: -1 | 1) => {
    const next = origOrder[pos + dir];
    if (next !== undefined) store.patch(tab.key, { contentPage: next });
  };

  // ---- Création (tracé dans le vide)
  const onLayerDown = (e: React.PointerEvent) => {
    if (readOnly || e.button !== 0 || e.target !== layerRef.current) return;
    if (crop) return;
    if (sel && tool !== "redact") {
      // Premier clic dans le vide : on désélectionne seulement.
      cs.set({ sel: null, editing: false });
      return;
    }
    const p0 = toPt(e);
    let g: Gesture = { kind: "draw", x0: p0.x, y0: p0.y, x: p0.x, y: p0.y };
    setGesture(g);
    const move = (ev: PointerEvent) => {
      const p = toPt(ev);
      g = { kind: "draw", x0: p0.x, y0: p0.y, x: p.x, y: p.y };
      setGesture(g);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setGesture(null);
      if (g.kind !== "draw") return;
      const r = rectFrom(g.x0, g.y0, g.x, g.y);
      const dragged = r.w > 6 && r.h > 4;
      void create(r, dragged, p0);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const create = async (r: Rect, dragged: boolean, p0: { x: number; y: number }) => {
    if (tool === "redact") {
      if (dragged) store.addRedact(tab.key, cp, [r]);
    } else if (tool === "link") {
      if (!dragged) return;
      const count = (d.links[cp] ?? []).length;
      store.editLinks(tab.key, cp, (l) => [...l, { op: "add", rect: r, uri: "" }]);
      cs.set({ sel: { kind: "link", id: `a${count}` } });
    } else if (tool === "text") {
      const rect = dragged ? r : { x: p0.x, y: p0.y, w: Math.min(260, size.w - p0.x - 20), h: 16 };
      const spec: TextSpec = { text: t("edit.newText"), rect, font: "sans", bold: false, italic: false, size: 12, color: "#1A1B1E", align: 0, lineHeight: 1.2 };
      if (await apply({ op: "addText", page: cp, spec })) cs.set({ sel: { kind: "text", rect: { ...rect, h: 14 } }, editing: true });
    } else if (tool === "image") {
      const path = await pickImage(t("edit.pickImage"));
      if (!path) return;
      const dims = await imageSize(path);
      const w = dragged ? r.w : Math.min(200, size.w - p0.x - 10);
      const h = dragged ? r.h : (w * dims.h) / Math.max(1, dims.w);
      const rect = { x: dragged ? r.x : p0.x, y: dragged ? r.y : p0.y, w, h };
      if (await apply({ op: "addImage", page: cp, rect, path })) cs.set({ sel: { kind: "image", rect } });
    }
  };

  // ---- Déplacement / redimensionnement d'un élément sélectionné
  const onItemDown = (e: React.PointerEvent, it: ContentItem) => {
    if (e.button !== 0 || readOnly) return;
    e.stopPropagation();
    if (tool === "redact") {
      store.addRedact(tab.key, cp, [{ x: it.rect.x - 1, y: it.rect.y - 1, w: it.rect.w + 2, h: it.rect.h + 2 }]);
      return;
    }
    const isSel = item && item.ids.join() === it.ids.join();
    if (!isSel) {
      cs.set({ sel: { kind: it.kind, rect: it.rect }, editing: false, crop: null });
      return;
    }
    if (editing) return;
    const p0 = toPt(e);
    let g: Gesture = { kind: "move", dx: 0, dy: 0 };
    const move = (ev: PointerEvent) => {
      const p = toPt(ev);
      g = { kind: "move", dx: p.x - p0.x, dy: p.y - p0.y };
      setGesture(g);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setGesture(null);
      if (g.kind === "move" && Math.hypot(g.dx, g.dy) * scale > 2) {
        const { dx, dy } = g;
        void apply({ op: "move", page: cp, ids: it.ids, dx, dy }).then((ok) => ok && cs.set({ sel: { kind: it.kind, rect: { ...it.rect, x: it.rect.x + dx, y: it.rect.y + dy } } }));
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const onHandleDown = (e: React.PointerEvent, it: ContentItem, corner: number, cropMode: boolean) => {
    e.stopPropagation();
    e.preventDefault();
    const start = cropMode && crop ? crop : it.rect;
    const fixed = { x: corner % 2 === 0 ? start.x + start.w : start.x, y: corner < 2 ? start.y + start.h : start.y };
    const bound = it.rect;
    let next = start;
    const move = (ev: PointerEvent) => {
      const p = toPt(ev);
      let x = p.x;
      let y = p.y;
      if (cropMode) {
        x = Math.max(bound.x, Math.min(bound.x + bound.w, x));
        y = Math.max(bound.y, Math.min(bound.y + bound.h, y));
      } else if (!ev.shiftKey) {
        // Proportions conservées (Maj pour déformer).
        const ratio = start.w / Math.max(0.01, start.h);
        const w = Math.abs(x - fixed.x);
        y = fixed.y + Math.sign(y - fixed.y || 1) * (w / ratio);
      }
      next = rectFrom(fixed.x, fixed.y, x, y);
      if (cropMode) cs.set({ crop: next });
      else setGesture({ kind: "resize", rect: next });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (cropMode) return;
      setGesture(null);
      if (next.w > 4 && next.h > 4 && next !== start) {
        void apply({ op: "imageResize", page: cp, id: it.ids[0], rect: next }).then((ok) => ok && cs.set({ sel: { kind: "image", rect: next } }));
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  // ---- Raccourcis : Suppr supprime l'élément, Échap désélectionne.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (el.closest("input, textarea, [contenteditable]") || document.querySelector(".scrim")) return;
      const s = useContentSel.getState();
      const it = resolveItem(s.items, s.sel);
      if (e.key === "Escape") {
        s.set({ sel: null, editing: false, crop: null });
      } else if ((e.key === "Delete" || e.key === "Backspace") && !readOnly) {
        if (it) {
          e.preventDefault();
          void useEdit.getState().applyContent(tab.key, { op: "delete", page: cp, ids: it.ids });
          s.set({ sel: null });
        }
      } else if (e.key === "Enter" && it?.kind === "text" && !readOnly) {
        e.preventDefault();
        s.set({ editing: true });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tab.key, cp, readOnly]);

  const visibleItems = tool === "image" ? items.filter((i) => i.kind === "image") : tool === "link" ? [] : items.filter((i) => i.kind === "text");
  const draft = gesture?.kind === "draw" ? rectFrom(gesture.x0, gesture.y0, gesture.x, gesture.y) : null;
  const S = (r: Rect): React.CSSProperties => ({ left: r.x * scale, top: r.y * scale, width: r.w * scale, height: r.h * scale });
  const hint = { text: "edit.hintText", image: "edit.hintImage", link: "edit.hintLink", redact: "edit.hintRedact" }[tool];
  const marks = editPage?.redact ?? [];

  return (
    <div className="cview rise" ref={wrapRef}>
      <div className="cview__bar">
        <div className="cview__tools" role="toolbar" aria-label={t("edit.tools")}>
          {TOOLS.map((x) => (
            <Chip key={x.id} pressed={tool === x.id} className={x.id === "redact" ? "chip--danger" : undefined} onClick={() => cs.set({ tool: x.id, sel: null, editing: false, crop: null })}>
              {t(x.label)}
            </Chip>
          ))}
        </div>
        <div className="cview__nav">
          <IconButton label={t("edit.prevPage")} icon={<Icons.chevronLeft />} disabled={pos <= 0} onClick={() => go(-1)} />
          <span className="tnum muted">{t("edit.pageOf", { n: planIndex + 1, total: d.pages.length })}</span>
          <IconButton label={t("edit.nextPage")} icon={<Icons.chevronRight />} disabled={pos < 0 || pos >= origOrder.length - 1} onClick={() => go(1)} />
        </div>
      </div>
      <p className="cview__hint muted">
        {t(hint)}
        {marks.length > 0 && <span className="cview__marks"> · {t("edit.markCount", { count: marks.length })}</span>}
      </p>
      <article className="cview__page" style={{ width: W, height: H }}>
        <canvas ref={canvasRef} className="cview__canvas" />
        {editPage && (
          <span className="cview__over" aria-hidden="true">
            <Overlays settings={d.settings} n={planIndex + 1} total={d.pages.length} w={size.w} h={size.h} crop={false} />
          </span>
        )}
        <div ref={layerRef} className={`cview__layer cview__layer--${tool}`} onPointerDown={onLayerDown}>
          {visibleItems.map((it) => {
            const selected = !!item && item.ids.join() === it.ids.join();
            const moving = selected && gesture?.kind === "move";
            const resizing = selected && gesture?.kind === "resize";
            const r = resizing ? gesture.rect : it.rect;
            return (
              <div
                key={it.ids.join("-")}
                className={`cobj ${selected ? "is-sel" : ""}`}
                style={{ ...S({ x: r.x - 2, y: r.y - 2, w: r.w + 4, h: r.h + 4 }), transform: moving ? `translate(${gesture.dx * scale}px, ${gesture.dy * scale}px)` : undefined }}
                onPointerDown={(e) => onItemDown(e, it)}
                onDoubleClick={() => it.kind === "text" && !readOnly && cs.set({ sel: { kind: "text", rect: it.rect }, editing: true })}
                role="button"
                aria-pressed={selected}
                aria-label={it.kind === "text" ? it.text.slice(0, 80) : t("edit.image")}
                tabIndex={0}
              >
                {selected && !editing && !crop && [0, 1, 2, 3].map((c) => (
                  <span key={c} className={`chandle chandle--${c}`} onPointerDown={it.kind === "image" ? (e) => onHandleDown(e, it, c, false) : undefined} />
                ))}
              </div>
            );
          })}
          {tool === "link" &&
            links.map((l) => (
              <button
                key={l.id}
                type="button"
                className={`clink ${sel?.kind === "link" && sel.id === l.id ? "is-sel" : ""}`}
                style={S(l.rect)}
                aria-label={l.uri ?? t("edit.linkTarget", { n: (l.page ?? 0) + 1 })}
                title={l.uri ?? t("edit.linkTarget", { n: (l.page ?? 0) + 1 })}
                onPointerDown={(e) => e.stopPropagation()}
                onClick={() => cs.set({ sel: { kind: "link", id: l.id } })}
              />
            ))}
          {marks.map((m, i) => (
            <span key={i} className="cmark" style={S(m)}>
              {!readOnly && editPage && (
                <button
                  type="button"
                  className="cmark__x"
                  aria-label={t("edit.removeMark")}
                  title={t("edit.removeMark")}
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={() => store.clearRedact(tab.key, editPage.key, i)}
                >
                  <Icons.close />
                </button>
              )}
            </span>
          ))}
          {draft && <span className={`cdraft cdraft--${tool}`} style={S(draft)} />}
          {crop && item?.kind === "image" && (
            <>
              <span className="ccrop" style={S(crop)}>
                {[0, 1, 2, 3].map((c) => (
                  <span key={c} className={`chandle chandle--${c}`} onPointerDown={(e) => onHandleDown(e, item, c, true)} />
                ))}
              </span>
              <span className="ccrop__bar" style={{ left: crop.x * scale, top: (crop.y + crop.h) * scale + 10 }}>
                <Button size="sm" variant="primary" onClick={() => void apply({ op: "imageCrop", page: cp, id: item.ids[0], rect: crop }).then(() => cs.set({ crop: null, sel: { kind: "image", rect: crop } }))}>
                  {t("edit.cropApply")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => cs.set({ crop: null })}>
                  {t("edit.cropCancel")}
                </Button>
              </span>
            </>
          )}
          {editing && item?.kind === "text" && <InlineEditor key={item.ids.join()} item={item} scale={scale} onDone={(text) => {
            cs.set({ editing: false });
            if (text !== null && text !== item.text) void apply({ op: "replaceText", page: cp, ids: item.ids, spec: { ...specOf(item), text } });
          }} />}
        </div>
      </article>
    </div>
  );
}

/** Saisie directe sur la page, avec la police, la taille et la couleur du bloc. */
function InlineEditor({ item, scale, onDone }: { item: ContentItem; scale: number; onDone: (text: string | null) => void }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [text, setText] = useState(item.text);
  const done = useRef(false);
  useEffect(() => {
    const id = window.setTimeout(() => {
      ref.current?.focus();
      ref.current?.select();
    }, 30);
    return () => window.clearTimeout(id);
  }, []);
  const finish = (v: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(v);
  };
  const r = item.rect;
  return (
    <textarea
      ref={ref}
      className="cedit"
      value={text}
      spellCheck
      style={{
        left: (r.x - 3) * scale,
        top: (r.y - 3) * scale,
        width: (r.w * 1.03 + 6) * scale,
        minHeight: (r.h + 6) * scale,
        height: (Math.max(r.h, (text.split("\n").length + Math.floor(text.length / 90)) * item.size * item.lineHeight) + 6) * scale,
        fontFamily: fontCss(item.font),
        fontSize: item.size * scale,
        lineHeight: item.lineHeight,
        fontWeight: item.bold ? 700 : 400,
        fontStyle: item.italic ? "italic" : "normal",
        color: item.color,
        textAlign: (["left", "center", "right"] as const)[item.align] ?? "left",
        padding: 3 * scale,
      }}
      onChange={(e) => setText(e.target.value)}
      onPointerDown={(e) => e.stopPropagation()}
      onBlur={() => finish(text)}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          finish(null);
        } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
          e.preventDefault();
          finish(text);
        }
      }}
    />
  );
}

function imageSize(path: string): Promise<{ w: number; h: number }> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve({ w: img.naturalWidth, h: img.naturalHeight });
    img.onerror = () => resolve({ w: 4, h: 3 });
    img.src = localImageUrl(path);
  });
}
