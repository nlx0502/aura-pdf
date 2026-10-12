/**
 * Module Formulaires (maquette Forms.dc.html) : liste des champs et ajout à
 * gauche ; champs saisis et mis en forme directement sur les pages (rendu
 * fidèle au PDF enregistré) ; progression, propriétés, apparence et
 * aplatissement à droite.
 */
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, ColorDots, Icons, Segmented, Stepper, Switch, TextField, ToastHost } from "../../components";
import { perms, type Rect } from "../../lib/api";
import { annotApi } from "../../lib/annot";
import {
  ADDABLE,
  captionPos,
  CHECK_GLYPH,
  defaultSize,
  FONT_CSS,
  fontSizeOf,
  fromIsoDate,
  isFilled,
  KIND_ICON,
  progress,
  toIsoDate,
  validEmail,
  type Caption,
  type CaptionPos,
  type CheckStyle,
  type FieldKind,
  type FormField,
  type Widget,
} from "../../lib/forms";
import { isMod, isTyping, shortcut } from "../../lib/platform";
import { useActiveTab, useApp, type DocTab } from "../../stores/app";
import { useDocForms, useForms, type DocForms } from "../../stores/forms";
import { useSign } from "../../stores/sign";
import { Viewer } from "../reader/Viewer";
import "./forms.css";

const RING = 213.6; // 2π × 34
const TEXT_COLORS = [
  { hex: "#1A1B1E", name: "Noir" },
  { hex: "#2563C9", name: "Bleu" },
  { hex: "#1F2A6B", name: "Encre" },
  { hex: "#D2453D", name: "Rouge" },
];
const BG_COLORS = [
  { hex: "#FFFFFF", name: "Blanc" },
  { hex: "#EEF2FF", name: "Lavande" },
  { hex: "#FFF7E6", name: "Crème" },
  { hex: "#EAF7F0", name: "Menthe" },
];
const BORDER_COLORS = [
  { hex: "#C9C7BE", name: "Gris" },
  { hex: "#6E6AF6", name: "Violet" },
  { hex: "#1A1B1E", name: "Noir" },
  { hex: "#D2453D", name: "Rouge" },
];
const CHECK_STYLES: CheckStyle[] = ["check", "cross", "circle", "square", "star", "diamond"];

function KindIcon({ kind }: { kind: FieldKind }) {
  return (
    <svg className="i" width="16" height="16" aria-hidden="true">
      <path d={KIND_ICON[kind]} />
    </svg>
  );
}

export function Forms() {
  const tab = useActiveTab();
  const ensure = useForms((s) => s.ensure);
  useEffect(() => {
    if (tab) void ensure(tab.key, tab.info, tab.viewRev ?? 0);
  }, [tab?.key, tab?.info.id, tab?.viewRev, ensure]); // eslint-disable-line react-hooks/exhaustive-deps
  const d = useDocForms(tab?.key);
  if (!tab || !d || d.docId !== tab.info.id || !d.loaded) return <div className="workspace" />;
  return <FormsInner tab={tab} d={d} />;
}

