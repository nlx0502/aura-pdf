import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button, Chip, ColorDots, IconButton, Icons, Segmented, Slider, Stepper, Switch, TextField } from "../../components";
import { TEXT_COLORS } from "../../lib/annot";
import { fillNumber, NUM_FORMATS, type ContentItem, type NumPos } from "../../lib/edit";
import { pickImage } from "../../lib/files";
import type { DocTab } from "../../stores/app";
import { useEdit, type DocEdit, type RedactPattern } from "../../stores/edit";
import { resolveItem, useContentSel } from "./contentSel";
import { linkList, specOf } from "./ContentView";

const WM_COLORS = [
  { name: "indigo", hex: "#6E6AF6" },
  { name: "rouge", hex: "#D2453D" },
  { name: "gris", hex: "#8A8D96" },
  { name: "ambre", hex: "#F2B84B" },
];

type SecId = "hdr" | "num" | "wm" | "crop" | "red";

/** Pictogrammes des sections (tracés de la maquette). */
const SEC_PATH: Record<SecId, string> = {
  hdr: "M2.5 2.5h11v11h-11z M2.5 5h11 M2.5 11h11",
  num: "M4 3.5h1.5v9 M3.5 12.5h3 M9 5a2 2 0 1 1 3.5 1.3L9 12.5h4",
  wm: "M8 2c2.5 3 4 5.2 4 7.2a4 4 0 0 1-8 0C4 7.2 5.5 5 8 2z",
  crop: "M4.5 1.5v10h10 M1.5 4.5h10v10",
  red: "M2.5 5h11v2.5h-11z M2.5 9.5h7v2.5h-7z",
};

function Section({ id, title, tag, danger, open, onToggle, children }: { id: SecId; title: string; tag?: string; danger?: boolean; open: boolean; onToggle: () => void; children: ReactNode }) {
  return (
    <section className="esec">
      <button type="button" className="esec__head" aria-expanded={open} onClick={onToggle}>
        <span className="esec__ic">
          <svg className="i" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d={SEC_PATH[id]} />
          </svg>
        </span>
        <span className="esec__title">{title}</span>
        {tag && <span className={`esec__tag ${danger ? "esec__tag--danger" : ""}`}>{tag}</span>}
        <Icons.chevronRight className="i esec__chev" />
      </button>
      {open && <div className="esec__body rise">{children}</div>}
    </section>
  );
}

