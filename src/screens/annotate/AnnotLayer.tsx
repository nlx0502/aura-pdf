import { memo, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ColorDots, Icons } from "../../components";
import type { Rect, TextWord } from "../../lib/api";
import {
  arrowHead,
  blankAnnot,
  bounds,
  COLORS,
  FONT_CSS,
  layoutText,
  LINE_HEIGHT,
  MARKUP_TOOLS,
  TEXT_PAD,
  textBoxHeight,
  TEXT_COLORS,
  nearestWord,
  quoteFor,
  rgba,
  simplify,
  translate,
  wordRects,
  type Annot,
} from "../../lib/annot";
import { pageWords } from "../../lib/pagedata";
import { useAnnots, useDocAnnots } from "../../stores/annot";
import { useApp } from "../../stores/app";

interface Props {
  tabKey: string;
  docId: number;
  page: number;
  /** Taille de la page en points (sans rotation d'affichage). */
  size: { w: number; h: number };
  scale: number;
  /** Module Annoter actif : outils et sélection disponibles. */
  interactive: boolean;
}

type Draft =
  | { kind: "markup"; from: number; to: number }
  | { kind: "box"; x0: number; y0: number; x1: number; y1: number }
  | { kind: "pen"; pts: [number, number][] }
  | { kind: "move"; id: string; x0: number; y0: number; dx: number; dy: number };

