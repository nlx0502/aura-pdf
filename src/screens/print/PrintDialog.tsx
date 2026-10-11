import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { Button, Chip, IconButton, Icons, Modal, ModalHeader, ProgressBar, Segmented, Slider, Stepper, Switch, TextField, toast } from "../../components";
import { asAppError } from "../../lib/api";
import { Prio, renderPage } from "../../lib/render";
import { stripExt } from "../../lib/format";
import { shortcut } from "../../lib/platform";
import {
  defaultPrintSettings,
  physicalSheets,
  printApi,
  type PrinterInfo,
  type PrintLayout,
  type PrintSettings,
  type Sheet,
} from "../../lib/print";
import { useActiveTab, useApp } from "../../stores/app";
import { useAnnots } from "../../stores/annot";
import "./print.css";

const PDF = "__pdf__";

type Phase = { kind: "idle" } | { kind: "running"; job: number; phase: "compose" | "send" | "save"; done: number; total: number } | { kind: "done"; label: string; path?: string };

/** Fenêtre d'impression complète (Ctrl+P), accessible depuis tous les écrans. */
export function PrintDialog() {
  const { t } = useTranslation();
  const open = useApp((s) => s.printOpen);
  const set = useApp((s) => s.set);
  const tab = useActiveTab();

  // Sans document ouvert, rien à imprimer.
  useEffect(() => {
    if (open && !tab) {
      set({ printOpen: false });
      toast.info(t("print.needDoc"));
    }
  }, [open, tab, set, t]);

  if (!open || !tab) return null;
  return <PrintDialogInner key={tab.key} />;
}

