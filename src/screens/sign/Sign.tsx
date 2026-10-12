import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Button, ColorDots, Icons, Modal, ModalHeader, Segmented, StepList, Switch, TextField, type StepItem } from "../../components";
import type { Rect } from "../../lib/api";
import { pickImage } from "../../lib/files";
import { localImageUrl } from "../../lib/render";
import { INKS, SIG_FONTS, signApi, strokePath, strokesBounds, summary, type SigReport, type Visual } from "../../lib/sign";
import { useActiveTab, useApp, type DocTab } from "../../stores/app";
import { useSign, type Placement, type SigMode } from "../../stores/sign";
import { Viewer } from "../reader/Viewer";
import "./sign.css";

const D_OK = "M3.5 8.5l3 3 6-7";
const D_PEN = "M2 11.5c2-3 3.5-7 5-7s-1 7 1 7 2.5-3 3.5-3 1 2 2.5 2";
const D_WARN = "M8 2.5l6 11H2z M8 7v3 M8 12h.01";
const D_X = "M4.5 4.5l7 7 M11.5 4.5l-7 7";
/** Signature d'exemple qui se dessine dans le pad vide (maquette). */
const DEMO = "M28 104 C 40 60, 52 30, 62 34 C 72 40, 50 100, 60 104 C 72 108, 82 64, 92 66 C 100 68, 94 98, 104 98 C 116 98, 120 70, 130 72 C 138 74, 134 96, 144 96 C 158 96, 166 60, 176 62 C 186 64, 178 100, 192 98 C 210 94, 230 80, 262 84";

const Path = ({ d, style }: { d: string; style?: React.CSSProperties }) => (
  <svg className="i" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={style}>
    <path d={d} />
  </svg>
);

/** Module Signer (maquette Sign.dc.html). */
export function Sign() {
  const tab = useActiveTab();
  const loadReport = useSign((s) => s.loadReport);
  useEffect(() => {
    void useSign.getState().init();
  }, []);
  useEffect(() => {
    if (tab) void loadReport(tab.key, tab.info.path);
  }, [tab?.key, tab?.info.id, tab?.info.fileSize]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!tab) return <div className="workspace" />;
  return <SignInner tab={tab} />;
}

function SignInner({ tab }: { tab: DocTab }) {
  const report = useSign((s) => s.reports[tab.key] ?? null);
  return (
    <div className="workspace signw">
      <main className="panel signw__main">
        <Banner tab={tab} />
        <div className="signw__viewer">
          <Viewer key={tab.key} tab={tab} active overlay={(page, scale) => <SigLayer tab={tab} page={page} scale={scale} report={report} />} />
        </div>
      </main>
      <SignSide tab={tab} />
    </div>
  );
}

// ---------------------------------------------------------------- bannière d'état