/** Annotations d'une page : dessin, sélection, création et déplacement. */
export const AnnotLayer = memo(function AnnotLayer({ tabKey, docId, page, size, scale, interactive }: Props) {
  const { t } = useTranslation();
  const d = useDocAnnots(tabKey);
  const tool = useAnnots((s) => s.tool);
  const style = useAnnots((s) => s.style);
  const hlOpacity = useAnnots((s) => s.highlightOpacity);
  const editing = useAnnots((s) => s.editing);
  const svgRef = useRef<SVGSVGElement>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [words, setWords] = useState<TextWord[]>([]);
  // Annotations créées à l'instant : animées une seule fois.
  const fresh = useRef(new Set<string>());

  useEffect(() => {
    if (interactive) void pageWords(docId, page).then(setWords);
  }, [interactive, docId, page]);

  if (!d?.loaded) return null;
  const list = d.list.filter((a) => a.page === page);
  const store = useAnnots.getState();
  const author = () => useApp.getState().prefs.userName.trim() || useApp.getState().systemName || "Vous";

  // ---- Coordonnées : écran → points de page (transformations CSS comprises).
  const toPt = (e: React.PointerEvent): [number, number] => {
    const svg = svgRef.current!;
    const m = svg.getScreenCTM()?.inverse();
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m);
    return [Math.max(0, Math.min(size.w, p.x)), Math.max(0, Math.min(size.h, p.y))];
  };

  const create = (a: Annot) => {
    fresh.current.add(a.id);
    window.setTimeout(() => fresh.current.delete(a.id), 1500);
    store.commit(tabKey, (l) => [...l, a], a.id);
  };

  const onDown = (e: React.PointerEvent) => {
    if (!interactive || e.button !== 0) return;
    const [x, y] = toPt(e);
    (e.target as Element).setPointerCapture?.(e.pointerId);
    if (MARKUP_TOOLS.includes(tool)) {
      const i = nearestWord(words, x, y);
      if (i >= 0) setDraft({ kind: "markup", from: i, to: i });
    } else if (tool === "pen") {
      setDraft({ kind: "pen", pts: [[x, y]] });
    } else if (tool === "note") {
      e.preventDefault();
      const a = blankAnnot({ page, kind: "note", color: style.color, rect: { x: x - 11, y: y - 11, w: 22, h: 22 }, author: author() });
      create(a);
      useAnnots.setState({ tool: "select" });
      window.dispatchEvent(new CustomEvent("aura:comment", { detail: a.id }));
    } else if (tool === "text") {
      // Sinon le « mousedown » qui suit redonne le focus au visualiseur et ferme la saisie.
      e.preventDefault();
      const ts = useAnnots.getState().textStyle;
      const h = Math.ceil(2 * TEXT_PAD + ts.fontSize * LINE_HEIGHT);
      const a = blankAnnot({
        page,
        kind: "text",
        color: ts.color,
        font: ts.font,
        fontSize: ts.fontSize,
        bold: ts.bold,
        italic: ts.italic,
        align: ts.align,
        fill: ts.fill,
        borderColor: ts.borderColor,
        width: ts.borderColor ? Math.max(1, style.width) : 0,
        rect: { x, y: y - h / 2, w: 200, h },
        author: author(),
      });
      create(a);
      store.setEditing(a.id);
      useAnnots.setState({ tool: "select" });
    } else if (tool === "select") {
      store.select(tabKey, null);
    } else {
      setDraft({ kind: "box", x0: x, y0: y, x1: x, y1: y });
    }
  };

  const onMove = (e: React.PointerEvent) => {
    if (!draft) return;
    const [x, y] = toPt(e);
    if (draft.kind === "markup") {
      const i = nearestWord(words, x, y);
      if (i >= 0 && i !== draft.to) setDraft({ ...draft, to: i });
    } else if (draft.kind === "pen") {
      const last = draft.pts[draft.pts.length - 1];
      if (Math.hypot(x - last[0], y - last[1]) > 1.2) setDraft({ kind: "pen", pts: [...draft.pts, [x, y]] });
    } else if (draft.kind === "box") {
      let [x1, y1] = [x, y];
      // Maj : carré, cercle ou angle multiple de 45°.
      if (e.shiftKey) {
        if (tool === "rect" || tool === "ellipse") {
          const s = Math.max(Math.abs(x - draft.x0), Math.abs(y - draft.y0));
          x1 = draft.x0 + Math.sign(x - draft.x0) * s;
          y1 = draft.y0 + Math.sign(y - draft.y0) * s;
        } else {
          const ang = Math.round(Math.atan2(y - draft.y0, x - draft.x0) / (Math.PI / 4)) * (Math.PI / 4);
          const len = Math.hypot(x - draft.x0, y - draft.y0);
          x1 = draft.x0 + Math.cos(ang) * len;
          y1 = draft.y0 + Math.sin(ang) * len;
        }
      }
      setDraft({ ...draft, x1, y1 });
    } else if (draft.kind === "move") {
      setDraft({ ...draft, dx: x - draft.x0, dy: y - draft.y0 });
    }
  };

  const onUp = () => {
    if (!draft) return;
    setDraft(null);
    if (draft.kind === "markup") {
      const rects = wordRects(words, draft.from, draft.to);
      if (!rects.length) return;
      const kind = tool === "underline" ? "underline" : tool === "strike" ? "strike" : "highlight";
      create(
        blankAnnot({
          page,
          kind,
          color: style.color,
          opacity: kind === "highlight" ? hlOpacity : 1,
          rects: rects.map((r) => ({ ...r, y: r.y - 1, h: r.h + 2 })),
          text: quoteFor(words, rects),
          author: author(),
        }),
      );
    } else if (draft.kind === "pen") {
      if (draft.pts.length < 2) return;
      create(blankAnnot({ page, kind: "ink", color: style.color, width: style.width, opacity: style.opacity, ink: [simplify(draft.pts)], author: author() }));
    } else if (draft.kind === "box") {
      const { x0, y0, x1, y1 } = draft;
      if (Math.hypot(x1 - x0, y1 - y0) < 4) return;
      if (tool === "arrow" || tool === "line") {
        create(blankAnnot({ page, kind: tool, color: style.color, width: Math.max(style.width, 1.5), opacity: style.opacity, line: [x0, y0, x1, y1], author: author() }));
      } else {
        const rect = { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
        create(blankAnnot({ page, kind: tool === "ellipse" ? "ellipse" : "rect", color: style.color, width: style.width, opacity: style.opacity, rect, author: author() }));
      }
    } else if (draft.kind === "move") {
      if (Math.abs(draft.dx) + Math.abs(draft.dy) < 0.5) return;
      store.commit(tabKey, (l) => l.map((a) => (a.id === draft.id ? { ...a, ...translate(a, draft.dx, draft.dy), geomDirty: true } : a)));
    }
  };

  const startMove = (e: React.PointerEvent, a: Annot) => {
    if (!interactive || tool !== "select" || e.button !== 0) return;
    e.stopPropagation();
    store.select(tabKey, a.id);
    if (a.locked || editing === a.id) return;
    const [x, y] = toPt(e);
    (e.target as Element).setPointerCapture?.(e.pointerId);
    setDraft({ kind: "move", id: a.id, x0: x, y0: y, dx: 0, dy: 0 });
  };

  // Annotation affichée (déplacement en cours appliqué).
  const shown = (a: Annot): Annot => (draft?.kind === "move" && draft.id === a.id ? { ...a, ...translate(a, draft.dx, draft.dy), geomDirty: true } : a);
  const selected = list.find((a) => a.id === d.selected);
  const capture = interactive && tool !== "select";
  const W = size.w * scale;
  const H = size.h * scale;

  return (
    <div className="alayer" style={{ width: W, height: H }}>
      {/* Surlignages : calque à part, mélangé au papier en mode « multiply » comme dans le PDF. */}
      <svg className="alayer__hl" width={W} height={H} viewBox={`0 0 ${size.w} ${size.h}`} aria-hidden="true">
        {list.map((raw) => {
          const a = shown(raw);
          if (a.kind !== "highlight" || !(a.source === null || a.geomDirty)) return null;
          return (
            <g key={a.id} className={fresh.current.has(a.id) ? "ann--fresh" : ""} opacity={a.opacity}>
              {a.rects.map((r, i) => (
                <rect key={i} className="ann__sweep" style={{ animationDelay: `${i * 120}ms` }} x={r.x} y={r.y} width={r.w} height={r.h} rx={1.5} fill={a.color} />
              ))}
            </g>
          );
        })}
        {draft?.kind === "markup" &&
          wordRects(words, draft.from, draft.to).map((r, i) => <rect key={i} x={r.x} y={r.y} width={r.w} height={r.h} fill={rgba(style.color, 0.45)} rx={1.5} />)}
      </svg>
      <svg
        ref={svgRef}
        className={`alayer__svg ${capture ? "alayer__svg--capture" : ""} ${MARKUP_TOOLS.includes(tool) && capture ? "is-text" : ""}`}
        width={W}
        height={H}
        viewBox={`0 0 ${size.w} ${size.h}`}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={() => setDraft(null)}
      >
        {list.map((raw) => {
          const a = shown(raw);
          // Annotations d'origine intactes : dessinées par PDFium, ici zone cliquable seulement.
          const drawn = a.source === null || a.geomDirty;
          return (
            <g
              key={a.id}
              className={`ann ${drawn ? "" : "ann--ghost"} ${fresh.current.has(a.id) ? "ann--fresh" : ""} ${interactive && tool === "select" ? "ann--pick" : ""}`}
              onPointerDown={(e) => startMove(e, raw)}
              onDoubleClick={() => a.kind === "text" && !a.locked && store.setEditing(a.id)}
            >
              <Shape a={a} />
            </g>
          );
        })}
        {draft?.kind === "pen" && <polyline points={draft.pts.map((p) => p.join(",")).join(" ")} fill="none" stroke={style.color} strokeWidth={style.width} strokeLinecap="round" strokeLinejoin="round" />}
        {draft?.kind === "box" && <DraftBox tool={tool} d={draft} color={style.color} width={style.width} />}
        {selected && interactive && <SelectionFrame r={bounds(shown(selected))} />}
      </svg>

      {/* Zones de texte en cours de saisie */}
      {list
        .filter((a) => a.kind === "text" && editing === a.id)
        .map((a) => (
          <TextEditor key={a.id} a={a} scale={scale} tabKey={tabKey} />
        ))}

      {/* Barre d'actions flottante au-dessus de la sélection */}
      {selected && interactive && tool === "select" && !draft && editing !== selected.id && (
        <Hud a={selected} words={words} scale={scale} tabKey={tabKey} t={t} />
      )}
    </div>
  );
});