function PrintDialogInner() {
  const { t } = useTranslation();
  const tab = useActiveTab()!;
  const set = useApp((s) => s.set);
  const openPath = useApp((s) => s.openPath);
  const savedPrefs = useApp((s) => s.prefs as unknown as { print?: Partial<PrintSettings> & { printer?: string } });
  const setPref = useApp((s) => s.setPref as unknown as (k: string, v: unknown) => void);

  const [s, setS] = useState<PrintSettings>(() => ({ ...defaultPrintSettings, ...(savedPrefs.print ?? {}), range: "all", current: tab.page }));
  const [printers, setPrinters] = useState<PrinterInfo[] | null>(null);
  const [printer, setPrinter] = useState<string>(savedPrefs.print?.printer ?? "");
  const [margin, setMargin] = useState(0);
  const [layout, setLayout] = useState<PrintLayout | null>(null);
  const [sheetIdx, setSheetIdx] = useState(0);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const jobRef = useRef(0);

  const up = (patch: Partial<PrintSettings>) => setS((cur) => ({ ...cur, ...patch }));
  const unsavedAnnots = useAnnots((st) => !!st.docs[tab.key]?.dirty);
  const isPdf = printer === PDF;
  const printerInfo = printers?.find((p) => p.name === printer);
  const sizes = useMemo(() => tab.info.pages.map((p) => [p.w, p.h] as [number, number]), [tab.info.pages]);

  // ---- Imprimantes Windows
  useEffect(() => {
    void printApi
      .printers()
      .then((list) => {
        setPrinters(list);
        setPrinter((cur) => (cur === PDF || list.some((p) => p.name === cur) ? cur : (list.find((p) => p.isDefault)?.name ?? list[0]?.name ?? PDF)));
      })
      .catch(() => {
        setPrinters([]);
        setPrinter(PDF);
      });
  }, []);

  // ---- Marges non imprimables de l'imprimante (0 pour un fichier PDF)
  useEffect(() => {
    if (!printer || isPdf) {
      setMargin(0);
      return;
    }
    void printApi
      .margins(printer, s.paper, false)
      .then((m) => setMargin(m.margin))
      .catch(() => setMargin(12));
  }, [printer, isPdf, s.paper]);

  // ---- Imposition (calculée par le cœur Rust)
  useEffect(() => {
    let alive = true;
    void printApi.layout(s, sizes, margin).then((l) => {
      if (!alive) return;
      setLayout(l);
    });
    return () => {
      alive = false;
    };
  }, [s, sizes, margin]);

  // ---- Progression de la tâche
  useEffect(() => {
    const un: (() => void)[] = [];
    let disposed = false;
    const keep = (u: () => void) => (disposed ? u() : un.push(u));
    void listen<{ job: number; phase: "compose" | "send"; done: number; total: number }>("print:progress", (e) => {
      if (e.payload.job !== jobRef.current) return;
      setPhase({ kind: "running", job: e.payload.job, phase: e.payload.phase, done: e.payload.done, total: e.payload.total });
    }).then(keep);
    void listen<{ job: number; ok: boolean; cancelled?: boolean; message?: string; path?: string; printer?: string }>("print:done", (e) => {
      const p = e.payload;
      if (p.job !== jobRef.current) return;
      if (p.ok) {
        const label = p.path ? t("print.savedPdf") : t("print.sentTo", { printer: p.printer ?? "" });
        setPhase({ kind: "done", label, path: p.path });
        toast.success(label);
        if (!p.path) window.setTimeout(() => set({ printOpen: false }), 1600);
      } else {
        setPhase({ kind: "idle" });
        if (p.cancelled) toast.info(t("print.cancelled"));
        else toast.error(t("print.failed", { message: p.message ?? "" }));
      }
    }).then(keep);
    return () => {
      disposed = true;
      un.forEach((u) => u());
    };
  }, [t, set]);

  const running = phase.kind === "running";
  const close = () => {
    if (running) return;
    set({ printOpen: false });
  };

  // ---- Feuilles de l'aperçu (affiche : une vue par page source)
  const sheets = layout?.sheets ?? [];
  const posterPages = useMemo(() => (s.mode === "poster" ? [...new Set(sheets.map((sh) => sh.items[0]?.page ?? 0))] : []), [s.mode, sheets]);
  const summaryExtra = s.summary ? 1 : 0;
  const viewCount = Math.max(1, (s.mode === "poster" ? posterPages.length : sheets.length) + summaryExtra);
  const si = Math.min(sheetIdx, viewCount - 1);
  useEffect(() => setSheetIdx(0), [s.mode, s.range, s.custom, s.subset, s.reverse, s.perSheet]);

  const physical = layout ? physicalSheets(layout, s, summaryExtra) : 0;
  const badRange = !!layout?.error;

  const doPrint = async () => {
    if (!layout || layout.error) return;
    let target: { kind: "pdf"; path: string } | { kind: "printer"; name: string };
    if (isPdf) {
      // En développement, les scripts de capture fournissent le chemin (pas de fenêtre native).
      const testPath = import.meta.env.DEV ? (window as unknown as { __auraSavePath?: string }).__auraSavePath : undefined;
      const path = testPath ?? (await save({ defaultPath: `${stripExt(tab.info.name)} (impression).pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] }));
      if (!path) return;
      target = { kind: "pdf", path };
    } else {
      target = { kind: "printer", name: printer };
    }
    const job = Date.now();
    jobRef.current = job;
    setPhase({ kind: "running", job, phase: isPdf ? "save" : "compose", done: 0, total: layout.sides });
    // Mémoriser les réglages pour la prochaine fois (hors plage de pages).
    const { range: _r, current: _c, custom: _cu, ...keepSettings } = s;
    setPref("print", { ...keepSettings, printer });
    try {
      await printApi.start(tab.info.id, job, s, margin, target);
    } catch (e) {
      setPhase({ kind: "idle" });
      toast.error(asAppError(e).message);
    }
  };

  const progressLabel = (() => {
    if (phase.kind !== "running") return "";
    if (phase.phase === "send") return t("print.sending", { n: Math.max(1, Math.ceil((phase.done / Math.max(1, phase.total)) * physical)), total: physical });
    if (isPdf && phase.done >= phase.total) return t("print.saving");
    return t("print.composing", { n: Math.max(1, phase.done), total: phase.total });
  })();

  const noteKey =
    s.mode === "poster"
      ? t("print.notePoster", { count: sheets.filter((sh) => sh.items[0]?.page === posterPages[0]).length })
      : s.mode === "booklet"
        ? t("print.noteBooklet")
        : s.mode === "multi"
          ? t("print.noteMulti", { count: s.perSheet })
          : t("print.noteSize", { paper: s.paper === "letter" ? t("print.letter") : s.paper.toUpperCase() });

  const summaryLine = layout
    ? [
        t("print.sheets", { count: physical }),
        t("print.copiesCount", { count: s.copies }),
        t("print.pagesCount", { count: layout.pages.length }),
        ...(s.duplex && !isPdf ? [t("print.duplexShort")] : []),
      ].join(" · ")
    : "";

  return (
    <Modal open onClose={close} label={t("print.title")} width={1020} height="min(800px, 100%)" locked={running}>
      <ModalHeader icon={<Icons.print />} title={t("print.title")} subtitle={t("print.docLine", { name: tab.info.name, count: tab.info.pageCount })} onClose={running ? undefined : close} />
      <div className="pdlg">
        {/* ---------------------------------------------------------------- aperçu */}
        <div className="pdlg__preview">
          <div className="pdlg__stage">
            {layout && (
              <PreviewSheet
                doc={tab.info.id}
                settings={s}
                layout={layout}
                index={si}
                posterPages={posterPages}
                margin={margin}
                summary={s.summary && si === viewCount - 1}
                feeding={running && phase.kind === "running" && phase.phase === "send"}
              />
            )}
          </div>
          <div className="pdlg__nav">
            <IconButton size="sm" tip={false} label={t("print.prevSheet")} icon={<Icons.chevronLeft />} disabled={si === 0} onClick={() => setSheetIdx(si - 1)} />
            <span className="pdlg__navlabel tnum">
              {t("print.sheetOf", { n: si + 1, total: viewCount })}
              {s.duplex && s.mode !== "poster" ? (si % 2 ? t("print.verso") : t("print.recto")) : ""}
            </span>
            <IconButton size="sm" tip={false} label={t("print.nextSheet")} icon={<Icons.chevronRight />} disabled={si >= viewCount - 1} onClick={() => setSheetIdx(si + 1)} />
          </div>
          <span className="pdlg__note">{isPdf ? t("print.notePdf") : noteKey}</span>
          {unsavedAnnots && <span className="pnote pnote--warn" style={{ textAlign: "center", maxWidth: 420 }}>{t("annot.printUnsaved")}</span>}
        </div>

        {/* ---------------------------------------------------------------- réglages */}
        <div className="pdlg__settings">
          <section className="psec" style={{ borderTop: 0 }}>
            <label className="psec__h" htmlFor="print-printer">
              {t("print.printer")}
            </label>
            <span className="pselect">
              <select id="print-printer" className="field" value={printer} onChange={(e) => setPrinter(e.target.value)} disabled={running || printers === null}>
                {printers === null && <option value="">{t("print.loadingPrinters")}</option>}
                {printers?.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name}
                  </option>
                ))}
                <option value={PDF}>{t("print.pdfPrinter")}</option>
              </select>
              <Icons.chevronDown className="i pselect__chev" />
            </span>
            <span className={`pstatus pstatus--${isPdf ? "pdf" : (printerInfo?.level ?? "ok")}`}>
              <span className="pstatus__dot" />
              {isPdf ? t("print.pdfStatus") : (printerInfo?.status ?? (printers?.length === 0 ? t("print.noPrinter") : ""))}
            </span>
            <div className="prow" style={{ marginTop: 4 }}>
              <span>{t("print.copies")}</span>
              <span style={{ display: "flex", alignItems: "center", gap: 14 }}>
                <span className="prow__inline">
                  {t("print.collate")}
                  <Switch label={t("print.collateLabel")} checked={s.collate} onChange={(v) => up({ collate: v })} disabled={isPdf || s.copies < 2} />
                </span>
                <Stepper value={isPdf ? 1 : s.copies} min={1} max={99} onChange={(v) => up({ copies: v })} decLabel={t("print.fewerCopies")} incLabel={t("print.moreCopies")} />
              </span>
            </div>
          </section>

          <section className="psec">
            <span className="psec__h">{t("print.pages")}</span>
            <Segmented
              block
              label={t("print.pages")}
              value={s.range}
              onChange={(v) => up({ range: v, current: tab.page })}
              options={(["all", "current", "custom"] as const).map((r) => ({ value: r, label: t(`print.ranges.${r}`) }))}
            />
            {s.range === "custom" && (
              <TextField
                className="rise-sm"
                value={s.custom}
                invalid={badRange}
                placeholder={t("print.customPlaceholder")}
                aria-label={t("print.customLabel")}
                onChange={(e) => up({ custom: e.target.value })}
              />
            )}
            {badRange && (
              <span className="pnote pnote--warn rise-sm" role="alert">
                {layout?.error}
              </span>
            )}
            <div className="pchips">
              {(["all", "odd", "even"] as const).map((v) => (
                <Chip key={v} dialog pressed={s.subset === v} onClick={() => up({ subset: v })}>
                  {t(`print.subsets.${v}`)}
                </Chip>
              ))}
              <span style={{ flex: 1 }} />
              <span className="prow__inline">
                {t("print.reverse")}
                <Switch label={t("print.reverse")} checked={s.reverse} onChange={(v) => up({ reverse: v })} />
              </span>
            </div>
          </section>

          <section className="psec">
            <span className="psec__h">{t("print.handling")}</span>
            <Segmented
              block
              label={t("print.handling")}
              value={s.mode}
              onChange={(v) => up(v === "booklet" ? { mode: v, duplex: !isPdf && (printerInfo?.duplex ?? true), shortEdge: true } : { mode: v })}
              options={(["size", "poster", "multi", "booklet"] as const).map((m) => ({ value: m, label: t(`print.modes.${m}`) }))}
            />
            {s.mode === "size" && (
              <div className="pgroup rise-sm">
                <div className="pchips">
                  {(["fit", "actual", "shrink", "custom"] as const).map((f) => (
                    <Chip key={f} dialog pressed={s.fit === f} onClick={() => up({ fit: f })}>
                      {t(`print.fits.${f}`)}
                    </Chip>
                  ))}
                </div>
                {s.fit === "custom" && <Slider label={t("print.scale")} value={s.scale} min={30} max={200} step={5} onChange={(v) => up({ scale: v })} format={(v) => `${v} %`} />}
              </div>
            )}
            {s.mode === "multi" && (
              <div className="pgroup rise-sm">
                <div className="prow">
                  <span className="prow__label">{t("print.perSheet")}</span>
                  <span className="pchips">
                    {[2, 4, 6, 9].map((n) => (
                      <Chip key={n} dialog className="pchip--sq" pressed={s.perSheet === n} onClick={() => up({ perSheet: n })}>
                        {n}
                      </Chip>
                    ))}
                  </span>
                </div>
                <div className="prow">
                  <span className="prow__label">{t("print.borders")}</span>
                  <Switch label={t("print.borders")} checked={s.borders} onChange={(v) => up({ borders: v })} />
                </div>
              </div>
            )}
            {s.mode === "booklet" && (
              <div className="prow rise-sm">
                <span className="prow__label">{t("print.binding")}</span>
                <span className="pchips">
                  <Chip dialog pressed={!s.bindRight} onClick={() => up({ bindRight: false })}>
                    {t("print.bindLeft")}
                  </Chip>
                  <Chip dialog pressed={s.bindRight} onClick={() => up({ bindRight: true })}>
                    {t("print.bindRight")}
                  </Chip>
                </span>
              </div>
            )}
            {s.mode === "poster" && (
              <div className="pgroup rise-sm">
                <div className="prow">
                  <span className="prow__label" style={{ whiteSpace: "nowrap" }}>
                    {t("print.tile")}
                  </span>
                  <span style={{ flex: 1, maxWidth: 260 }}>
                    <Slider label={t("print.tile")} value={s.tile} min={125} max={400} step={25} onChange={(v) => up({ tile: v })} format={(v) => `${v} %`} />
                  </span>
                </div>
                <div className="prow">
                  <span className="prow__label">{t("print.cutMarks")}</span>
                  <Switch label={t("print.cutMarks")} checked={s.cutMarks} onChange={(v) => up({ cutMarks: v })} />
                </div>
              </div>
            )}
            <div className="prow" style={{ marginTop: 2 }}>
              <span className="prow__label">{t("print.paper")}</span>
              <span className="pchips">
                {(["a4", "a3", "letter"] as const).map((p) => (
                  <Chip key={p} dialog pressed={s.paper === p} onClick={() => up({ paper: p })}>
                    {p === "letter" ? t("print.letter") : p.toUpperCase()}
                  </Chip>
                ))}
              </span>
            </div>
            <div className="prow">
              <span className="prow__label">{t("print.orientation")}</span>
              <span style={{ width: 250 }}>
                <Segmented
                  block
                  size="sm"
                  label={t("print.orientation")}
                  value={s.orient}
                  onChange={(v) => up({ orient: v })}
                  options={(["auto", "portrait", "landscape"] as const).map((o) => ({ value: o, label: t(`print.orients.${o}`) }))}
                />
              </span>
            </div>
          </section>

          <section className="psec">
            <div className="prow">
              <span className="psec__h">{t("print.duplex")}</span>
              <Switch label={t("print.duplex")} checked={s.duplex && !isPdf} onChange={(v) => up({ duplex: v })} disabled={isPdf || printerInfo?.duplex === false} />
            </div>
            {printerInfo?.duplex === false && !isPdf && <span className="pnote">{t("print.noDuplex")}</span>}
            {s.duplex && !isPdf && (
              <div className="pchips rise-sm">
                <Chip dialog pressed={!s.shortEdge} onClick={() => up({ shortEdge: false })}>
                  {t("print.flipLong")}
                </Chip>
                <Chip dialog pressed={s.shortEdge} onClick={() => up({ shortEdge: true })}>
                  {t("print.flipShort")}
                </Chip>
              </div>
            )}
            <div className="prow">
              <span className="prow__label">{t("print.color")}</span>
              <span style={{ width: 250 }}>
                <Segmented
                  block
                  size="sm"
                  label={t("print.color")}
                  value={s.gray ? "gray" : "color"}
                  onChange={(v) => up({ gray: v === "gray" })}
                  options={[
                    { value: "color", label: t("print.colorOn") },
                    { value: "gray", label: t("print.gray") },
                  ]}
                />
              </span>
            </div>
          </section>

          <section className="psec">
            <span className="psec__h">{t("print.comments")}</span>
            <div className="pchips">
              {(["doc", "docann", "stamps"] as const).map((c) => (
                <Chip key={c} dialog pressed={s.comments === c} onClick={() => up({ comments: c })}>
                  {t(`print.commentModes.${c}`)}
                </Chip>
              ))}
            </div>
            <div className="prow">
              <span className="prow__label">{t("print.summary")}</span>
              <Switch label={t("print.summary")} checked={s.summary} onChange={(v) => up({ summary: v })} />
            </div>
          </section>
        </div>
      </div>

      {/* ---------------------------------------------------------------- pied */}
      <div className="dialog__foot pdlg__foot">
        {phase.kind === "idle" && <span className="pnote tnum" style={{ flex: 1 }}>{summaryLine}</span>}
        {phase.kind === "running" && (
          <div className="rise-sm" style={{ flex: 1, display: "flex", flexDirection: "column", gap: 7 }}>
            <span className="pnote tnum">{progressLabel}</span>
            <ProgressBar value={(phase.done / Math.max(1, phase.total)) * 100} />
          </div>
        )}
        {phase.kind === "done" && (
          <div className="rise-sm pdone">
            <span className="pdone__ok">
              <Icons.check size={13} strokeWidth={2.4} />
            </span>
            <span style={{ fontWeight: 500, fontSize: 13.5 }}>{phase.label}</span>
            {phase.path && (
              <Button
                size="sm"
                onClick={() => {
                  set({ printOpen: false });
                  void openPath(phase.path!);
                }}
              >
                {t("print.open")}
              </Button>
            )}
          </div>
        )}
        {running ? (
          <Button variant="ghost" onClick={() => void printApi.cancel(jobRef.current)}>
            {t("common.cancel")}
          </Button>
        ) : (
          <Button variant="ghost" onClick={close}>
            {phase.kind === "done" ? t("common.close") : t("common.cancel")}
          </Button>
        )}
        <Button variant="primary" style={{ minWidth: 120 }} onClick={() => void doPrint()} disabled={!layout || badRange || running || phase.kind === "done" || !printer} kbd={phase.kind === "idle" ? shortcut("P") : undefined}>
          {running ? t("print.printing") : isPdf ? t("print.submitPdf") : t("print.submit")}
        </Button>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------- aperçu d'une feuille

function PreviewSheet({
  doc,
  settings,
  layout,
  index,
  posterPages,
  summary,
  feeding,
  margin,
}: {
  doc: number;
  margin: number;
  settings: PrintSettings;
  layout: PrintLayout;
  index: number;
  posterPages: number[];
  summary: boolean;
  feeding: boolean;
}) {
  const { t } = useTranslation();
  const annots = settings.comments !== "doc";
  const long = settings.paper === "a3" ? 410 : 400;

  if (summary || !layout.sheets.length) {
    const ref = layout.sheets[0];
    const portrait = !ref || ref.h >= ref.w;
    const [w, h] = portrait ? [Math.round(long / 1.414), long] : [long, Math.round(long / 1.414)];
    return (
      <div className={`psheet ${feeding ? "psheet--feeding" : ""}`} style={{ width: w, height: h }}>
        {summary && (
          <div className="psummary">
            <b>{t("print.summarySheet")}</b>
            <span className="pln" style={{ width: "60%" }} />
            <span className="pln" style={{ width: "92%", marginTop: 10 }} />
            <span className="pln" style={{ width: "84%" }} />
            <span className="pln" style={{ width: "88%", marginTop: 10 }} />
            <span className="pln" style={{ width: "70%" }} />
          </div>
        )}
      </div>
    );
  }

  // Affiche : la page entière, quadrillée par les tuiles qui la composent.
  if (settings.mode === "poster") {
    const page = posterPages[index] ?? posterPages[0];
    const tiles = layout.sheets.filter((sh) => sh.items[0]?.page === page);
    const grid = tiles[0]?.tile ?? [0, 0, 1, 1];
    const item = tiles[0]?.items[0];
    const ratio = item ? item.w / item.h : 0.707;
    // Tuiles proportionnelles : zone imprimable pleine, dernière rangée/colonne partielle.
    const sheet0 = tiles[0];
    const track = (count: number, full: number, total: number) =>
      Array.from({ length: count }, (_, i) => `${Math.max(0.0001, i < count - 1 ? full : total - full * (count - 1))}fr`).join(" ");
    const cols = sheet0 && item ? track(grid[2], sheet0.w - 2 * margin, item.w) : `repeat(${grid[2]}, 1fr)`;
    const rows = sheet0 && item ? track(grid[3], sheet0.h - 2 * margin, item.h) : `repeat(${grid[3]}, 1fr)`;
    const [w, h] = ratio < 1 ? [Math.round(long * ratio), long] : [long, Math.round(long / ratio)];
    return (
      <div className={`psheet psheet--poster ${feeding ? "psheet--feeding" : ""}`} style={{ width: w, height: h, filter: settings.gray ? "grayscale(1)" : "none" }}>
        <PreviewPage doc={doc} page={page} width={w} annots={annots} />
        <div className="ptiles" aria-hidden="true" style={{ gridTemplateColumns: cols, gridTemplateRows: rows }}>
          {tiles.map((_, i) => (
            <span key={i} className="ptile" style={{ animationDelay: `${i * 40}ms` }}>
              {i + 1}
            </span>
          ))}
        </div>
      </div>
    );
  }

  const sheet: Sheet = layout.sheets[Math.min(index, layout.sheets.length - 1)];
  const k = long / Math.max(sheet.w, sheet.h);
  return (
    <div
      className={`psheet ${feeding ? "psheet--feeding" : ""}`}
      style={{ width: Math.round(sheet.w * k), height: Math.round(sheet.h * k), filter: settings.gray ? "grayscale(1)" : "none" }}
    >
      {sheet.items.map((it, i) => (
        <div
          key={`${index}:${i}:${it.page}`}
          className={`pmini ${settings.mode === "multi" && !settings.borders ? "" : "pmini--edge"}`}
          style={{ left: it.x * k, top: it.y * k, width: it.w * k, height: it.h * k, animationDelay: `${i * 45}ms` }}
        >
          <PreviewPage doc={doc} page={it.page} width={it.w * k} annots={annots} />
        </div>
      ))}
      {sheet.frames.map((f, i) => (
        <span key={`f${i}`} className="pframe" style={{ left: f[0] * k, top: f[1] * k, width: f[2] * k, height: f[3] * k }} />
      ))}
      {sheet.blank && <span className="pblank">{t("print.blankPage")}</span>}
    </div>
  );
}

const PreviewPage = memo(function PreviewPage({ doc, page, width, annots }: { doc: number; page: number; width: number; annots: boolean }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let alive = true;
    const w = Math.max(32, Math.round((width * (window.devicePixelRatio || 1)) / 8) * 8);
    const timer = window.setTimeout(async () => {
      const res = await renderPage({ doc, page, width: w, rot: 0, annots, forms: true, prio: Prio.THUMB, gen: 0 });
      if (!alive || res.kind !== "ok" || !ref.current) return;
      ref.current.width = res.bitmap.width;
      ref.current.height = res.bitmap.height;
      ref.current.getContext("2d")?.drawImage(res.bitmap, 0, 0);
      setReady(true);
    }, 30);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [doc, page, width, annots]);
  return <canvas ref={ref} className="ppage" style={{ opacity: ready ? 1 : 0 }} aria-hidden="true" />;
});