function Banner({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const report = useSign((s) => s.reports[tab.key] ?? null);
  const verifying = useSign((s) => s.verifying);
  const sum = summary(report);
  let tone: "accent" | "ok" | "warn" | "bad" = "accent";
  let icon = D_PEN;
  let title = t("sign.bannerNone");
  let sub = t("sign.bannerNoneSub");
  if (tab.info.encrypted) {
    tone = "warn";
    icon = D_WARN;
    title = t("sign.locked");
    sub = t("sign.lockedSub");
  } else if (sum.invalid) {
    tone = "bad";
    icon = D_X;
    title = t("sign.bannerInvalid", { count: sum.invalid });
    sub = t("sign.bannerInvalidSub");
  } else if (report?.empty.length) {
    title = t("sign.bannerRequired");
    sub = t("sign.bannerRequiredSub", { count: report.empty.length });
  } else if (sum.signed) {
    tone = sum.untrusted ? "warn" : "ok";
    icon = sum.untrusted ? D_WARN : D_OK;
    title = sum.untrusted ? t("sign.bannerUntrusted") : t("sign.bannerValid");
    sub = sum.untrusted ? t("sign.bannerUntrustedSub") : t("sign.bannerValidSub");
  }
  return (
    <div className={`sbanner sbanner--${tone}`} role="status">
      <span className="sbanner__ic">{verifying ? <span className="spinner" /> : <Path d={icon} />}</span>
      <span className="sbanner__text">
        <span className="sbanner__t">{title}</span>
        <span className="muted">{sub}</span>
      </span>
      {sum.total > 0 && <span className="sbanner__n tnum">{t("sign.count", { n: sum.signed, total: sum.total })}</span>}
    </div>
  );
}

// ---------------------------------------------------------------- couche sur chaque page

/** Marge intérieure et mention : mêmes règles que l'apparence PDF (appearance.rs). */
const PAD = 4;
const CAPTION = 6.5;

/**
 * Aperçu de la signature, dessiné comme dans le PDF (dimensions en points,
 * `scale` pixels par point) : l'emplacement montre exactement le résultat.
 */
function SigPreview({ w, h, scale, visual, ink, caption, inkAnim }: { w: number; h: number; scale: number; visual: Visual; ink: string; caption: string[]; inkAnim?: boolean }) {
  const capH = caption.length ? caption.length * CAPTION * 1.2 + 2 : 0;
  const aw = Math.max(1, w - 2 * PAD);
  const ah = Math.max(1, h - 2 * PAD - capH);
  let body: React.ReactNode = null;
  if (visual.kind === "draw") {
    const b = strokesBounds(visual.strokes);
    if (b) {
      const s = Math.min(aw / b.w, ah / b.h);
      const dx = PAD + (aw - b.w * s) / 2 - b.x * s;
      const dy = PAD + (ah - b.h * s) / 2 - b.y * s;
      body = (
        <path
          d={strokePath(visual.strokes, s, dx, dy)}
          className={inkAnim ? "sprev__draw" : undefined}
          style={{ fill: "none", stroke: ink, strokeWidth: Math.min(3, Math.max(0.6, 2.6 * s)), strokeLinecap: "round", strokeLinejoin: "round" }}
        />
      );
    }
  } else if (visual.kind === "type") {
    const f = SIG_FONTS.find((x) => x.id === visual.font) ?? SIG_FONTS[0];
    const w1 = textWidth1(visual.text, f.family);
    const size = Math.min(ah * 0.72, aw / w1);
    body = (
      <text x={PAD + (aw - w1 * size) / 2} y={PAD + ah / 2 + size * 0.3} style={{ fontFamily: `'${f.family}', cursive`, fontSize: size, fill: ink }}>
        {visual.text}
      </text>
    );
  } else {
    body = <image href={localImageUrl(visual.path)} x={PAD} y={PAD} width={aw} height={ah} preserveAspectRatio="xMidYMid meet" />;
  }
  return (
    <svg className={inkAnim ? "sprev sprev--ink" : "sprev"} width={w * scale} height={h * scale} viewBox={`0 0 ${w} ${h}`} aria-hidden="true">
      {body}
      {caption.map((line, i) => (
        <text key={i} x={PAD} y={h - PAD - capH + CAPTION + 1 + i * CAPTION * 1.2} className="sprev__cap" style={{ fontSize: CAPTION }}>
          {line}
        </text>
      ))}
    </svg>
  );
}

let ctx: CanvasRenderingContext2D | null = null;
/** Largeur du texte pour un corps de 1 point. */
function textWidth1(text: string, family: string) {
  ctx ??= document.createElement("canvas").getContext("2d");
  if (!ctx) return text.length * 0.5;
  ctx.font = `100px '${family}'`;
  return ctx.measureText(text).width / 100 || 0.01;
}

/** Mention qui accompagnera une signature numérique (même texte que celui écrit dans le PDF). */
function useCaption() {
  const { t } = useTranslation();
  const certOn = useSign((s) => s.certOn);
  const name = useSign((s) => s.cert?.name ?? "");
  if (!certOn || !name) return [];
  const when = new Date().toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" });
  return [t("sign.captionBy", { name }), t("sign.captionDate", { date: when })];
}

function statusLook(s: SigReport) {
  if (s.kind === "visual") return { d: D_PEN, tone: "accent" };
  if (s.status === "valid") return { d: D_OK, tone: "ok" };
  if (s.status === "invalid") return { d: D_X, tone: "bad" };
  return { d: D_WARN, tone: "warn" };
}

function SigLayer({ tab, page, scale, report }: { tab: DocTab; page: number; scale: number; report: ReturnType<typeof useSign.getState>["reports"][string] | null }) {
  const { t } = useTranslation();
  const st = useSign();
  const layerRef = useRef<HTMLDivElement>(null);
  const visual = st.visual();
  const size = tab.info.pages[page];
  const busy = st.step >= 0 && st.step < 4;
  const canPlace = !!visual && !busy && !tab.info.encrypted;
  const S = (r: Rect): React.CSSProperties => ({ left: r.x * scale, top: r.y * scale, width: r.w * scale, height: r.h * scale });

  /** Taille par défaut d'une signature, proportions du dessin. */
  const defaultSize = () => {
    // Place de la mention « Signé numériquement par… » sous une signature numérique.
    const extra = st.certOn ? 2 * 6.5 * 1.2 + 2 : 0;
    if (visual?.kind === "draw") {
      const b = strokesBounds(visual.strokes)!;
      const ratio = Math.min(4, Math.max(1.2, b.w / b.h));
      return { w: Math.min(200, 70 * ratio), h: Math.min(200, 70 * ratio) / ratio + 12 + extra };
    }
    return { w: 190, h: 64 + extra };
  };

  const onDown = (e: React.PointerEvent) => {
    if (e.button !== 0 || e.target !== layerRef.current) return;
    if (!canPlace) {
      if (!visual && !busy) useSign.getState().set({ placement: null });
      return;
    }
    const r = layerRef.current!.getBoundingClientRect();
    const x = (e.clientX - r.left) / scale;
    const y = (e.clientY - r.top) / scale;
    const d = defaultSize();
    const rect = { x: Math.max(0, Math.min(size.w - d.w, x - d.w / 2)), y: Math.max(0, Math.min(size.h - d.h, y - d.h / 2)), w: d.w, h: d.h };
    st.set({ placement: { page, rect, field: null } });
  };

  /** Déplacer ou redimensionner l'emplacement choisi. */
  const drag = (e: React.PointerEvent, p: Placement, resize: boolean) => {
    e.stopPropagation();
    e.preventDefault();
    if (busy) return;
    const x0 = e.clientX;
    const y0 = e.clientY;
    const start = p.rect;
    const move = (ev: PointerEvent) => {
      const dx = (ev.clientX - x0) / scale;
      const dy = (ev.clientY - y0) / scale;
      const rect = resize
        ? { ...start, w: Math.max(40, Math.min(size.w - start.x, start.w + dx)), h: Math.max(20, Math.min(size.h - start.y, start.h + dy)) }
        : { ...start, x: Math.max(0, Math.min(size.w - start.w, start.x + dx)), y: Math.max(0, Math.min(size.h - start.h, start.y + dy)) };
      useSign.getState().set({ placement: { ...p, rect, field: resize || (Math.abs(dx) < 1 && Math.abs(dy) < 1) ? p.field : null } });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const placed = st.placement?.page === page ? st.placement : null;
  const caption = useCaption();
  const inked = st.justSigned?.page === page ? st.justSigned : null;
  return (
    <div ref={layerRef} className={`slayer ${canPlace ? "is-placing" : ""}`} onPointerDown={onDown}>
      {report?.empty
        .filter((f) => f.page === page && !(placed?.field === f.field))
        .map((f) => (
          <button
            key={f.field}
            type="button"
            className="sslot"
            style={S(f.rect)}
            disabled={busy || tab.info.encrypted}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => {
              // Sans signature préparée, le champ invite à la créer dans le panneau.
              if (visual) st.set({ placement: { page, rect: f.rect, field: f.field } });
            }}
            title={f.field}
          >
            <Path d={D_PEN} />
            {visual ? t("sign.clickToSign") : t("sign.prepareFirst")}
          </button>
        ))}
      {report?.signatures
        .filter((s) => s.page === page && s.rect)
        .map((s, i) => {
          const look = statusLook(s);
          return (
            <span key={`${s.field}-${i}`} className={`sbadge sbadge--${look.tone}`} style={{ left: (s.rect!.x + s.rect!.w) * scale - 22, top: (s.rect!.y + s.rect!.h) * scale - 22 }} title={`${s.signer} — ${t(`sign.status.${s.status}`)}`}>
              <Path d={look.d} />
            </span>
          );
        })}
      {placed && visual && (
        <div className={`splace ${placed.field ? "is-field" : ""}`} style={S(placed.rect)} onPointerDown={(e) => drag(e, placed, false)} role="img" aria-label={t("sign.placed")}>
          <SigPreview w={placed.rect.w} h={placed.rect.h} scale={scale} visual={visual} ink={st.ink} caption={caption} />
          {!busy && (
            <>
              <button type="button" className="splace__x" aria-label={t("sign.removePlacement")} onPointerDown={(e) => e.stopPropagation()} onClick={() => st.set({ placement: null })}>
                <Icons.close />
              </button>
              <span className="splace__h" onPointerDown={(e) => drag(e, placed, true)} aria-hidden="true" />
            </>
          )}
        </div>
      )}
      {inked && visual && (
        <div className="sinked" style={S(inked.rect)} aria-hidden="true">
          <SigPreview w={inked.rect.w} h={inked.rect.h} scale={scale} visual={visual} ink={st.ink} caption={st.lastCaption} inkAnim />
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- panneau de droite

function SignSide({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const st = useSign();
  const report = st.reports[tab.key] ?? null;
  const jumpTo = useApp((s) => s.jumpTo);
  const [creating, setCreating] = useState(false);
  const busy = st.step >= 0 && st.step < 4;
  const visual = st.visual();

  const steps: StepItem[] = useMemo(() => {
    const list = st.certOn
      ? [
          { id: 0, label: t("sign.stCert") },
          { id: 1, label: t("sign.stDigest") },
          ...(st.tsa ? [{ id: 2, label: t("sign.stTsa") }] : []),
          { id: 3, label: t("sign.stEmbed") },
        ]
      : [
          { id: 0, label: t("sign.stPrepare") },
          { id: 1, label: t("sign.stEmbed") },
        ];
    return list.map((s) => ({ label: s.label, state: st.step > s.id || st.step === 4 ? "done" : st.step === s.id ? "active" : "pending" }));
  }, [st.certOn, st.tsa, st.step, t]);

  const pickCert = async () => {
    const p = await open({ multiple: false, directory: false, title: t("sign.pickCert"), filters: [{ name: t("sign.certFilter"), extensions: ["pfx", "p12"] }] });
    if (typeof p === "string") {
      st.set({ certPath: p, cert: null, certError: null, certPassword: "" });
    }
  };

  const canSign = !!visual && !!st.placement && !busy && !tab.info.encrypted && (!st.certOn || (!!st.cert && !st.cert.expired));
  const btn = busy ? t("sign.signing") : st.step === 4 ? t("sign.signedBtn") : t("sign.signBtn");

  return (
    <aside className="panel signw__side" aria-label={t("nav.signer")}>
      <section className="ssec">
        <h2 className="title">{t("sign.yours")}</h2>
        <Segmented<SigMode>
          block
          label={t("sign.kind")}
          value={st.mode}
          onChange={(mode) => st.set({ mode })}
          options={[
            { value: "draw", label: t("sign.draw") },
            { value: "type", label: t("sign.type") },
            { value: "import", label: t("sign.import") },
          ]}
        />
        {st.mode === "draw" && <Pad />}
        {st.mode === "type" && (
          <div className="rise sfield">
            <TextField value={st.typed} aria-label={t("sign.nameToSign")} placeholder={t("sign.namePh")} onChange={(e) => st.set({ typed: e.target.value })} />
            <div className="sfonts">
              {SIG_FONTS.map((f) => (
                <button key={f.id} type="button" className="sfont" style={{ font: f.css, color: st.ink }} aria-pressed={st.font === f.id} aria-label={t("sign.fontStyle", { name: f.family })} onClick={() => st.set({ font: f.id })}>
                  {st.typed.trim() || t("sign.namePh")}
                </button>
              ))}
            </div>
          </div>
        )}
        {st.mode === "import" && (
          <button type="button" className="simport rise" onClick={() => void pickImage(t("sign.importTitle")).then((p) => p && st.set({ image: p }))}>
            {st.image ? (
              <img src={localImageUrl(st.image)} alt="" className="simport__img" />
            ) : (
              <>
                <span className="simport__bub">
                  <Icons.image />
                </span>
                <span>{t("sign.importBtn")}</span>
                <span className="muted">{t("sign.importHint")}</span>
              </>
            )}
          </button>
        )}
        <div className="srow">
          <span className="muted">{t("sign.ink")}</span>
          <ColorDots size="sm" label={t("sign.ink")} presets={INKS} value={st.ink} onChange={(c) => c && st.set({ ink: c })} />
        </div>
      </section>

      <section className="ssec">
        <div className="srow">
          <h2 className="title">{t("sign.digital")}</h2>
          <Switch label={t("sign.digitalAria")} checked={st.certOn} onChange={(v) => st.set({ certOn: v })} />
        </div>
        {!st.certOn && <p className="muted sp">{t("sign.digitalOff")}</p>}
        {st.certOn && (
          <div className="rise sfield">
            {st.certPath ? (
              <div className="scert">
                <span className={`scert__ic ${st.cert && !st.cert.expired ? "is-ok" : ""}`}>
                  <Path d="M8 1.5l5 2v4c0 3.2-2.2 5.6-5 7-2.8-1.4-5-3.8-5-7v-4z M5.8 8l1.6 1.6 3-3.2" />
                </span>
                <span className="scert__text">
                  <span className="scert__name">{st.cert ? st.cert.name : st.certPath.split(/[\\/]/).pop()}</span>
                  <span className="muted">
                    {st.cert
                      ? `${st.cert.selfSigned ? t("sign.selfSigned") : t("sign.authority", { name: st.cert.issuer })} · ${t("sign.validUntil", { year: new Date(st.cert.notAfter * 1000).getFullYear() })}`
                      : t("sign.enterPassword")}
                  </span>
                </span>
                <Button variant="ghost" size="sm" onClick={() => void pickCert()} disabled={busy}>
                  {t("sign.change")}
                </Button>
              </div>
            ) : (
              <div className="scert-actions">
                <Button onClick={() => void pickCert()}>{t("sign.pickCertBtn")}</Button>
                <Button variant="ghost" onClick={() => setCreating(true)}>
                  {t("sign.createCert")}
                </Button>
              </div>
            )}
            {st.certPath && (
              <TextField
                type="password"
                value={st.certPassword}
                placeholder={t("sign.passwordPh")}
                aria-label={t("sign.passwordAria")}
                invalid={!!st.certError}
                onChange={(e) => st.set({ certPassword: e.target.value, certError: null })}
                onBlur={() => void st.checkCert()}
                onKeyDown={(e) => e.key === "Enter" && void st.checkCert()}
              />
            )}
            {st.certError && <p className="serror">{st.certError}</p>}
            <TextField value={st.reason} placeholder={t("sign.reasonPh")} aria-label={t("sign.reason")} onChange={(e) => st.set({ reason: e.target.value })} />
            <div className="srow">
              <span className="slabel">{t("sign.tsa")}</span>
              <Switch label={t("sign.tsa")} checked={st.tsa} onChange={(v) => st.set({ tsa: v })} />
            </div>
            {st.tsa && <p className="muted sp">{t("sign.tsaHint", { url: st.tsaUrl })}</p>}
          </div>
        )}
      </section>

      {st.step >= 0 && (
        <section className="ssteps rise">
          <StepList steps={steps} />
        </section>
      )}

      <div className="sactions">
        {!st.placement && !busy && <p className="muted sp">{visual ? t("sign.placeHint") : t("sign.prepareHint")}</p>}
        {busy ? (
          <Button size="lg" onClick={() => st.cancel()}>
            {t("common.cancel")}
          </Button>
        ) : (
          <Button variant="primary" size="lg" disabled={!canSign} onClick={() => void st.sign(tab.key)}>
            {btn}
          </Button>
        )}
      </div>

      <section className="ssec">
        <div className="srow">
          <h2 className="title">{t("sign.docSignatures")}</h2>
          <Button variant="ghost" size="sm" onClick={() => void st.loadReport(tab.key, tab.info.path, true)} disabled={st.verifying}>
            {st.verifying ? t("sign.verifying") : t("sign.verify")}
          </Button>
        </div>
        {!report?.signatures.length && !report?.empty.length && <p className="muted sp">{t("sign.noneYet")}</p>}
        {report?.signatures.map((s, i) => {
          const look = statusLook(s);
          const when = s.timestamp ?? s.date;
          return (
            <button key={`${s.field}-${i}`} type="button" className="sitem rise" style={{ animationDelay: `${i * 40}ms` }} onClick={() => s.page !== null && jumpTo(tab.key, s.page)} title={s.messages.join("\n")}>
              <span className={`sitem__ic sitem__ic--${look.tone}`}>{st.verifying ? <span className="spinner" /> : <Path d={look.d} />}</span>
              <span className="sitem__text">
                <span className="sitem__who">
                  {s.signer || t("sign.unknownSigner")}
                  {s.field && <span className="sitem__field"> · {s.field}</span>}
                </span>
                <span className="muted">
                  {[s.kind === "visual" ? t("sign.handwritten") : s.issuer, when ? formatWhen(when) : null, s.timestamp ? t("sign.timestamped") : null].filter(Boolean).join(" · ")}
                </span>
                {s.status === "invalid" && s.messages.length > 0 && <span className="sitem__msg">{s.messages[s.messages.length - 1]}</span>}
              </span>
              <span className={`sitem__chip sitem__chip--${look.tone}`}>{st.verifying ? t("sign.verifying") : t(`sign.status.${s.status}`)}</span>
            </button>
          );
        })}
        {report?.empty.map((f) => (
          <button key={f.field} type="button" className="sitem rise" onClick={() => jumpTo(tab.key, f.page)}>
            <span className="sitem__ic sitem__ic--accent">
              <Path d={D_PEN} />
            </span>
            <span className="sitem__text">
              <span className="sitem__who">{f.field}</span>
              <span className="muted">{t("sign.waitingField", { n: f.page + 1 })}</span>
            </span>
            <span className="sitem__chip sitem__chip--accent">{t("sign.status.pending")}</span>
          </button>
        ))}
      </section>
      {creating && <CreateCert onClose={() => setCreating(false)} />}
    </aside>
  );
}

function formatWhen(iso: string) {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString("fr-FR", { day: "numeric", month: "short", year: "numeric" });
}

// ---------------------------------------------------------------- pad de dessin

function Pad() {
  const { t } = useTranslation();
  const strokes = useSign((s) => s.strokes);
  const ink = useSign((s) => s.ink);
  const set = useSign((s) => s.set);
  const [demo, setDemo] = useState(true);
  const drawing = useRef(false);
  const pt = (e: React.PointerEvent<SVGSVGElement>): [number, number] => {
    const r = e.currentTarget.getBoundingClientRect();
    return [Math.round((e.clientX - r.left) * 10) / 10, Math.round((e.clientY - r.top) * 10) / 10];
  };
  return (
    <div className="rise sfield">
      <div className="spad">
        <svg
          width="100%"
          height="150"
          role="img"
          aria-label={t("sign.padAria")}
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            drawing.current = true;
            setDemo(false);
            set({ strokes: [...useSign.getState().strokes, [pt(e)]], placement: useSign.getState().placement });
          }}
          onPointerMove={(e) => {
            if (!drawing.current) return;
            const s = useSign.getState().strokes.slice();
            const last = s[s.length - 1];
            const p = pt(e);
            const prev = last[last.length - 1];
            if (Math.hypot(p[0] - prev[0], p[1] - prev[1]) < 1.2) return;
            s[s.length - 1] = [...last, p];
            set({ strokes: s });
          }}
          onPointerUp={() => (drawing.current = false)}
          onPointerCancel={() => (drawing.current = false)}
        >
          <path d="M20 118 H 270" className="spad__line" />
          {demo && !strokes.length && <path d={DEMO} className="spad__demo" style={{ stroke: ink }} />}
          <path d={strokePath(strokes) || "M0 0"} style={{ fill: "none", stroke: ink, strokeWidth: 2.6, strokeLinecap: "round", strokeLinejoin: "round" }} />
        </svg>
      </div>
      <div className="srow">
        <span className="muted">{t("sign.padHint")}</span>
        <Button variant="ghost" size="sm" onClick={() => set({ strokes: [] })} disabled={!strokes.length}>
          {t("sign.clear")}
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- certificat personnel

function CreateCert({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const st = useSign();
  const [name, setName] = useState(st.typed || useApp.getState().prefs.userName || useApp.getState().systemName);
  const [email, setEmail] = useState("");
  const [org, setOrg] = useState("");
  const [pw, setPw] = useState("");
  const [pw2, setPw2] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ok = name.trim() && pw.length >= 6 && pw === pw2;
  const create = async () => {
    const out = await save({ title: t("sign.saveCert"), defaultPath: `Certificat ${name.trim()}.p12`, filters: [{ name: t("sign.certFilter"), extensions: ["p12"] }] });
    if (!out) return;
    setBusy(true);
    try {
      const info = await signApi.certCreate(name.trim(), email.trim(), org.trim(), pw, out);
      st.set({ certPath: out, certPassword: pw, cert: info, certError: null, certOn: true });
      void import("../../lib/api").then((m) => m.api.prefsSet("signCert", out)).catch(() => {});
      onClose();
    } catch (e) {
      setError((e as { message?: string }).message ?? String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal open onClose={onClose} label={t("sign.createTitle")} width={460} locked={busy}>
      <ModalHeader icon={<Path d="M8 1.5l5 2v4c0 3.2-2.2 5.6-5 7-2.8-1.4-5-3.8-5-7v-4z M5.8 8l1.6 1.6 3-3.2" />} title={t("sign.createTitle")} onClose={busy ? undefined : onClose} />
      <div className="dialog__body screate">
        <p className="muted">{t("sign.createIntro")}</p>
        <label className="sfield">
          <span className="slabel">{t("sign.fullName")}</span>
          <TextField value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </label>
        <label className="sfield">
          <span className="slabel">{t("sign.email")}</span>
          <TextField value={email} type="email" placeholder={t("sign.optional")} onChange={(e) => setEmail(e.target.value)} />
        </label>
        <label className="sfield">
          <span className="slabel">{t("sign.organization")}</span>
          <TextField value={org} placeholder={t("sign.optional")} onChange={(e) => setOrg(e.target.value)} />
        </label>
        <label className="sfield">
          <span className="slabel">{t("sign.newPassword")}</span>
          <TextField type="password" value={pw} onChange={(e) => setPw(e.target.value)} invalid={pw.length > 0 && pw.length < 6} />
        </label>
        <label className="sfield">
          <span className="slabel">{t("sign.confirmPassword")}</span>
          <TextField type="password" value={pw2} onChange={(e) => setPw2(e.target.value)} invalid={pw2.length > 0 && pw2 !== pw} />
        </label>
        {error && <p className="serror">{error}</p>}
      </div>
      <div className="dialog__foot">
        <Button variant="ghost" onClick={onClose} disabled={busy}>
          {t("common.cancel")}
        </Button>
        <Button variant="primary" onClick={() => void create()} disabled={!ok || busy}>
          {t("sign.createBtn")}
        </Button>
      </div>
    </Modal>
  );
}
