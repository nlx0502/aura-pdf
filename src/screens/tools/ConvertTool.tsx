/**
 * Outil « Convertir et OCR » (maquette Convert.dc.html) : flux animé du format
 * source au format cible, formats, file de fichiers avec progression ; OCR
 * avec balayage de la page et texte reconnu ligne par ligne.
 */
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath as openExternal, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button, Chip, Icons, Segmented, Switch } from "../../components";
import { badgeOf, extOf, FROM, LANGS, TO, type FormatCard } from "../../lib/convert";
import { formatSize } from "../../lib/format";
import { Prio, renderPage } from "../../lib/render";
import { useActiveTab, useApp } from "../../stores/app";
import { useConvert } from "../../stores/convert";
import "./tools.css";
import "./convert.css";

const D_SHIELD = "M8 1.5l5 2v4c0 3.2-2.2 5.6-5 7-2.8-1.4-5-3.8-5-7v-4z";
const D_SCAN = "M2 5V2h3 M14 5V2h-3 M2 11v3h3 M14 11v3h-3 M5 6h6 M5 8h6 M5 10h4";

function Badge({ text, bg, fg, small }: { text: string; bg: string; fg: string; small?: boolean }) {
  return (
    <span className={`cbadge ${small ? "cbadge--sm" : ""}`} style={{ background: bg, color: fg }}>
      {text}
    </span>
  );
}