export function EditSide({ tab, d, readOnly }: { tab: DocTab; d: DocEdit; readOnly: boolean }) {
  const { t } = useTranslation();
  const store = useEdit();
  const [open, setOpen] = useState<Record<SecId, boolean>>({ hdr: false, num: true, wm: true, crop: false, red: true });
  const s = d.settings;
  const set = store.setSettings;
  const toggle = (id: SecId) => setOpen((o) => ({ ...o, [id]: !o[id] }));
  const marks = d.pages.reduce((n, p) => n + p.redact.length, 0);
  const markedPages = d.pages.filter((p) => p.redact.length).length;
  const total = d.pages.length;

  const patterns: RedactPattern[] = ["iban", "email", "tel", "card"];
  const terms = d.redact.terms.split(/[,;\n]/).filter((x) => x.trim());
  const canScan = d.redact.patterns.length > 0 || terms.length > 0;
  const scanning = d.redact.step === "scan";
  const status = scanning
    ? { tone: "warn", icon: "scan", msg: t("edit.redScan") }
    : d.redact.step === "done"
      ? d.redact.found
        ? { tone: "ok", icon: "ok", msg: `${t("edit.redFound", { count: d.redact.found })} ${t("edit.redMarked", { zones: marks, pages: markedPages })}` }
        : { tone: "warn", icon: "info", msg: marks ? t("edit.redMarked", { zones: marks, pages: markedPages }) : t("edit.redNone") }
      : marks
        ? { tone: "warn", icon: "info", msg: t("edit.redMarked", { zones: marks, pages: markedPages }) }
        : { tone: "warn", icon: "info", msg: canScan ? t("edit.redReady") : t("edit.redChoose") };

  return (
    <aside className="panel eside" aria-label={t("nav.editer")}>
      {d.mode === "content" && <ContentPanel tab={tab} d={d} readOnly={readOnly} />}
      <fieldset className="eside__sections" disabled={readOnly}>
        <Section id="hdr" title={t("edit.sections.hdr")} tag={s.header.on ? t("edit.on") : undefined} open={open.hdr} onToggle={() => toggle("hdr")}>
          <div className="eside__row">
            <span>{t("edit.showEveryPage")}</span>
            <Switch label={t("edit.sections.hdr")} checked={s.header.on} onChange={(v) => set(tab.key, (x) => ({ ...x, header: { ...x.header, on: v } }))} />
          </div>
          <label className="eside__field">
            <span className="muted">{t("edit.header")}</span>
            <TextField value={s.header.header} onChange={(e) => set(tab.key, (x) => ({ ...x, header: { ...x.header, header: e.target.value, on: true } }), "header")} />
          </label>
          <label className="eside__field">
            <span className="muted">{t("edit.footer")}</span>
            <TextField value={s.header.footer} placeholder={t("edit.footerPh")} onChange={(e) => set(tab.key, (x) => ({ ...x, header: { ...x.header, footer: e.target.value, on: true } }), "footer")} />
          </label>
        </Section>

        <Section id="num" title={t("edit.sections.num")} tag={s.numbering.on ? t("edit.on") : undefined} open={open.num} onToggle={() => toggle("num")}>
          <div className="eside__row">
            <span>{t("edit.numberPages")}</span>
            <Switch label={t("edit.sections.num")} checked={s.numbering.on} onChange={(v) => set(tab.key, (x) => ({ ...x, numbering: { ...x.numbering, on: v } }))} />
          </div>
          <div className="eside__field">
            <span className="muted">{t("edit.format")}</span>
            <div className="eside__chips">
              {NUM_FORMATS.map((f) => (
                <Chip key={f} pressed={s.numbering.format === f} onClick={() => set(tab.key, (x) => ({ ...x, numbering: { ...x.numbering, format: f, on: true } }))}>
                  {fillNumber(f, 1, total)}
                </Chip>
              ))}
            </div>
          </div>
          <div className="eside__field">
            <span className="muted">{t("edit.position")}</span>
            <div className="epos" role="group" aria-label={t("edit.position")}>
              {(["tl", "tc", "tr", "bl", "bc", "br"] as NumPos[]).map((p) => (
                <button
                  key={p}
                  type="button"
                  className="chip epos__btn"
                  style={{ alignSelf: p[0] === "t" ? "start" : "end" }}
                  aria-pressed={s.numbering.position === p}
                  aria-label={t(`edit.positions.${p}`)}
                  title={t(`edit.positions.${p}`)}
                  onClick={() => set(tab.key, (x) => ({ ...x, numbering: { ...x.numbering, position: p, on: true } }))}
                >
                  <span className="epos__bar" />
                </button>
              ))}
            </div>
          </div>
        </Section>

        <Section id="wm" title={t("edit.sections.wm")} tag={s.watermark.text.trim() ? t("edit.on") : undefined} open={open.wm} onToggle={() => toggle("wm")}>
          <label className="eside__field">
            <span className="muted">{t("edit.wmText")}</span>
            <TextField value={s.watermark.text} placeholder={t("edit.wmNone")} onChange={(e) => set(tab.key, (x) => ({ ...x, watermark: { ...x.watermark, text: e.target.value } }), "wm")} />
          </label>
          <div className="eside__field">
            <span className="eside__row">
              <span>{t("edit.opacity")}</span>
            </span>
            <Slider label={t("edit.wmOpacity")} min={5} max={60} value={s.watermark.opacity} format={(v) => `${v} %`} onChange={(v) => set(tab.key, (x) => ({ ...x, watermark: { ...x.watermark, opacity: v } }), "wmop")} />
          </div>
          <div className="eside__row">
            <span>{t("edit.color")}</span>
            <ColorDots size="sm" label={t("edit.color")} presets={WM_COLORS} value={s.watermark.color} onChange={(c) => c && set(tab.key, (x) => ({ ...x, watermark: { ...x.watermark, color: c } }))} />
          </div>
        </Section>

        <Section id="crop" title={t("edit.sections.crop")} tag={s.crop.on ? t("edit.on") : undefined} open={open.crop} onToggle={() => toggle("crop")}>
          <div className="eside__row">
            <span>{t("edit.cropOn")}</span>
            <Switch label={t("edit.cropOn")} checked={s.crop.on} onChange={(v) => set(tab.key, (x) => ({ ...x, crop: { ...x.crop, on: v } }))} />
          </div>
          <div className="eside__grid2">
            {(["top", "bottom", "left", "right"] as const).map((k) => (
              <label key={k} className="eside__field">
                <span className="muted">{t(`edit.${k}`)}</span>
                <TextField
                  type="number"
                  min={0}
                  max={80}
                  className="tnum"
                  value={s.crop[k]}
                  suffix="mm"
                  onChange={(e) => set(tab.key, (x) => ({ ...x, crop: { ...x.crop, [k]: Math.max(0, Math.min(80, Number(e.target.value) || 0)), on: true } }), `crop-${k}`)}
                />
              </label>
            ))}
          </div>
          <div className="eside__row">
            <span>{t("edit.cropScope")}</span>
            <Segmented
              size="sm"
              label={t("edit.cropScope")}
              value={s.crop.scope}
              onChange={(v) => set(tab.key, (x) => ({ ...x, crop: { ...x.crop, scope: v } }))}
              options={[
                { value: "all", label: t("edit.cropAll") },
                { value: "sel", label: t("edit.cropSel") },
              ]}
            />
          </div>
        </Section>

        <Section id="red" title={t("edit.sections.red")} tag={t("edit.irreversible")} danger open={open.red} onToggle={() => toggle("red")}>
          <p className="muted eside__p">{t("edit.redIntro")}</p>
          <div className="eside__chips">
            {patterns.map((p) => (
              <Chip
                key={p}
                pressed={d.redact.patterns.includes(p)}
                onClick={() =>
                  store.patch(tab.key, {
                    redact: { ...d.redact, step: "idle", patterns: d.redact.patterns.includes(p) ? d.redact.patterns.filter((x) => x !== p) : [...d.redact.patterns, p] },
                  })
                }
              >
                {t(`edit.patterns.${p}`)}
              </Chip>
            ))}
          </div>
          <label className="eside__field">
            <span className="muted">{t("edit.terms")}</span>
            <TextField value={d.redact.terms} placeholder={t("edit.termsPh")} onChange={(e) => store.patch(tab.key, { redact: { ...d.redact, terms: e.target.value, step: "idle" } })} />
          </label>
          <div className={`ered ered--${status.tone}`} role="status">
            <span className="ered__ic">{status.icon === "ok" ? <Icons.check /> : status.icon === "scan" ? <span className="spinner" /> : <Icons.info />}</span>
            <span>{status.msg}</span>
          </div>
          <Button variant="danger" className="ered__btn" disabled={!canScan || scanning} onClick={() => void store.findRedactions(tab.key)}>
            {scanning ? t("edit.redBusy") : d.redact.step === "done" ? t("edit.redAgain") : t("edit.redApply")}
          </Button>
          {marks > 0 && (
            <Button variant="ghost" size="sm" onClick={() => store.commit(tab.key, (x) => ({ pages: x.pages.map((p) => (p.redact.length ? { ...p, redact: [] } : p)) }))}>
              {t("edit.redClear")}
            </Button>
          )}
          <p className="muted eside__p">{t("edit.redManual")}</p>
        </Section>
      </fieldset>
    </aside>
  );
}