function FormsInner({ tab, d }: { tab: DocTab; d: DocForms }) {
  const { t } = useTranslation();
  const store = useForms();
  const canFill = perms(tab.info).forms;
  const canEdit = perms(tab.info).edit;
  const sel = d.fields.find((f) => f.name === d.selected) ?? null;
  const p = progress(d.fields);
  const [confirm, setConfirm] = useState(false);

  // Raccourcis : Échap (annule le placement), Suppr (champ sélectionné), flèches
  // (déplacer) ; Ctrl+S est géré globalement (enregistre aussi le formulaire).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = useForms.getState();
      const cur = s.docs[tab.key];
      if (!cur || isMod(e)) return;
      if (e.key === "Escape" && cur.placing) {
        e.preventDefault();
        s.startPlacing(tab.key, null);
        return;
      }
      if (isTyping(e) || document.querySelector(".scrim, .menu")) return;
      if ((e.key === "Delete" || e.key === "Backspace") && cur.selected && canEdit) {
        e.preventDefault();
        s.remove(tab.key, cur.selected);
      } else if (cur.selected && canEdit && e.key.startsWith("Arrow")) {
        e.preventDefault();
        const step = e.shiftKey ? 10 : 1;
        const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
        const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
        s.move(tab.key, cur.selected, dx, dy);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tab.key, canEdit]);

  const goTo = (f: FormField) => {
    store.select(tab.key, f.name);
    const w = f.widgets[0];
    if (w) useApp.getState().jumpTo(tab.key, w.page, Math.max(0, w.rect.y - 80));
  };

  const missing = p.missing;
  const ringColor = p.pct === 100 ? "var(--success)" : "var(--accent)";

  return (
    <div className={`workspace formsw ${d.flattening ? "is-flattening" : ""}`}>
      {/* ---- Champs */}
      <aside className="panel forms__left" aria-label={t("forms.fields")}>
        <div className="forms__head">
          <h2 className="title">{t("forms.fields")}</h2>
          <span className="pill-count tnum">{d.fields.length}</span>
        </div>
        <div className="flist" role="list">
          {d.fields.length === 0 && (
            <div className="flist__empty">
              <span>{t("forms.none")}</span>
              <span className="muted">{t("forms.noneSub")}</span>
            </div>
          )}
          {d.fields.map((f) => (
            <button
              key={f.name}
              type="button"
              role="listitem"
              className={`fitem ${f.name === d.selected ? "fitem--cur" : ""} ${f.name === d.fresh ? "fitem--fresh" : ""}`}
              aria-current={f.name === d.selected || undefined}
              onClick={() => goTo(f)}
            >
              <span className="fitem__ic">
                <KindIcon kind={f.kind} />
              </span>
              <span className="fitem__t">
                <span className="fitem__l">{f.label}</span>
                <span className="muted">
                  {t(`forms.kind.${f.kind}`)}
                  {f.required ? t("forms.required") : ""}
                </span>
              </span>
              <span className="fitem__dot" style={{ background: isFilled(f) ? "var(--success)" : f.required ? "var(--warning)" : "#3A3B42" }} />
            </button>
          ))}
        </div>
        <div className="fadd">
          <span className="muted">{t("forms.addField")}</span>
          <div className="fadd__grid">
            {ADDABLE.map((k) => (
              <button
                key={k}
                type="button"
                className="fadd__k"
                aria-pressed={d.placing === k}
                disabled={!canEdit || d.flattening}
                onClick={() => store.startPlacing(tab.key, d.placing === k ? null : k)}
                title={t(`forms.kind.${k}`)}
              >
                <KindIcon kind={k} />
                <span>{t(`forms.kind.${k}`)}</span>
              </button>
            ))}
          </div>
        </div>
      </aside>

      {/* ---- Document */}
      <main className="panel forms__main">
        {d.placing && <div className="fplace rise">{t("forms.placeHint", { kind: t(`forms.kind.${d.placing}`) })}</div>}
        {!canFill && <div className="forms__notice">{t("forms.locked")}</div>}
        <Viewer key={tab.key} tab={tab} active renderForms={false} overlay={(page, scale) => <FormLayer tab={tab} d={d} page={page} scale={scale} canFill={canFill} canEdit={canEdit} />} />
        <ToastHost only="right" contained />
      </main>

      {/* ---- Progression, propriétés, aplatissement */}
      <aside className="panel forms__side" aria-label={t("forms.props")}>
        <section className="fprog">
          <div className="fring">
            <svg width="84" height="84" viewBox="0 0 84 84" aria-hidden="true">
              <circle cx="42" cy="42" r="34" className="fring__bg" />
              <circle cx="42" cy="42" r="34" className="fring__fg" style={{ strokeDashoffset: (RING * (1 - p.pct / 100)).toFixed(1), stroke: ringColor }} />
            </svg>
            <span className="fring__t tnum">{p.pct} %</span>
          </div>
          <div className="fprog__t">
            <h2 className="title">{p.pct === 100 ? t("forms.complete") : t("forms.progress")}</h2>
            <span className="muted tnum">{t("forms.filled", { filled: p.filled, count: p.total })}</span>
            <span className="fprog__req" style={{ color: missing ? "var(--warning-text)" : "#6FDCA8" }}>
              {missing ? t("forms.missing", { count: missing }) : t("forms.allRequired")}
            </span>
          </div>
        </section>

        <section className="fprops">
          <h2 className="title">{t("forms.props")}</h2>
          {sel ? <FieldProps key={sel.name} tab={tab} f={sel} disabled={!canEdit || d.flattening} /> : <span className="muted">{t("forms.selectHint")}</span>}
        </section>

        <div className="forms__foot">
          <Button variant={d.dirty ? "primary" : "secondary"} size="lg" disabled={!d.dirty || d.saving} onClick={() => void store.save(tab.key)} title={`${t("forms.save")} (${shortcut("S")})`}>
            {d.saving ? t("forms.saving") : t("forms.save")}
          </Button>
          {d.flatBackup && d.fields.length === 0 ? (
            <div className="fflat fflat--done rise">
              <span className="tool__done-ic popin">
                <Icons.check />
              </span>
              <span className="fflat__t">{t("forms.flattened")}</span>
              <Button variant="ghost" size="sm" onClick={() => void store.unflatten(tab.key)}>
                {t("common.cancel")}
              </Button>
            </div>
          ) : (
            d.fields.length > 0 && (
              <div className="fflat">
                <div className="fflat__head">
                  <h3>{t("forms.flatten")}</h3>
                  <span className="muted">{t("forms.flattenSub")}</span>
                </div>
                {confirm ? (
                  <div className="fflat__btns rise">
                    <Button variant="ghost" onClick={() => setConfirm(false)}>
                      {t("common.cancel")}
                    </Button>
                    <Button
                      variant="danger"
                      onClick={() => {
                        setConfirm(false);
                        void store.flatten(tab.key);
                      }}
                    >
                      {t("forms.flattenConfirm")}
                    </Button>
                  </div>
                ) : (
                  <Button onClick={() => setConfirm(true)} disabled={!canEdit || d.flattening}>
                    {t("forms.flatten")}
                  </Button>
                )}
              </div>
            )
          )}
        </div>
      </aside>
    </div>
  );
}