// ---------------------------------------------------------------- formes

function Shape({ a }: { a: Annot }) {
  const c = a.color;
  const w = a.width;
  const common = { pathLength: 1, className: "ann__stroke", fill: "none", stroke: c, strokeWidth: w, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, opacity: a.opacity };
  switch (a.kind) {
    case "highlight":
      // Dessiné dans le calque « multiply » ; ici, seulement la zone cliquable.
      return (
        <>
          {a.rects.map((r, i) => (
            <rect key={i} x={r.x} y={r.y} width={r.w} height={r.h} fill="transparent" />
          ))}
        </>
      );
    case "underline":
    case "strike":
    case "squiggly":
      return (
        <>
          {a.rects.map((r, i) => {
            const y = a.kind === "strike" ? r.y + r.h * 0.55 : r.y + r.h * 0.93;
            const lw = Math.max(1, r.h * 0.07);
            return (
              <g key={i}>
                <rect x={r.x} y={r.y} width={r.w} height={r.h} fill="transparent" />
                <line {...common} strokeWidth={lw} x1={r.x} y1={y} x2={r.x + r.w} y2={y} style={{ animationDelay: `${i * 120}ms` }} />
              </g>
            );
          })}
        </>
      );
    case "rect": {
      const r = a.rect!;
      return <rect {...common} x={r.x + w / 2} y={r.y + w / 2} width={Math.max(0, r.w - w)} height={Math.max(0, r.h - w)} rx={3} />;
    }
    case "ellipse": {
      const r = a.rect!;
      return <ellipse {...common} cx={r.x + r.w / 2} cy={r.y + r.h / 2} rx={Math.max(0, r.w / 2 - w / 2)} ry={Math.max(0, r.h / 2 - w / 2)} />;
    }
    case "line":
    case "arrow": {
      const l = a.line!;
      const head = a.kind === "arrow" ? arrowHead(l, w) : null;
      return (
        <>
          <line x1={l[0]} y1={l[1]} x2={l[2]} y2={l[3]} stroke="transparent" strokeWidth={Math.max(10, w + 8)} />
          <path {...common} d={`M${l[0]} ${l[1]}L${l[2]} ${l[3]}`} />
          {head && <path {...common} style={{ animationDelay: ".35s" }} d={`M${head[0].join(" ")}L${head[1].join(" ")}L${head[2].join(" ")}`} />}
        </>
      );
    }
    case "ink":
      return (
        <>
          {a.ink.map((s, i) => (
            <g key={i}>
              <polyline points={s.map((p) => p.join(",")).join(" ")} fill="none" stroke="transparent" strokeWidth={Math.max(10, w + 8)} />
              <polyline {...common} className="ann__stroke ann__stroke--slow" points={s.map((p) => p.join(",")).join(" ")} />
            </g>
          ))}
        </>
      );
    case "note": {
      const r = a.rect!;
      return (
        <g transform={`translate(${r.x} ${r.y})`} className="ann__pin">
          <circle cx={11} cy={11} r={11} fill={c} className="ann__ping" />
          <path d="M5 1h12a4 4 0 0 1 4 4v8a4 4 0 0 1-4 4h-6l-5 4v-4h-1a4 4 0 0 1-4-4V5a4 4 0 0 1 4-4z" fill={c} stroke="rgba(0,0,0,.35)" strokeWidth={0.8} />
          <path d="M6.5 6.5h9M6.5 10h9M6.5 13.5h5" stroke="rgba(0,0,0,.55)" strokeWidth={1.1} strokeLinecap="round" />
        </g>
      );
    }
    case "text": {
      const r = a.rect!;
      const border = a.borderColor && a.width > 0;
      const lines = layoutText(a, r.w);
      const anchor = a.align === 1 ? "middle" : a.align === 2 ? "end" : "start";
      const tx = a.align === 1 ? r.x + r.w / 2 : a.align === 2 ? r.x + r.w - TEXT_PAD : r.x + TEXT_PAD;
      return (
        <g opacity={a.opacity}>
          {/* Zone cliquable (le texte seul serait difficile à attraper). */}
          <rect x={r.x} y={r.y} width={r.w} height={r.h} fill={a.fill ?? "transparent"} rx={3} />
          {border && <rect x={r.x + a.width / 2} y={r.y + a.width / 2} width={r.w - a.width} height={r.h - a.width} rx={3} fill="none" stroke={a.borderColor!} strokeWidth={a.width} />}
          <text
            fill={c}
            textAnchor={anchor}
            style={{ fontFamily: FONT_CSS[a.font], fontSize: a.fontSize, fontWeight: a.bold ? 700 : 400, fontStyle: a.italic ? "italic" : "normal" }}
          >
            {lines.map((l, i) => (
              <tspan key={i} x={tx} y={r.y + TEXT_PAD + a.fontSize * 0.86 + i * a.fontSize * LINE_HEIGHT} xmlSpace="preserve">
                {l}
              </tspan>
            ))}
          </text>
        </g>
      );
    }
    case "stamp": {
      const r = a.rect!;
      return (
        <g className="ann__stamp" style={{ transformOrigin: `${r.x + r.w / 2}px ${r.y + r.h / 2}px` }}>
          <g transform={`rotate(-6 ${r.x + r.w / 2} ${r.y + r.h / 2})`}>
            <rect x={r.x + 1.25} y={r.y + 1.25} width={r.w - 2.5} height={r.h - 2.5} rx={6} fill="transparent" stroke={c} strokeWidth={2.5} />
            <text x={r.x + r.w / 2} y={r.y + r.h / 2 + 5} textAnchor="middle" fill={c} className="ann__stamptext">
              {a.text}
            </text>
          </g>
        </g>
      );
    }
  }
}