// ---------------------------------------------------------------- élément sélectionné (mode Contenu)

function ContentPanel({ tab, d, readOnly }: { tab: DocTab; d: DocEdit; readOnly: boolean }) {
  const { t } = useTranslation();
  const store = useEdit();
  const { sel, items, set } = useContentSel();
  const item = resolveItem(items, sel);
  const page = d.contentPage;
  const apply = (op: Parameters<typeof store.applyContent>[1]) => void store.applyContent(tab.key, op);

  let title = t("edit.nothingSelected");
  let body: ReactNode = null;
  if (item?.kind === "text") {
    title = t("edit.textBlock");
    body = <TextPanel key={`${item.rect.x},${item.rect.y}`} item={item} disabled={readOnly} onApply={(spec) => apply({ op: "replaceText", page, ids: item.ids, spec })} />;
  } else if (item?.kind === "image") {
    title = t("edit.image");
    body = (
      <>
        <div className="eside__grid2">
          <Button
            disabled={readOnly}
            onClick={() =>
              void pickImage(t("edit.pickImage")).then((path) => path && apply({ op: "imageReplace", page, id: item.ids[0], path }))
            }
          >
            {t("edit.replace")}
          </Button>
          <Button disabled={readOnly} onClick={() => set({ crop: { ...item.rect } })}>
            {t("edit.crop")}
          </Button>
          <Button disabled={readOnly} onClick={() => apply({ op: "imageRotate", page, id: item.ids[0] })}>
            {t("edit.imgRotate")}
          </Button>
          <Button
            variant="danger"
            disabled={readOnly}
            onClick={() => {
              apply({ op: "delete", page, ids: item.ids });
              set({ sel: null });
            }}
          >
            {t("edit.delete")}
          </Button>
        </div>
        {item.px && <span className="muted">{t("edit.imgInfo", { w: item.px[0], h: item.px[1] })}</span>}
      </>
    );
  } else if (sel?.kind === "link") {
    const l = linkList(d, page).find((x) => x.id === sel.id);
    if (l) {
      title = t("edit.link");
      body = <LinkPanel key={l.id} tab={tab} d={d} linkId={l.id} uri={l.uri ?? ""} target={l.page} disabled={readOnly} />;
    }
  }
  return (
    <section className="ecpanel slide">
      <h2 className="eside__h">{title}</h2>
      {body}
    </section>
  );
}