export function ConvertTool() {
  const { t } = useTranslation();
  const s = useConvert();
  const tab = useActiveTab();
  useEffect(() => {
    void s.loadInfo();
    // Le document actif est proposé d'office dans la file « depuis un PDF ».
    if (tab && s.dir === "from" && !s.files.length && /\.pdf$/i.test(tab.info.path)) s.addFiles([tab.info.path]);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const conv = s.tab === "conv";
  return (
    <div className="workspace toolw">
      <main className="panel tool__main">
        <div className="tool__col tool__col--wide cvt">
          <div className="tool__head">
            <header className="tool__title">
              <h1>{conv ? t("convert.title") : t("convert.ocrTitle")}</h1>
              <span className="muted">{conv ? t("convert.sub") : t("convert.ocrSub")}</span>
              <span className="cvt__local">
                <svg className="i" width="14" height="14" aria-hidden="true">
                  <path d={D_SHIELD} />
                </svg>
                {t("convert.local")}
              </span>
            </header>
            <Segmented
              label={t("convert.tool")}
              value={s.tab}
              itemWidth={130}
              options={[
                { value: "conv", label: t("convert.tabConvert") },
                { value: "ocr", label: t("convert.tabOcr") },
              ]}
              onChange={(v) => !s.running && s.set({ tab: v })}
            />
          </div>
          {conv ? <ConvertPane /> : <OcrPane />}
        </div>
      </main>
      <Side />
    </div>
  );
}

function ConvertPane() {
  const { t } = useTranslation();
  const s = useConvert();
  const list = s.dir === "from" ? FROM : TO.filter((f) => f.id !== "office" || s.info?.excel || s.info?.powerpoint);
  const cur = list.find((f) => f.id === s.fmt) ?? list[0];
  const pdf = { text: "PDF", bg: "#F2F1ED", fg: "#D2453D" };
  const curShort = cur.id === "img" && s.dir === "from" ? (s.imgFmt === "png" ? "PNG" : "JPG") : cur.short;
  const from = s.dir === "from" ? pdf : { text: curShort, bg: cur.bg, fg: cur.fg };
  const to = s.dir === "from" ? { text: curShort, bg: cur.bg, fg: cur.fg } : pdf;

  const pick = async () => {
    const ext = s.dir === "from" ? ["pdf"] : (cur.ext ?? []);
    const res = await open({ multiple: true, directory: false, title: t("convert.pick"), filters: [{ name: t(`convert.fmt.${cur.id}`), extensions: ext }] });
    if (Array.isArray(res)) s.addFiles(res);
    else if (typeof res === "string") s.addFiles([res]);
  };

  return (
    <div className="rise cvt__pane">
      <div className="cflow">
        <Badge {...from} />
        <span className={`cflow__dots ${s.running ? "on" : ""}`} aria-hidden="true">
          {[0, 1, 2, 3, 4].map((i) => (
            <span key={i} style={{ animationDelay: `${i * 0.15}s` }} />
          ))}
        </span>
        <span className="cflow__to" key={to.text}>
          <Badge {...to} />
        </span>
      </div>
      <Segmented
        className="cvt__dir"
        label={t("convert.direction")}
        value={s.dir}
        itemWidth={150}
        options={[
          { value: "from", label: t("convert.fromPdf") },
          { value: "to", label: t("convert.toPdf") },
        ]}
        onChange={(v) => !s.running && s.setDir(v)}
      />
      <div className="cfmts">
        {list.map((f, i) => (
          <FormatButton key={f.id} f={f} on={f.id === cur.id} delay={i * 50} onPick={() => !s.running && s.set({ fmt: f.id, outputs: null })} />
        ))}
      </div>
      <div className="cqueue">
        <div className="cqueue__head">
          <span className="title">{t("convert.files")}</span>
          <Button variant="ghost" size="sm" icon={<Icons.plus />} onClick={() => void pick()} disabled={s.running}>
            {t("convert.add")}
          </Button>
        </div>
        {s.files.length === 0 ? (
          <button type="button" className="mdrop rise" onClick={() => void pick()}>
            <span className="mdrop__bub">
              <Icons.plus />
            </span>
            <span className="mdrop__t">{s.dir === "from" ? t("convert.dropPdf") : t("convert.dropOther")}</span>
            <span className="muted">{t("convert.dropHint")}</span>
          </button>
        ) : (
          s.files.map((f) => {
            const b = badgeOf(f.path);
            const name = f.path.split(/[\\/]/).pop();
            return (
              <div key={f.path} className="cfile rise">
                <span className="cbadge cbadge--file" style={{ background: "#F2F1ED", color: b.c }}>
                  {b.ext}
                </span>
                <span className="cfile__body">
                  <span className="cfile__row">
                    <span className="cfile__name">{name}</span>
                    <span className="muted tnum">{f.status === "done" ? t("convert.stDone") : f.status === "error" ? t("convert.stError") : f.status === "run" ? `${f.pct} %` : t("convert.stWait")}</span>
                  </span>
                  <span className="cfile__bar">
                    <span style={{ width: `${f.pct}%`, background: f.status === "done" ? "var(--success)" : f.status === "error" ? "var(--danger)" : "var(--accent)" }} />
                  </span>
                </span>
                {f.status === "done" ? (
                  <span className="cfile__ok popin">
                    <Icons.check />
                  </span>
                ) : (
                  !s.running && (
                    <button type="button" className="btn btn--ghost cfile__rm" aria-label={t("convert.remove")} onClick={() => s.removeFile(f.path)}>
                      <Icons.close />
                    </button>
                  )
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

function FormatButton({ f, on, delay, onPick }: { f: FormatCard; on: boolean; delay: number; onPick: () => void }) {
  const { t } = useTranslation();
  const s = useConvert();
  const short = f.id === "img" && s.dir === "from" ? (s.imgFmt === "png" ? "PNG" : "JPG") : f.short;
  return (
    <button type="button" className="cfmt rise" aria-pressed={on} onClick={onPick} style={{ animationDelay: `${delay}ms` }}>
      <Badge text={short} bg={f.bg} fg={f.fg} small />
      <span className="cfmt__t">
        <span className="cfmt__name">{t(`convert.fmt.${f.id}`)}</span>
        <span className="muted">{t(`convert.hint.${s.dir}.${f.id}`)}</span>
      </span>
    </button>
  );
}

// ---------------------------------------------------------------- OCR

function OcrPane() {
  const { t } = useTranslation();
  const s = useConvert();
  const tab = useActiveTab();
  const openFiles = useApp((x) => x.openFiles);
  const [shown, setShown] = useState(0);
  const doc = tab?.info.id ?? null;
  const total = tab?.info.pageCount ?? 0;
  // Pendant l'OCR, la page en cours ; ensuite, celle choisie.
  const page = s.running && s.ocrDoc === doc ? (s.ocrPage ?? 0) : shown;
  const result = s.ocrDoc === doc ? s.ocrPages.find((p) => p.page === page) : undefined;
  const scanning = s.running && s.ocrDoc === doc && !result && !s.ocrWriting;

  if (!tab) {
    return (
      <div className="rise cvt__pane">
        <button type="button" className="mdrop" onClick={() => void open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] }).then((p) => {
              if (typeof p === "string") void openFiles([p]);
            })}>
          <span className="mdrop__bub">
            <Icons.plus />
          </span>
          <span className="mdrop__t">{t("convert.ocrPick")}</span>
        </button>
      </div>
    );
  }
  return (
    <div className="rise cocr">
      <div className="cocr__col">
        <span className="muted cocr__lbl">
          <span>{t("convert.scanned", { name: tab.info.name })}</span>
          {total > 1 && (
            <span className="cocr__pager">
              <button type="button" className="btn btn--ghost btn--sm" disabled={s.running || page <= 0} onClick={() => setShown(page - 1)} aria-label={t("convert.prevPage")}>
                ‹
              </button>
              <span className="tnum">
                {page + 1} / {total}
              </span>
              <button type="button" className="btn btn--ghost btn--sm" disabled={s.running || page >= total - 1} onClick={() => setShown(page + 1)} aria-label={t("convert.nextPage")}>
                ›
              </button>
            </span>
          )}
        </span>
        <PagePreview doc={tab.info.id} page={page} ratio={tab.info.pages[page] ? tab.info.pages[page].h / tab.info.pages[page].w : 1.41} scanning={scanning} lines={result?.lines ?? []} />
      </div>
      <div className="cocr__col">
        <span className="cocr__lbl">
          <span className="muted">{t("convert.recognized")}</span>
          {result && !result.skipped && <span className="muted tnum" style={{ color: "#6FDCA8" }}>{t("convert.confidence", { n: Math.round(result.conf) })}</span>}
        </span>
        <div className="cocr__text">
          {result?.skipped ? (
            <div className="cocr__empty">{t("convert.hasText")}</div>
          ) : result && result.lines.length ? (
            result.lines.map((l, i) => (
              <p key={i} style={{ animationDelay: `${Math.min(i, 30) * 60}ms`, fontWeight: i === 0 && l.b[3] > 0.02 ? 600 : 400 }}>
                {l.t}
              </p>
            ))
          ) : (
            <div className="cocr__empty">
              <svg className="i" width="28" height="28" aria-hidden="true">
                <path d={D_SCAN} />
              </svg>
              <span>{scanning ? t("convert.reading") : t("convert.ocrEmpty")}</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/** Page du document, avec le faisceau de balayage et les lignes trouvées. */
function PagePreview({ doc, page, ratio, scanning, lines }: { doc: number; page: number; ratio: number; scanning: boolean; lines: { b: [number, number, number, number] }[] }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let alive = true;
    void renderPage({ doc, page, width: 760, rot: 0, annots: true, forms: true, prio: Prio.THUMB, gen: 0 }).then((r) => {
      const c = ref.current;
      if (!alive || !c || r.kind !== "ok") return;
      c.width = r.bitmap.width;
      c.height = r.bitmap.height;
      c.getContext("2d")?.drawImage(r.bitmap, 0, 0);
    });
    return () => {
      alive = false;
    };
  }, [doc, page]);
  return (
    <div className="cscan" style={{ aspectRatio: `1 / ${ratio}` }}>
      <canvas ref={ref} className="cscan__img" />
      {lines.map((l, i) => (
        <span key={i} className="cscan__box" style={{ left: `${l.b[0] * 100}%`, top: `${l.b[1] * 100}%`, width: `${l.b[2] * 100}%`, height: `${l.b[3] * 100}%`, animationDelay: `${Math.min(i, 30) * 60}ms` }} />
      ))}
      {scanning && <span className="cscan__beam" />}
    </div>
  );
}

// ---------------------------------------------------------------- options

function Side() {
  const { t } = useTranslation();
  const s = useConvert();
  const tab = useActiveTab();
  const openPath = useApp((x) => x.openPath);
  const conv = s.tab === "conv";
  const img = s.dir === "from" && s.fmt === "img";
  const outputs = conv ? s.outputs : s.ocrOutput ? [s.ocrOutput] : null;

  const openOut = (p: string) => {
    if (extOf(p) === "pdf") void openPath(p);
    else void openExternal(p).catch(() => void revealItemInDir(p));
  };
  const doneMsg = conv
    ? t("convert.done", { count: outputs?.length ?? 0, fmt: t(`convert.fmtTo.${s.dir === "to" ? "pdf" : s.fmt}`) })
    : extOf(s.ocrOutput?.path ?? "") === "pdf"
      ? t("convert.ocrDone")
      : t("convert.ocrDoneText");
  const runLabel = conv
    ? s.running
      ? t("convert.running")
      : t("convert.run", { count: s.files.length })
    : s.running
      ? s.ocrWriting
        ? t("convert.writing")
        : t("convert.ocrRunning", { done: s.ocrDone, total: s.ocrTotal })
      : t("convert.ocrRun");

  return (
    <aside className="panel tool__side" aria-label={t("convert.options")}>
      <h2 className="title">{t("convert.options")}</h2>
      <fieldset className="csets" disabled={s.running}>
        {conv ? (
          <div className="rise tool__stack">
            {img && (
              <>
                <div className="tool__field">
                  <span className="muted">{t("convert.dpi")}</span>
                  <div className="cvt__chips">
                    {[150, 300, 600].map((d) => (
                      <Chip key={d} pressed={s.dpi === d} onClick={() => s.set({ dpi: d })}>
                        <span className="tnum">{d} ppp</span>
                      </Chip>
                    ))}
                  </div>
                </div>
                <div className="tool__field">
                  <span className="muted">{t("convert.imgFormat")}</span>
                  <div className="cvt__chips">
                    <Chip pressed={s.imgFmt === "png"} onClick={() => s.set({ imgFmt: "png" })}>
                      PNG
                    </Chip>
                    <Chip pressed={s.imgFmt === "jpeg"} onClick={() => s.set({ imgFmt: "jpeg" })}>
                      JPEG
                    </Chip>
                  </div>
                </div>
              </>
            )}
            {s.dir === "from" && !img && (
              <>
                <div className="tool__row">
                  <span>{t("convert.keepLayout")}</span>
                  <Switch label={t("convert.keepLayout")} checked={s.keepLayout} onChange={(keepLayout) => s.set({ keepLayout })} />
                </div>
                <div className="tool__row">
                  <span>{t("convert.autoOcr")}</span>
                  <Switch label={t("convert.autoOcr")} checked={s.autoOcr} onChange={(autoOcr) => s.set({ autoOcr })} />
                </div>
                <div className="tool__row">
                  <span>{t("convert.perPage")}</span>
                  <Switch label={t("convert.perPage")} checked={s.perPage} onChange={(perPage) => s.set({ perPage })} />
                </div>
              </>
            )}
            {s.dir === "to" && (
              <div className="cvt__note">
                <span className="cvt__note-t">{s.info?.word ? t("convert.officeOn") : t("convert.officeOff")}</span>
                <span className="muted">{s.info?.word ? t("convert.officeOnSub") : t("convert.officeOffSub")}</span>
              </div>
            )}
          </div>
        ) : (
          <div className="rise tool__stack">
            <div className="tool__field">
              <span className="muted">{t("convert.langs")}</span>
              <div className="cvt__chips">
                {LANGS.map((l) => (
                  <Chip key={l} pressed={s.langs.includes(l)} onClick={() => s.set({ langs: s.langs.includes(l) ? s.langs.filter((x) => x !== l) : [...s.langs, l] })}>
                    {t(`convert.lang.${l}`)}
                  </Chip>
                ))}
              </div>
            </div>
            <div className="tool__row">
              <span>{t("convert.searchable")}</span>
              <Switch label={t("convert.searchable")} checked={s.searchable} onChange={(searchable) => s.set({ searchable })} />
            </div>
            <div className="tool__row">
              <span>{t("convert.deskew")}</span>
              <Switch label={t("convert.deskew")} checked={s.deskew} onChange={(deskew) => s.set({ deskew })} />
            </div>
            <div className="cvt__note">
              <span className="cvt__note-t">{t("convert.engine")}</span>
              <span className="muted">{t("convert.engineSub")}</span>
            </div>
          </div>
        )}
      </fieldset>
      <div className="tool__foot">
        {outputs && outputs.length > 0 && !s.running && (
          <div className="rise tool__done">
            <span className="tool__done-ic popin">
              <Icons.check />
            </span>
            <span className="tool__done-t">
              {doneMsg}
              {outputs.length === 1 && <span className="muted tnum"> · {formatSize(outputs[0].size)}</span>}
            </span>
            <Button variant="ghost" size="sm" className="tool__open" onClick={() => openOut(outputs[0].path)}>
              {t("tool.open")}
            </Button>
          </div>
        )}
        {s.running ? (
          <>
            <Button variant="primary" size="lg" disabled>
              {runLabel}
            </Button>
            <Button variant="ghost" onClick={() => s.cancel()}>
              {t("common.cancel")}
            </Button>
          </>
        ) : (
          <Button
            variant="primary"
            size="lg"
            disabled={conv ? !s.files.length : !tab || !s.langs.length}
            onClick={() => (conv ? void s.run() : tab && void s.runOcr(tab.info.id, tab.info.pageCount))}
          >
            {runLabel}
          </Button>
        )}
      </div>
    </aside>
  );
}