// ---------------------------------------------------------------- propriétés

function FieldProps({ tab, f, disabled }: { tab: DocTab; f: FormField; disabled: boolean }) {
  const { t } = useTranslation();
  const store = useForms();
  const [pane, setPane] = useState<"field" | "look">("field");
  const set = (p: Parameters<typeof store.setProps>[2]) => store.setProps(tab.key, f.name, p);
  const style = (p: Parameters<typeof store.setStyle>[2]) => store.setStyle(tab.key, f.name, p);
  const textLike = f.kind === "text" || f.kind === "email" || f.kind === "date";
  const choice = f.kind === "select" || f.kind === "list" || f.kind === "radio";
  const button = f.kind === "check" || f.kind === "radio";
  const capPos: CaptionPos | "none" = f.caption && f.widgets[0] ? (f.kind === "radio" ? "above" : captionPos(f.caption, f.widgets[0].rect)) : "none";
  const [optText, setOptText] = useState(() => f.options.map((o) => o.label).join("\n"));
  const s = f.style;

  return (
    <fieldset className="fprops__f" disabled={disabled}>
      <Segmented
        block
        size="sm"
        label={t("forms.props")}
        value={pane}
        options={[
          { value: "field", label: t("forms.paneField") },
          { value: "look", label: t("forms.paneLook") },
        ]}
        onChange={setPane}
      />
      {pane === "field" ? (
        <>
          <label className="sfield">
            <span className="muted">{t("forms.label")}</span>
            <TextField value={f.label} onChange={(e) => set({ label: e.target.value })} />
          </label>
          <div className="sfield">
            <span className="muted">{t("forms.captionShow")}</span>
            <Segmented
              block
              size="sm"
              label={t("forms.captionShow")}
              value={capPos}
              options={(f.kind === "radio" ? ["none", "above"] : ["none", "above", "left", "right"]).map((v) => ({ value: v as CaptionPos | "none", label: t(`forms.cap.${v}`) }))}
              onChange={(v) => store.setCaption(tab.key, f.name, v === "none" ? null : v)}
            />
          </div>
          {textLike && (
            <label className="sfield">
              <span className="muted">{t("forms.placeholder")}</span>
              <TextField value={f.placeholder} placeholder={t("forms.placeholderHint")} onChange={(e) => set({ placeholder: e.target.value })} />
            </label>
          )}
          {choice && (
            <label className="sfield">
              <span className="muted">{t("forms.options")}</span>
              <textarea
                className="field fprops__opts"
                rows={Math.min(6, Math.max(3, f.options.length + 1))}
                value={optText}
                onChange={(e) => setOptText(e.target.value)}
                onBlur={() => store.setOptions(tab.key, f.name, optText.split("\n"))}
              />
            </label>
          )}
          <div className="tool__row">
            <span>{t("forms.requiredLabel")}</span>
            <Switch label={t("forms.requiredLabel")} checked={f.required} onChange={(required) => set({ required })} />
          </div>
          <div className="tool__row">
            <span>{t("forms.readonly")}</span>
            <Switch label={t("forms.readonly")} checked={f.readonly} onChange={(readonly) => set({ readonly })} />
          </div>
          {f.kind === "text" && (
            <div className="tool__row">
              <span>{t("forms.multiline")}</span>
              <Switch label={t("forms.multiline")} checked={f.multiline} onChange={(multiline) => set({ multiline })} />
            </div>
          )}
          {textLike && (
            <div className="tool__row">
              <span>{t("forms.maxLen")}</span>
              <Stepper value={f.maxLen ?? 0} min={0} max={999} decLabel={t("forms.less")} incLabel={t("forms.more")} onChange={(v) => set({ maxLen: v || null })} />
            </div>
          )}
          <div className="tool__row">
            <span>{t("forms.type")}</span>
            <span className="fprops__type">
              <KindIcon kind={f.kind} />
              {t(`forms.kind.${f.kind}`)}
            </span>
          </div>
          <Button variant="ghost" className="fprops__del" icon={<Icons.trash />} onClick={() => store.remove(tab.key, f.name)}>
            {t("forms.deleteField")}
          </Button>
        </>
      ) : (
        <>
          {f.kind !== "sig" && (
            <>
              {!button && (
                <div className="sfield">
                  <span className="muted">{t("forms.font")}</span>
                  <Segmented
                    block
                    size="sm"
                    label={t("forms.font")}
                    value={s.font}
                    options={[
                      { value: "sans", label: "Sans" },
                      { value: "serif", label: "Serif" },
                      { value: "mono", label: "Mono" },
                    ]}
                    onChange={(font) => style({ font })}
                  />
                </div>
              )}
              {!button && (
                <div className="tool__row">
                  <span>{t("forms.bold")}</span>
                  <Switch label={t("forms.bold")} checked={s.bold} onChange={(bold) => style({ bold })} />
                </div>
              )}
              <div className="tool__row">
                <span>{t("forms.fontSize")}</span>
                <span className="fprops__size">
                  {s.fontSize > 0 ? (
                    <Stepper value={s.fontSize} min={6} max={36} decLabel={t("forms.less")} incLabel={t("forms.more")} onChange={(fontSize) => style({ fontSize })} />
                  ) : (
                    <span className="muted">{t("forms.autoSize")}</span>
                  )}
                  <Switch label={t("forms.autoSize")} checked={s.fontSize === 0} onChange={(auto) => style({ fontSize: auto ? 0 : 11 })} />
                </span>
              </div>
              <div className="sfield">
                <span className="muted">{button ? t("forms.markColor") : t("forms.textColor")}</span>
                <ColorDots presets={TEXT_COLORS} value={s.textColor} label={t("forms.textColor")} onChange={(c) => c && style({ textColor: c.toLowerCase() })} />
              </div>
            </>
          )}
          {(textLike || f.kind === "select" || f.kind === "list") && (
            <div className="sfield">
              <span className="muted">{t("forms.align")}</span>
              <Segmented
                block
                size="sm"
                label={t("forms.align")}
                value={String(s.align)}
                options={[
                  { value: "0", label: t("forms.alignLeft") },
                  { value: "1", label: t("forms.alignCenter") },
                  { value: "2", label: t("forms.alignRight") },
                ]}
                onChange={(v) => style({ align: Number(v) as 0 | 1 | 2 })}
              />
            </div>
          )}
          {button && (
            <div className="sfield">
              <span className="muted">{t("forms.mark")}</span>
              <div className="fmarks" role="radiogroup" aria-label={t("forms.mark")}>
                {CHECK_STYLES.map((c) => (
                  <button key={c} type="button" role="radio" aria-checked={s.checkStyle === c} className="fmark" title={t(`forms.marks.${c}`)} onClick={() => style({ checkStyle: c })}>
                    {CHECK_GLYPH[c]}
                  </button>
                ))}
              </div>
            </div>
          )}
          <div className="sfield">
            <span className="muted">{t("forms.background")}</span>
            <ColorDots presets={BG_COLORS} allowNone value={s.background} label={t("forms.background")} onChange={(c) => style({ background: c?.toLowerCase() ?? null })} />
          </div>
          <div className="sfield">
            <span className="muted">{t("forms.border")}</span>
            <ColorDots
              presets={BORDER_COLORS}
              allowNone
              value={s.borderWidth > 0 ? s.border : null}
              label={t("forms.border")}
              onChange={(c) => style(c ? { border: c.toLowerCase(), borderWidth: s.borderWidth || 1 } : { borderWidth: 0 })}
            />
          </div>
          {s.borderWidth > 0 && (
            <>
              <div className="tool__row">
                <span>{t("forms.borderWidth")}</span>
                <Stepper value={s.borderWidth} min={1} max={4} decLabel={t("forms.less")} incLabel={t("forms.more")} onChange={(borderWidth) => style({ borderWidth })} />
              </div>
              {f.kind !== "radio" && (
                <Segmented
                  block
                  size="sm"
                  label={t("forms.borderStyle")}
                  value={s.borderStyle}
                  options={[
                    { value: "solid", label: t("forms.bs.solid") },
                    { value: "dashed", label: t("forms.bs.dashed") },
                    { value: "underline", label: t("forms.bs.underline") },
                  ]}
                  onChange={(borderStyle) => style({ borderStyle })}
                />
              )}
            </>
          )}
        </>
      )}
    </fieldset>
  );
}

