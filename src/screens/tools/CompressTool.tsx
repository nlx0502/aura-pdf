import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, Icons, Slider, StepList, Switch, type StepItem } from "../../components";
import { formatSize } from "../../lib/format";
import { estimate, PRESETS, type Analysis, type Estimate, type Level } from "../../lib/tools";
import { useApp } from "../../stores/app";
import { useCompressTool } from "../../stores/tools";
import "./tools.css";

const CAT = [
  { k: "images", c: "#8B87FA" },
  { k: "fonts", c: "#F2B84B" },
  { k: "content", c: "#5BC0EB" },
  { k: "meta", c: "#F07A9A" },
] as const;

/** Nombre qui s'anime vers sa nouvelle valeur (maquette : « la taille estimée s'anime en chiffres »). */
function useAnimated(target: number) {
  const [v, setV] = useState(target);
  const from = useRef(target);
  useEffect(() => {
    const start = performance.now();
    const a = from.current;
    let raf = 0;
    const reduce = document.documentElement.dataset.motion === "reduce" || window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const tick = (now: number) => {
      const k = reduce ? 1 : Math.min(1, (now - start) / 550);
      const e = 1 - Math.pow(1 - k, 3);
      const x = a + (target - a) * e;
      setV(x);
      from.current = x;
      if (k < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [target]);
  return v;
}

/** Outil « Compresser » (maquette Compress.dc.html). */
export function CompressTool() {
  const { t } = useTranslation();
  const s = useCompressTool();
  const active = useApp((x) => x.tabs.find((tb) => tb.key === x.active)?.info ?? null);
  const openPath = useApp((x) => x.openPath);

  useEffect(() => {
    const st = useCompressTool.getState();
    if (active && st.source?.path !== active.path && !st.analyzing) void st.setSource(active.path);
  }, [active?.path]); // eslint-disable-line react-hooks/exhaustive-deps

  const pick = () =>
    void open({ multiple: false, directory: false, title: t("tool.pickCompress"), filters: [{ name: "PDF", extensions: ["pdf"] }] }).then((p) => {
      if (typeof p === "string") void s.setSource(p);
    });

  const a = s.analysis;
  const est = a ? estimate(a, s.opts) : null;
  // Après traitement : la taille réelle remplace l'estimation.
  const after: Estimate | null = s.result ? { images: s.result.images, fonts: s.result.fonts, content: s.result.content, meta: s.result.meta, total: s.result.total } : est;
  const total = a?.total ?? 0;
  const shown = useAnimated(after?.total ?? 0);
  const saved = Math.max(0, total - (after?.total ?? total));
  const pct = total ? Math.round((saved / total) * 100) : 0;
  const run = s.run;
  const running = run.kind === "running";

  const levels: { id: Level; label: string; hint: string; bars: number }[] = [
    { id: "light", label: t("tool.lvlLight"), hint: t("tool.lvlLightHint"), bars: 1 },
    { id: "rec", label: t("tool.lvlRec"), hint: t("tool.lvlRecHint"), bars: 2 },
    { id: "strong", label: t("tool.lvlStrong"), hint: t("tool.lvlStrongHint"), bars: 3 },
    { id: "custom", label: t("tool.lvlCustom"), hint: t("tool.lvlCustomHint"), bars: 3 },
  ];

  const stepLabels = [t("tool.stAnalyze"), t("tool.stImages"), t("tool.stFonts"), t("tool.stClean"), t("tool.stWrite")];
  const curStep = running ? run.step : run.kind === "done" ? 5 : -1;
  const steps: StepItem[] = stepLabels.map((label, i) => ({
    label,
    state: i < curStep ? "done" : i === curStep ? "active" : "pending",
    meta: i === 1 && running && run.step === 1 && run.total ? `${run.done} / ${run.total}` : i < curStep ? t("tool.stDone") : undefined,
  }));

  return (
    <div className="workspace toolw">
      <main className="panel tool__main">
        <div className="tool__col tool__col--wide">
          <header className="tool__title">
            <h1>{t("tool.compressTitle")}</h1>
            <span className="muted">{t("tool.compressSub")}</span>
          </header>
          {!s.source ? (
            <button type="button" className="mdrop rise" onClick={pick}>
              <span className="mdrop__bub">
                <Icons.plus />
              </span>
              <span className="mdrop__t">{t("tool.pickCompress")}</span>
            </button>
          ) : (
            <>
              <div className="tool__file">
                <span className={`mfile__sheet ${running ? "is-squeezing" : ""}`} aria-hidden="true">
                  <span style={{ height: 4, width: "70%", background: "var(--success)", borderRadius: 2 }} />
                  <span className="mfile__ln" />
                  <span style={{ height: 9, width: "100%", background: "#E4E2DB", borderRadius: 1, flex: "none" }} />
                  <span className="mfile__ln" style={{ width: "85%" }} />
                </span>
                <span className="mfile__text">
                  <span className="mfile__name">{s.source.name}</span>
                  <span className="muted tnum">
                    {t("tool.pages", { count: s.source.pages })} · {formatSize(s.source.size)}
                    {a ? ` · ${t("tool.imagesCount", { count: a.imageList.length })}` : ""}
                  </span>
                </span>
                <Button variant="ghost" onClick={pick} disabled={running}>
                  {t("tool.change")}
                </Button>
              </div>
              <div className="clevels" role="radiogroup" aria-label={t("tool.level")}>
                {levels.map((l) => {
                  const sz = a ? estimate(a, l.id === "custom" ? s.opts : PRESETS[l.id]).total : 0;
                  return (
                    <button key={l.id} type="button" role="radio" className="clvl" aria-checked={s.level === l.id} aria-pressed={s.level === l.id} onClick={() => s.setLevel(l.id)} disabled={running}>
                      <span className="clvl__bars" aria-hidden="true">
                        {[6, 11, 18].map((h, i) => (
                          <span key={h} className={i < l.bars ? "f" : ""} style={{ height: h }} />
                        ))}
                      </span>
                      <span className="clvl__t">{l.label}</span>
                      <span className="muted">{l.hint}</span>
                      <span className="clvl__s tnum">{a ? `≈ ${formatSize(sz)}` : "…"}</span>
                    </button>
                  );
                })}
              </div>
              {a && after && (
                <section className="csum">
                  <div className="cring">
                    <svg width="132" height="132" viewBox="0 0 132 132" aria-hidden="true">
                      <circle cx="66" cy="66" r="56" className="cring__bg" />
                      <circle cx="66" cy="66" r="56" className="cring__fg" style={{ strokeDashoffset: (351.9 * (1 - pct / 100)).toFixed(1) }} />
                    </svg>
                    <span className="cring__t">
                      <span className="tnum">−{pct} %</span>
                      <span className="muted">{t("tool.reduction")}</span>
                    </span>
                  </div>
                  <div className="csum__body">
                    <div className="csum__line">
                      <span className="csum__size tnum">{formatSize(shown)}</span>
                      <span className="muted tnum">{t("tool.insteadOf", { before: formatSize(total), saved: formatSize(saved) })}</span>
                    </div>
                    <Bars label={t("tool.before")} a={a} parts={{ images: a.images, fonts: a.fonts, content: a.content, meta: a.meta }} />
                    <Bars label={s.result ? t("tool.afterReal") : t("tool.after")} a={a} parts={after} />
                    <div className="csum__legend">
                      {CAT.map((c) => (
                        <span key={c.k}>
                          <span className="csum__dot" style={{ background: c.c }} />
                          {t(`tool.cat.${c.k}`)} <span className="tnum">{formatSize(after[c.k])}</span>
                        </span>
                      ))}
                    </div>
                  </div>
                </section>
              )}
              {s.analyzing && <p className="muted">{t("tool.analyzing")}</p>}
              {curStep >= 0 && (
                <section className="csteps rise">
                  <StepList steps={steps} />
                </section>
              )}
            </>
          )}
        </div>
      </main>
      <aside className="panel tool__side" aria-label={t("tool.settings")}>
        <div className="tool__row">
          <h2 className="title">{t("tool.settings")}</h2>
          <span className="muted">{levels.find((l) => l.id === s.level)?.label}</span>
        </div>
        <fieldset className="csets" disabled={running}>
          <section className="csec">
            <span className="csec__h">{t("tool.cat.images")}</span>
            <div className="tool__field">
              <span className="tool__row">
                <span>{t("tool.maxDpi")}</span>
              </span>
              <Slider label={t("tool.maxDpi")} min={72} max={300} step={6} value={s.opts.dpi} format={(v) => `${v} ppp`} onChange={(dpi) => s.setOpt({ dpi })} />
            </div>
            <div className="tool__field">
              <span className="tool__row">
                <span>{t("tool.jpegQuality")}</span>
              </span>
              <Slider label={t("tool.jpegQuality")} min={30} max={100} step={5} value={s.opts.quality} format={(v) => `${v} %`} onChange={(quality) => s.setOpt({ quality })} />
            </div>
            <Opt label={t("tool.gray")} k="gray" />
          </section>
          <section className="csec">
            <span className="csec__h">{t("tool.cat.fonts")}</span>
            <Opt label={t("tool.subset")} k="subset" />
            <Opt label={t("tool.unembed")} k="unembed" />
          </section>
          <section className="csec">
            <span className="csec__h">{t("tool.cleanup")}</span>
            <Opt label={t("tool.optMeta")} k="meta" />
            <Opt label={t("tool.optThumbs")} k="thumbs" />
            <Opt label={t("tool.optAttach")} k="attach" />
            <Opt label={t("tool.optCompact")} k="compact" />
          </section>
        </fieldset>
        <div className="tool__foot">
          {run.kind === "done" && (
            <div className="rise tool__done">
              <span className="tool__done-ic popin">
                <Icons.check />
              </span>
              <span className="tool__done-t">{t("tool.compressDone", { size: formatSize(run.outputs[0]?.size ?? 0) })}</span>
              <Button variant="ghost" size="sm" className="tool__open" onClick={() => void openPath(run.outputs[0].path)}>
                {t("tool.open")}
              </Button>
            </div>
          )}
          {running ? (
            <Button size="lg" onClick={() => s.cancel()}>
              {t("common.cancel")}
            </Button>
          ) : (
            <Button variant="primary" size="lg" disabled={!s.source || !a} onClick={() => s.start()}>
              {run.kind === "done" ? t("tool.compressAgain") : t("tool.compressBtn")}
            </Button>
          )}
          {s.source && <span className="muted tool__dest">{t("tool.compressDest")}</span>}
        </div>
      </aside>
    </div>
  );
}

function Opt({ label, k }: { label: string; k: "gray" | "subset" | "unembed" | "meta" | "thumbs" | "attach" | "compact" }) {
  const v = useCompressTool((s) => s.opts[k]);
  const setOpt = useCompressTool((s) => s.setOpt);
  return (
    <div className="tool__row">
      <span>{label}</span>
      <Switch label={label} checked={v} onChange={(x) => setOpt({ [k]: x })} />
    </div>
  );
}

function Bars({ label, a, parts }: { label: string; a: Analysis; parts: Pick<Estimate, "images" | "fonts" | "content" | "meta"> }) {
  return (
    <div className="cbar">
      <span className="muted cbar__l">{label}</span>
      <span className="cbar__track" role="img" aria-label={label}>
        {CAT.map((c) => (
          <span key={c.k} className="cbar__seg" style={{ width: `${(parts[c.k] / Math.max(1, a.total)) * 100}%`, background: c.c }} />
        ))}
      </span>
    </div>
  );
}