function DraftBox({ tool, d, color, width }: { tool: string; d: { x0: number; y0: number; x1: number; y1: number }; color: string; width: number }) {
  const common = { fill: "none", stroke: color, strokeWidth: width, strokeLinecap: "round" as const };
  if (tool === "arrow" || tool === "line") {
    const l: [number, number, number, number] = [d.x0, d.y0, d.x1, d.y1];
    const h = tool === "arrow" ? arrowHead(l, width) : null;
    return (
      <g {...common}>
        <path d={`M${d.x0} ${d.y0}L${d.x1} ${d.y1}`} />
        {h && <path d={`M${h[0].join(" ")}L${h[1].join(" ")}L${h[2].join(" ")}`} />}
      </g>
    );
  }
  const r = { x: Math.min(d.x0, d.x1), y: Math.min(d.y0, d.y1), w: Math.abs(d.x1 - d.x0), h: Math.abs(d.y1 - d.y0) };
  return tool === "ellipse" ? <ellipse {...common} cx={r.x + r.w / 2} cy={r.y + r.h / 2} rx={r.w / 2} ry={r.h / 2} /> : <rect {...common} x={r.x} y={r.y} width={r.w} height={r.h} rx={3} />;
}

function SelectionFrame({ r }: { r: Rect }) {
  const pad = 4;
  return <rect className="ann__sel" x={r.x - pad} y={r.y - pad} width={r.w + pad * 2} height={r.h + pad * 2} rx={4} />;
}