// ---------------------------------------------------------------- page

/** Champs d'une page, dessinés comme dans le PDF et saisis directement. */
function FormLayer({ tab, d, page, scale, canFill, canEdit }: { tab: DocTab; d: DocForms; page: number; scale: number; canFill: boolean; canEdit: boolean }) {
  const { t } = useTranslation();
  const store = useForms();
  const layerRef = useRef<HTMLDivElement>(null);
  const [ghost, setGhost] = useState<Rect | null>(null);
  const size = tab.info.pages[page];
  const S = (r: Rect): React.CSSProperties => ({ left: r.x * scale, top: r.y * scale, width: r.w * scale, height: r.h * scale });

  // Les champs enregistrés ne sont pas dessinés par PDFium : la couche les remplace (aperçu fidèle).
  const hidden = d.orig
    .flatMap((f) => [...f.widgets.map((w) => (w.page === page ? w.index : null)), f.caption?.page === page ? f.caption.index : null, ...f.widgets.map((w) => (w.caption?.page === page ? w.caption.index : null))])
    .filter((i): i is number => typeof i === "number");
  const hiddenKey = hidden.join(",");
  useEffect(() => {
    void annotApi.hide(tab.info.id, page, hidden).catch(() => {});
    return () => void annotApi.hide(tab.info.id, page, []).catch(() => {});
  }, [tab.info.id, page, hiddenKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const items: { f: FormField; w: Widget; i: number; order: number }[] = [];
  d.fields.forEach((f, order) => f.widgets.forEach((w, i) => w.page === page && items.push({ f, w, i, order })));
  const captions: { f: FormField; c: Caption; key: string }[] = [];
  d.fields.forEach((f) => {
    if (f.caption?.page === page) captions.push({ f, c: f.caption, key: `${f.name}:cap` });
    f.widgets.forEach((w, i) => w.caption?.page === page && captions.push({ f, c: w.caption, key: `${f.name}:cap${i}` }));
  });
  const sel = d.fields.find((f) => f.name === d.selected && f.widgets.some((w) => w.page === page));

  /** Placement d'un nouveau champ : clic (taille par défaut) ou glisser. */
  const onDown = (e: React.PointerEvent) => {
    if (!d.placing || e.button !== 0) return;
    e.preventDefault();
    const box = layerRef.current!.getBoundingClientRect();
    const x0 = (e.clientX - box.left) / scale;
    const y0 = (e.clientY - box.top) / scale;
    const kind = d.placing;
    let cur: Rect | null = null;
    const move = (ev: PointerEvent) => {
      const x1 = (ev.clientX - box.left) / scale;
      const y1 = (ev.clientY - box.top) / scale;
      cur = { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
      setGhost(cur);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setGhost(null);
      const def = defaultSize(kind);
      let r: Rect = cur && cur.w > 6 && cur.h > 6 && kind !== "radio" ? cur : { x: x0 - (kind === "radio" ? 0 : def.w / 2), y: y0 - def.h / 2, ...def };
      if (kind === "check") r = { ...r, h: r.w };
      r = { ...r, x: Math.max(0, Math.min(size.w - r.w, r.x)), y: Math.max(0, Math.min(size.h - r.h, r.y)) };
      store.place(tab.key, page, r);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  /** Déplacer (poignée) ou redimensionner (coin) le champ sélectionné. */
  const drag = (e: React.PointerEvent, f: FormField, resize: boolean) => {
    e.preventDefault();
    e.stopPropagation();
    const x0 = e.clientX;
    const y0 = e.clientY;
    const start = f.widgets[0].rect;
    let last = { dx: 0, dy: 0 };
    const move = (ev: PointerEvent) => {
      const dx = (ev.clientX - x0) / scale;
      const dy = (ev.clientY - y0) / scale;
      if (resize) {
        const w = Math.max(f.kind === "check" ? 8 : 24, Math.min(size.w - start.x, start.w + dx));
        const h = f.kind === "check" ? w : Math.max(10, Math.min(size.h - start.y, start.h + dy));
        store.resize(tab.key, f.name, { ...start, w, h });
      } else {
        store.move(tab.key, f.name, dx - last.dx, dy - last.dy);
        last = { dx, dy };
      }
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div ref={layerRef} className={`flayer ${d.placing ? "flayer--placing" : ""}`} onPointerDown={onDown}>
      {captions.map(({ f, c, key }) => (
        <span
          key={key}
          className={`fcap ${f.name === d.selected ? "fcap--cur" : ""}`}
          style={{ ...S(c.rect), fontSize: c.size * scale, lineHeight: `${c.rect.h * scale}px`, color: c.color, fontWeight: c.bold ? 700 : 400 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            store.select(tab.key, f.name);
          }}
        >
          {c.text}
        </span>
      ))}
      {items.map(({ f, w, i, order }) => (
        <FieldBox key={`${f.name}:${i}`} tab={tab} d={d} f={f} w={w} scale={scale} order={order} readOnly={!canFill || f.readonly || d.flattening} />
      ))}
      {sel && canEdit && !d.placing && <SelectionFrame f={sel} page={page} scale={scale} onDrag={drag} label={t("forms.moveHint")} />}
      {ghost && <span className="fghost" style={S(ghost)} />}
    </div>
  );
}

function groupRect(f: FormField, page: number): Rect {
  const rs = f.widgets.filter((w) => w.page === page).flatMap((w) => [w.rect, ...(w.caption ? [w.caption.rect] : [])]);
  const x = Math.min(...rs.map((r) => r.x));
  const y = Math.min(...rs.map((r) => r.y));
  return { x, y, w: Math.max(...rs.map((r) => r.x + r.w)) - x, h: Math.max(...rs.map((r) => r.y + r.h)) - y };
}

function SelectionFrame({ f, page, scale, onDrag, label }: { f: FormField; page: number; scale: number; onDrag: (e: React.PointerEvent, f: FormField, resize: boolean) => void; label: string }) {
  const r = groupRect(f, page);
  const pad = 4;
  const single = f.widgets.length === 1 && f.kind !== "radio";
  return (
    <div className="fsel" style={{ left: r.x * scale - pad, top: r.y * scale - pad, width: r.w * scale + pad * 2, height: r.h * scale + pad * 2 }}>
      <button type="button" className="fsel__grip" title={label} aria-label={label} onPointerDown={(e) => onDrag(e, f, false)}>
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M8 1.5v13 M1.5 8h13 M8 1.5l-2 2 M8 1.5l2 2 M8 14.5l-2-2 M8 14.5l2-2 M1.5 8l2-2 M1.5 8l2 2 M14.5 8l-2-2 M14.5 8l-2 2" />
        </svg>
      </button>
      {single && <span className="fsel__knob" onPointerDown={(e) => onDrag(e, f, true)} />}
    </div>
  );
}

/** Un widget, avec l'apparence qu'il aura dans le PDF. */
function FieldBox({ tab, d, f, w, scale, order, readOnly }: { tab: DocTab; d: DocForms; f: FormField; w: Widget; scale: number; order: number; readOnly: boolean }) {
  const { t } = useTranslation();
  const store = useForms();
  const s = f.style;
  const r = w.rect;
  const radio = f.kind === "radio";
  const bw = s.borderWidth > 0 && s.border ? Math.max(1, s.borderWidth * scale) : 0;
  const fs = fontSizeOf(s, r.h, f.multiline) * scale;
  const cur = f.name === d.selected;
  const css: React.CSSProperties = {
    left: r.x * scale,
    top: r.y * scale,
    width: r.w * scale,
    height: r.h * scale,
    background: s.background ?? "transparent",
    borderColor: s.border ?? "transparent",
    borderStyle: bw ? (s.borderStyle === "dashed" ? "dashed" : "solid") : "none",
    borderWidth: bw ? (s.borderStyle === "underline" && !radio ? `0 0 ${bw}px 0` : bw) : 0,
    borderRadius: radio ? "50%" : 0,
    color: s.textColor,
    fontFamily: FONT_CSS[s.font],
    fontWeight: s.bold ? 700 : 400,
    fontSize: fs,
    textAlign: (["left", "center", "right"] as const)[s.align],
    // Apparition en cascade au chargement (maquette) ; un champ tout juste ajouté apparaît aussitôt.
    animationDelay: f.isNew ? "0ms" : `${Math.min(order, 12) * 50}ms`,
  };
  const cls = `fw fw--${f.kind} ${cur ? "fw--cur" : ""} ${f.name === d.fresh ? "fw--fresh" : ""} ${f.required && !isFilled(f) ? "fw--req" : ""}`;
  const select = () => store.select(tab.key, f.name);
  const down = (e: React.PointerEvent) => {
    e.stopPropagation();
    select();
  };

  if (f.kind === "text" || f.kind === "email" || f.kind === "date") {
    const bad = f.kind === "email" && !validEmail(f.value);
    const common = {
      className: "fw__in",
      "aria-label": f.label,
      title: bad ? t("forms.invalidEmail") : f.label,
      readOnly,
      maxLength: f.maxLen ?? undefined,
      placeholder: f.placeholder,
      onFocus: select,
    };
    return (
      <div className={`${cls} ${bad ? "fw--bad" : ""}`} style={css} onPointerDown={down}>
        {f.multiline ? (
          <textarea {...common} value={f.value} onChange={(e) => store.setValue(tab.key, f.name, e.target.value)} />
        ) : (
          <input
            {...common}
            type={f.kind === "date" ? "date" : f.kind === "email" ? "email" : "text"}
            value={f.kind === "date" ? toIsoDate(f.value) || "" : f.value}
            onChange={(e) => store.setValue(tab.key, f.name, f.kind === "date" ? fromIsoDate(e.target.value) : e.target.value)}
          />
        )}
      </div>
    );
  }
  if (f.kind === "select" || f.kind === "list") {
    return (
      <div className={cls} style={css} onPointerDown={down}>
        <select className="fw__in" aria-label={f.label} disabled={readOnly} value={f.value} onFocus={select} onChange={(e) => store.setValue(tab.key, f.name, e.target.value)}>
          <option value="">{t("forms.choose")}</option>
          {f.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>
    );
  }
  if (f.kind === "check" || radio) {
    const on = !!f.value && (f.kind === "check" || f.value === w.on);
    const glyphSize = (s.fontSize > 0 ? s.fontSize : Math.min(r.w, r.h) * 0.78) * scale;
    return (
      <button
        type="button"
        className={cls}
        style={css}
        role={radio ? "radio" : "checkbox"}
        aria-checked={on}
        aria-label={radio ? `${f.label} : ${f.options.find((o) => o.value === w.on)?.label ?? w.on ?? ""}` : f.label}
        disabled={readOnly}
        onPointerDown={down}
        onClick={() => store.setValue(tab.key, f.name, f.kind === "check" ? (on ? "" : (w.on ?? "Oui")) : (w.on ?? ""))}
      >
        {on && (
          <span className="fw__mark" style={{ fontSize: radio && s.checkStyle === "circle" ? glyphSize * 0.62 : glyphSize }}>
            {CHECK_GLYPH[s.checkStyle]}
          </span>
        )}
      </button>
    );
  }
  if (f.kind === "sig") {
    return (
      <button
        type="button"
        className={cls}
        style={{ ...css, fontSize: 12 * scale }}
        aria-label={f.label}
        onPointerDown={down}
        onDoubleClick={() => {
          if (f.value) return;
          // Signer ce champ : le module Signer s'ouvre sur l'emplacement.
          useSign.getState().set({ placement: { page: w.page, rect: w.rect, field: f.isNew ? null : f.name } });
          useApp.getState().setModule("signer");
        }}
      >
        <span>{f.value ? t("forms.signed") : t("forms.signHere")}</span>
      </button>
    );
  }
  return <span className={cls} style={css} />;
}
