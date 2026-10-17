import { memo, useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import type { PageLink, SearchHit, TextWord } from "../../lib/api";
import { bestCached, Prio, renderPage, type RenderRequest } from "../../lib/render";
import { measure100, pageLinks, pageWords } from "../../lib/pagedata";
import type { Box } from "../../lib/layout";

/** Au-delà de ce nombre de pixels physiques, la page est rendue par tuiles. */
const TILE_THRESHOLD = 16_000_000;
const TILE = 1024;

export interface PageViewProps {
  doc: number;
  page: number;
  box: Box;
  /** Taille de la page en points (rotation propre au PDF incluse, rotation d'affichage exclue). */
  size: { w: number; h: number };
  scale: number;
  rot: number;
  dark: boolean;
  annots: boolean;
  forms: boolean;
  /** Le zoom est stable (pas d'animation en cours). */
  settled: boolean;
  /** Partie visible de la page, en pixels CSS relatifs à son coin (pour les tuiles). */
  visible: Box | null;
  getGen: () => number;
  hits: SearchHit[];
  activeHit: SearchHit | null;
  onGoto: (page: number) => void;
  /** Révision des annotations : un changement force un nouveau rendu PDFium. */
  rev?: number;
  /** Couche supplémentaire (annotations), dans le repère de la page. */
  overlay?: React.ReactNode;
}

function sameRequestShape(a: RenderRequest | null, b: RenderRequest) {
  return !!a && a.doc === b.doc && a.page === b.page && a.width === b.width && a.rot === b.rot && a.annots === b.annots && a.forms === b.forms && (a.rev ?? 0) === (b.rev ?? 0);
}

/** Une page du document : bitmap (ou tuiles), surlignage de recherche, couche texte, liens. */
export const PageView = memo(function PageView(p: PageViewProps) {
  const { t } = useTranslation();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [hasBitmap, setHasBitmap] = useState(false);
  const [error, setError] = useState(false);
  const lastReq = useRef<RenderRequest | null>(null);

  const dpr = window.devicePixelRatio || 1;
  const pxW = Math.max(1, Math.round(p.box.w * dpr));
  const pxH = Math.max(1, Math.round(p.box.h * dpr));
  const tiled = pxW * pxH > TILE_THRESHOLD;
  // En mode tuiles, la page entière sert de fond flou à résolution réduite.
  const baseW = tiled ? Math.round(pxW * Math.sqrt(TILE_THRESHOLD / 4 / (pxW * pxH))) : pxW;

  // ---- Bitmap de la page entière (ou fond des tuiles)
  useEffect(() => {
    let alive = true;
    const canvas = canvasRef.current;
    const draw = (bmp: ImageBitmap) => {
      if (!alive || !canvas) return;
      if (canvas.width !== bmp.width || canvas.height !== bmp.height) {
        canvas.width = bmp.width;
        canvas.height = bmp.height;
      }
      canvas.getContext("2d")?.drawImage(bmp, 0, 0);
      setHasBitmap(true);
    };
    // Affichage immédiat d'une version déjà calculée (autre zoom), étirée.
    if (!hasBitmap) {
      const prev = bestCached(p.doc, p.page, p.rot, p.rev ?? 0) ?? bestCached(p.doc, p.page, p.rot, (p.rev ?? 1) - 1);
      if (prev) draw(prev);
    }
    const req: RenderRequest = { doc: p.doc, page: p.page, width: baseW, rot: p.rot, annots: p.annots, forms: p.forms, prio: Prio.VISIBLE, gen: 0, rev: p.rev ?? 0 };
    if (sameRequestShape(lastReq.current, req) && hasBitmap) return;
    // Pendant une animation de zoom, attendre la fin pour recalculer.
    if (!p.settled && hasBitmap) return;

    const timer = window.setTimeout(async () => {
      for (let attempt = 0; attempt < 30 && alive; attempt++) {
        const res = await renderPage({ ...req, gen: p.getGen() });
        if (!alive) return;
        if (res.kind === "ok") {
          lastReq.current = req;
          setError(false);
          draw(res.bitmap);
          return;
        }
        if (res.kind === "error") {
          setError(true);
          return;
        }
        // Obsolète : la vue a changé ; on réessaie si la page est toujours affichée.
        await new Promise((r) => setTimeout(r, 60));
      }
    }, hasBitmap ? 70 : 0);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.doc, p.page, baseW, p.rot, p.annots, p.forms, p.settled, p.rev]);

  // ---- Tuiles (grands zooms)
  const tiles = useMemo(() => {
    if (!tiled || !p.visible || !p.settled) return [];
    const v = p.visible;
    const x0 = Math.max(0, Math.floor((v.x * dpr) / TILE) - 0);
    const y0 = Math.max(0, Math.floor((v.y * dpr) / TILE) - 0);
    const x1 = Math.min(Math.ceil(pxW / TILE), Math.ceil(((v.x + v.w) * dpr) / TILE));
    const y1 = Math.min(Math.ceil(pxH / TILE), Math.ceil(((v.y + v.h) * dpr) / TILE));
    const out: { x: number; y: number; w: number; h: number }[] = [];
    for (let ty = y0; ty < y1; ty++)
      for (let tx = x0; tx < x1; tx++) {
        const x = tx * TILE;
        const y = ty * TILE;
        out.push({ x, y, w: Math.min(TILE, pxW - x), h: Math.min(TILE, pxH - y) });
      }
    return out;
  }, [tiled, p.visible, p.settled, dpr, pxW, pxH]);

  // ---- Couche texte et liens (chargés quand la page est stable à l'écran)
  const [words, setWords] = useState<TextWord[] | null>(null);
  const [links, setLinks] = useState<PageLink[]>([]);
  useEffect(() => {
    if (!p.settled) return;
    let alive = true;
    const timer = window.setTimeout(() => {
      void pageWords(p.doc, p.page).then((w) => alive && setWords(w));
      void pageLinks(p.doc, p.page).then((l) => alive && setLinks(l));
    }, 120);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [p.doc, p.page, p.settled]);

  // Calque en coordonnées « sans rotation d'affichage », tourné en bloc.
  const s = p.scale;
  const W0 = p.size.w * s;
  const H0 = p.size.h * s;
  const r = ((p.rot % 4) + 4) % 4;
  // Sans rotation : aucune transformation, pour ne pas isoler les surlignages (mode multiply).
  const overlayTransform = [undefined,`translate(${H0}px,0) rotate(90deg)`, `translate(${W0}px,${H0}px) rotate(180deg)`, `translate(0,${W0}px) rotate(270deg)`][r];

  return (
    <div
      className={`page ${p.dark ? "page--dark" : ""} ${hasBitmap ? "" : "page--loading"}`}
      style={{ left: p.box.x, top: p.box.y, width: p.box.w, height: p.box.h }}
      data-page={p.page}
      aria-label={`${t("reader.pageWord")} ${p.page + 1}`}
      role="group"
    >
      <canvas ref={canvasRef} className="page__canvas" aria-hidden="true" />
      {tiles.map((tile) => (
        <Tile key={`${tile.x}:${tile.y}:${p.rev ?? 0}`} req={{ doc: p.doc, page: p.page, width: pxW, rot: p.rot, annots: p.annots, forms: p.forms, prio: Prio.VISIBLE, gen: 0, tile, rev: p.rev }} dpr={dpr} getGen={p.getGen} />
      ))}
      {error && <span className="page__error">{t("reader.renderError")}</span>}

      <div className="page__overlay" style={{ width: W0, height: H0, transform: overlayTransform }}>
        {p.hits.map((h, i) =>
          h.rects.map((rc, k) => (
            <span
              key={`${i}:${k}`}
              className={`hit ${p.activeHit === h ? "hit--active" : ""}`}
              style={{ left: rc.x * s - 1, top: rc.y * s - 1, width: rc.w * s + 2, height: rc.h * s + 2 }}
            />
          )),
        )}
        {words && p.settled && <TextLayer words={words} scale={s} />}
        {links.map((l, i) => (
          <a
            key={i}
            className="plink"
            href={l.uri ?? "#"}
            aria-label={l.uri ?? t("reader.linkToPage", { n: (l.page ?? 0) + 1 })}
            title={l.uri ?? undefined}
            style={{ left: l.rect.x * s, top: l.rect.y * s, width: l.rect.w * s, height: l.rect.h * s }}
            onClick={(e) => {
              e.preventDefault();
              if (l.page !== null) p.onGoto(l.page);
              else if (l.uri) void openUrl(l.uri);
            }}
          />
        ))}
        {p.overlay}
      </div>
    </div>
  );
});

function Tile({ req, dpr, getGen }: { req: RenderRequest; dpr: number; getGen: () => number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let alive = true;
    void (async () => {
      for (let attempt = 0; attempt < 30 && alive; attempt++) {
        const res = await renderPage({ ...req, gen: getGen() });
        if (!alive) return;
        if (res.kind === "ok") {
          const c = ref.current;
          if (!c) return;
          c.width = res.bitmap.width;
          c.height = res.bitmap.height;
          c.getContext("2d")?.drawImage(res.bitmap, 0, 0);
          setReady(true);
          return;
        }
        if (res.kind === "error") return;
        await new Promise((r) => setTimeout(r, 60));
      }
    })();
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const t = req.tile!;
  return (
    <canvas
      ref={ref}
      className="page__tile"
      aria-hidden="true"
      style={{ left: t.x / dpr, top: t.y / dpr, width: t.w / dpr, height: t.h / dpr, opacity: ready ? 1 : 0 }}
    />
  );
}

/** Couche de texte transparente, alignée sur le rendu : sélection et copie. */
const TextLayer = memo(function TextLayer({ words, scale }: { words: TextWord[]; scale: number }) {
  return (
    <div className="textlayer">
      {words.map((w, i) => {
        const fontSize = Math.max(1, w.h * scale * 0.86);
        const natural = (measure100(w.t) * fontSize) / 100;
        const sx = natural > 0 ? (w.w * scale) / natural : 1;
        return (
          <span
            key={i}
            style={{
              left: w.x * scale,
              top: w.y * scale,
              fontSize,
              lineHeight: `${w.h * scale}px`,
              transform: `scaleX(${sx})`,
            }}
          >
            {w.t}
            {w.sp ? " " : ""}
            {w.nl ? "\n" : ""}
          </span>
        );
      })}
    </div>
  );
});