// ---------------------------------------------------------------- saisie d'une zone de texte

function TextEditor({ a, scale, tabKey }: { a: Annot; scale: number; tabKey: string }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [value, setValue] = useState(a.text);
  const r = a.rect!;
  useEffect(() => {
    // Après la fin du clic qui a créé la zone.
    const t = window.setTimeout(() => ref.current?.focus(), 0);
    return () => window.clearTimeout(t);
  }, []);
  const finish = () => {
    const store = useAnnots.getState();
    store.setEditing(null);
    const text = value.trim();
    if (!text) {
      store.commit(tabKey, (l) => l.filter((x) => x.id !== a.id), null);
      return;
    }
    // Hauteur ajustée aux lignes, calculées comme dans l'apparence PDF.
    const h = textBoxHeight({ ...a, text }, r.w);
    if (text !== a.text || h !== r.h) store.commit(tabKey, (l) => l.map((x) => (x.id === a.id ? { ...x, text, rect: { ...r, h }, geomDirty: true } : x)));
  };
  const rows = textBoxHeight({ ...a, text: value || " " }, r.w);
  return (
    <textarea
      ref={ref}
      className="ann__editor"
      value={value}
      style={{
        left: r.x * scale,
        top: r.y * scale,
        width: r.w * scale,
        height: Math.max(r.h, rows) * scale,
        padding: TEXT_PAD * scale,
        fontSize: a.fontSize * scale,
        lineHeight: LINE_HEIGHT,
        fontFamily: FONT_CSS[a.font],
        fontWeight: a.bold ? 700 : 400,
        fontStyle: a.italic ? "italic" : "normal",
        textAlign: (["left", "center", "right"] as const)[a.align] ?? "left",
        color: a.color,
        background: a.fill ?? "transparent",
        borderColor: a.borderColor && a.width > 0 ? a.borderColor : "transparent",
        borderWidth: a.borderColor && a.width > 0 ? a.width * scale : 0,
      }}
      onChange={(e) => setValue(e.target.value)}
      onBlur={finish}
      onKeyDown={(e) => {
        if (e.key === "Escape") (e.target as HTMLTextAreaElement).blur();
        e.stopPropagation();
      }}
      onPointerDown={(e) => e.stopPropagation()}
    />
  );
}