function TextPanel({ item, disabled, onApply }: { item: ContentItem; disabled: boolean; onApply: (spec: ReturnType<typeof specOf>) => void }) {
  const { t } = useTranslation();
  const [text, setText] = useState(item.text);
  useEffect(() => setText(item.text), [item.text]);
  const base = specOf(item);
  const change = (patch: Partial<typeof base>) => onApply({ ...base, text, ...patch });
  return (
    <>
      <label className="eside__field">
        <span className="muted">{t("edit.text")}</span>
        <textarea
          className="field eside__area"
          value={text}
          disabled={disabled}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => text !== item.text && change({})}
        />
      </label>
      <div className="eside__row">
        <Segmented
          size="sm"
          label={t("annot.font")}
          value={item.font}
          onChange={(font) => change({ font })}
          options={(["sans", "serif", "mono"] as const).map((f) => ({ value: f, label: t(`annot.fonts.${f}`), title: t(`annot.fontsLong.${f}`) }))}
        />
        <span className="eside__inline">
          <IconButton label={t("annot.bold")} icon={<Icons.bold />} on={item.bold} disabled={disabled} onClick={() => change({ bold: !item.bold })} />
          <IconButton label={t("annot.italic")} icon={<Icons.italic />} on={item.italic} disabled={disabled} onClick={() => change({ italic: !item.italic })} />
        </span>
      </div>
      <div className="eside__row">
        <span>{t("annot.fontSize")}</span>
        <Stepper value={Math.round(item.size)} min={4} max={96} onChange={(size) => change({ size })} decLabel={t("annot.smaller")} incLabel={t("annot.bigger")} />
      </div>
      <div className="eside__row">
        <span>{t("annot.align")}</span>
        <Segmented
          size="sm"
          label={t("annot.align")}
          value={String(item.align) as "0" | "1" | "2"}
          onChange={(v) => change({ align: Number(v) })}
          options={[
            { value: "0", icon: <Icons.alignLeft />, ariaLabel: t("annot.aligns.left") },
            { value: "1", icon: <Icons.alignCenter />, ariaLabel: t("annot.aligns.center") },
            { value: "2", icon: <Icons.alignRight />, ariaLabel: t("annot.aligns.right") },
          ]}
        />
      </div>
      <div className="eside__row">
        <span>{t("annot.textColor")}</span>
        <ColorDots size="sm" label={t("annot.textColor")} presets={TEXT_COLORS} value={item.color} onChange={(c) => c && change({ color: c })} />
      </div>
    </>
  );
}

function LinkPanel({ tab, d, linkId, uri, target, disabled }: { tab: DocTab; d: DocEdit; linkId: string; uri: string; target: number | null; disabled: boolean }) {
  const { t } = useTranslation();
  const store = useEdit();
  const [value, setValue] = useState(uri);
  const page = d.contentPage;
  const commit = () => {
    if (value.trim() === uri.trim() || !value.trim()) return;
    store.editLinks(tab.key, page, (l) => {
      if (linkId.startsWith("a")) {
        const i = Number(linkId.slice(1));
        return l.map((e, k) => (k === i && e.op === "add" ? { ...e, uri: value.trim() } : e));
      }
      const index = Number(linkId.slice(1));
      return [...l.filter((e) => !(e.op === "update" && e.index === index)), { op: "update", index, uri: value.trim() }];
    });
  };
  const remove = () => {
    store.editLinks(tab.key, page, (l) => {
      if (linkId.startsWith("a")) {
        const i = Number(linkId.slice(1));
        return l.filter((_, k) => k !== i);
      }
      const index = Number(linkId.slice(1));
      return [...l.filter((e) => !(e.op === "update" && e.index === index)), { op: "delete", index }];
    });
    useContentSel.getState().set({ sel: null });
  };
  return (
    <>
      {target !== null && !uri ? (
        <span className="muted">{t("edit.linkTarget", { n: target + 1 })}</span>
      ) : (
        <label className="eside__field">
          <span className="muted">{t("edit.address")}</span>
          <TextField
            autoFocus={!uri}
            value={value}
            disabled={disabled}
            placeholder={t("edit.addressPh")}
            onChange={(e) => setValue(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
          />
        </label>
      )}
      <Button variant="danger" disabled={disabled} icon={<Icons.trash />} onClick={remove}>
        {t("edit.linkDelete")}
      </Button>
    </>
  );
}