// ---------------------------------------------------------------- barre d'actions flottante

function Hud({ a, words, scale, tabKey, t }: { a: Annot; words: TextWord[]; scale: number; tabKey: string; t: (k: string) => string }) {
  const b = bounds(a);
  const store = useAnnots.getState();
  const quote = a.text || quoteFor(words, a.rects);
  const recolor = (hex: string) => store.commit(tabKey, (l) => l.map((x) => (x.id === a.id ? { ...x, color: hex, geomDirty: true } : x)));
  const top = Math.max(-46, b.y * scale - 50);
  return (
    <div className="hud" style={{ left: Math.max(0, b.x * scale), top }} role="toolbar" aria-label={t("annot.hudLabel")} onPointerDown={(e) => e.stopPropagation()}>
      {!a.locked && (
        <span style={{ padding: "0 4px" }}>
          <ColorDots size="sm" presets={a.kind === "text" ? TEXT_COLORS : COLORS} value={a.color} label={t("annot.recolor")} onChange={(h) => h && recolor(h)} />
        </span>
      )}
      {!a.locked && <span className="hud__sep" />}
      <button type="button" className="btn btn--ghost btn--sm" onClick={() => window.dispatchEvent(new CustomEvent("aura:comment", { detail: a.id }))}>
        <Icons.annotate />
        {t("annot.comment")}
      </button>
      {quote && (
        <button type="button" className="btn btn--ghost btn--sm" onClick={() => void navigator.clipboard?.writeText(quote)}>
          {t("annot.copy")}
        </button>
      )}
      <button type="button" className="btn btn--ghost btn--sm hud__danger" onClick={() => store.commit(tabKey, (l) => l.filter((x) => x.id !== a.id), null)}>
        {t("annot.remove")}
      </button>
    </div>
  );
}
