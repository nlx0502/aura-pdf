import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, Checkbox, IconButton, Icons, ProgressBar, Segmented, Slider, Switch, TextField } from "../../components";
import { formatSize } from "../../lib/format";
import { IMAGE_EXT } from "../../lib/files";
import { PART_COLORS, toolsApi } from "../../lib/tools";
import { useApp } from "../../stores/app";
import { dirName, stemOf, useMergeTool, type MergeFile } from "../../stores/tools";
import "./tools.css";

const ROW = 82;

const MergeIcon = () => (
  <svg className="i" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M3.5 2.5v2.5a3.5 3.5 0 0 0 3.5 3.5h2a3.5 3.5 0 0 0 3.5-3.5V2.5 M8 8.5v5.5" />
  </svg>
);
const SplitIcon = () => (
  <svg className="i" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M8 2v5.5 M8 7.5c0 2-4.5 2-4.5 5.5 M8 7.5c0 2 4.5 2 4.5 5.5" />
  </svg>
);

async function pickInputs(title: string, pdfOnly: boolean): Promise<string[]> {
  const res = await open({
    multiple: !pdfOnly,
    directory: false,
    title,
    filters: pdfOnly ? [{ name: "PDF", extensions: ["pdf"] }] : [{ name: "PDF et images", extensions: ["pdf", ...IMAGE_EXT] }],
  });
  if (!res) return [];
  return Array.isArray(res) ? res : [res];
}

/** Outil « Fusionner et diviser » (maquette Merge.dc.html). */
export function MergeTool() {
  const { t } = useTranslation();
  const s = useMergeTool();
  const active = useApp((x) => x.tabs.find((tb) => tb.key === x.active)?.info ?? null);

  // Premier affichage : le document ouvert sert de point de départ.
  useEffect(() => {
    const st = useMergeTool.getState();
    if (active && !st.files.length) void st.add([active.path]);
    if (active && !st.source) void st.setSource(active.path);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const merge = s.tab === "merge";
  return (
    <div className="workspace toolw">
      <main className="panel tool__main">
        <div className="tool__col">
          <header className="tool__head">
            <div className="tool__title">
              <h1>{merge ? t("tool.mergeTitle") : t("tool.splitTitle")}</h1>
              <span className="muted">{merge ? t("tool.mergeSub") : t("tool.splitSub")}</span>
            </div>
            <Segmented
              label={t("tool.operation")}
              value={s.tab}
              itemWidth={120}
              onChange={(tab) => s.set({ tab, run: { kind: "idle" } })}
              options={[
                { value: "merge", label: t("tool.merge"), icon: <MergeIcon /> },
                { value: "split", label: t("tool.split"), icon: <SplitIcon /> },
              ]}
            />
          </header>
          {merge ? <MergeList /> : <SplitSetup />}
        </div>
      </main>
      <aside className="panel tool__side" aria-label={t("tool.result")}>
        <h2 className="title">{t("tool.result")}</h2>
        {merge ? <MergeResult /> : <SplitResult />}
        <RunFooter />
      </aside>
    </div>
  );
}

// ---------------------------------------------------------------- fusion

function MergeList() {
  const { t } = useTranslation();
  const s = useMergeTool();
  const running = s.run.kind === "running" ? s.run : null;
  const finished = s.run.kind === "done";
  const [drag, setDrag] = useState<{ id: number; y: number; to: number } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const startDrag = (e: React.PointerEvent, f: MergeFile, index: number) => {
    if (running || e.button !== 0) return;
    e.preventDefault();
    const top0 = listRef.current!.getBoundingClientRect().top;
    const offset = e.clientY - top0 - index * ROW;
    const move = (ev: PointerEvent) => {
      const y = Math.max(-10, Math.min((s.files.length - 1) * ROW + 10, ev.clientY - top0 - offset));
      setDrag({ id: f.id, y, to: Math.max(0, Math.min(s.files.length - 1, Math.round(y / ROW))) });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setDrag((d) => {
        if (d && d.to !== index) useMergeTool.getState().move(f.id, d.to);
        return null;
      });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  // Position affichée de chaque fichier pendant le glisser (les autres s'écartent).
  const order = s.files.map((f) => f.id);
  if (drag) {
    const from = order.indexOf(drag.id);
    order.splice(from, 1);
    order.splice(drag.to, 0, drag.id);
  }

  return (
    <div className="rise mlist__wrap">
      <div ref={listRef} className="mlist" style={{ height: Math.max(0, s.files.length * ROW - 10) }}>
        {s.files.map((f, i) => {
          const pos = order.indexOf(f.id);
          const dragging = drag?.id === f.id;
          const done = !!running && i < running.done ? true : finished;
          const working = !!running && i === running.done;
          const color = PART_COLORS[i % PART_COLORS.length];
          return (
            <div
              key={f.id}
              className={`mfile ${dragging ? "is-dragging" : ""}`}
              style={{ top: dragging ? drag.y : pos * ROW, boxShadow: working ? "inset 0 0 0 1.5px var(--accent)" : undefined }}
            >
              <span className="mfile__grip" onPointerDown={(e) => startDrag(e, f, i)} aria-hidden="true">
                <svg className="i" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeLinecap="round">
                  <path d="M6 4h.01 M10 4h.01 M6 8h.01 M10 8h.01 M6 12h.01 M10 12h.01" style={{ strokeWidth: 2.4 }} />
                </svg>
              </span>
              <span className="mfile__sheet" aria-hidden="true">
                <span style={{ height: 4, width: "70%", background: color, borderRadius: 2 }} />
                <span className="mfile__ln" />
                <span className="mfile__ln" style={{ width: "85%" }} />
                <span className="mfile__ln" style={{ width: "92%" }} />
                <span className="mfile__pos tnum" style={{ background: color }}>
                  {pos + 1}
                </span>
              </span>
              <span className="mfile__text">
                <span className="mfile__name">{f.name}</span>
                <span className="muted tnum">{f.image ? t("tool.imagePage") : `${t("tool.pages", { count: f.pages })} · ${formatSize(f.size)}`}</span>
              </span>
              {done && (
                <span className="mfile__ok popin">
                  <Icons.check />
                </span>
              )}
              {working && <span className="spinner" />}
              {!running && !finished && (
                <span className="mfile__actions">
                  <IconButton size="sm" label={t("tool.up", { name: f.name })} tip={t("tool.upShort")} icon={<Icons.chevronUp />} disabled={i === 0} onClick={() => s.move(f.id, i - 1)} />
                  <IconButton size="sm" label={t("tool.down", { name: f.name })} tip={t("tool.downShort")} icon={<Icons.chevronDown />} disabled={i === s.files.length - 1} onClick={() => s.move(f.id, i + 1)} />
                  <IconButton size="sm" label={t("tool.removeFile", { name: f.name })} tip={t("tool.removeShort")} tipAlign="end" icon={<Icons.close />} onClick={() => s.remove(f.id)} />
                </span>
              )}
            </div>
          );
        })}
      </div>
      <button type="button" className="mdrop" onClick={() => void pickInputs(t("tool.addTitle"), false).then((p) => {
              if (p.length) void s.add(p);
            })} disabled={!!running}>
        <span className="mdrop__bub">
          <Icons.plus />
        </span>
        <span className="mdrop__t">{t("tool.drop")}</span>
        <span className="muted">{t("tool.dropHint")}</span>
      </button>
    </div>
  );
}

function MergeResult() {
  const { t } = useTranslation();
  const s = useMergeTool();
  const pages = s.files.reduce((n, f) => n + f.pages, 0) + (s.opts.toc ? 1 : 0);
  const size = s.files.reduce((n, f) => n + f.size, 0);
  const setOpt = (patch: Partial<typeof s.opts>) => s.set({ opts: { ...s.opts, ...patch }, run: { kind: "idle" } });
  return (
    <div className="rise tool__res">
      <div className="tool__tiles">
        <div className="tool__tile">
          <span className="muted">{t("tool.pagesLabel")}</span>
          <span className="tnum">{pages}</span>
        </div>
        <div className="tool__tile">
          <span className="muted">{t("tool.size")}</span>
          <span className="tnum">{s.files.some((f) => f.image) ? "≈ " : ""}{formatSize(size)}</span>
        </div>
      </div>
      <label className="tool__field">
        <span className="muted">{t("tool.fileName")}</span>
        <TextField value={s.outName} suffix=".pdf" onChange={(e) => s.set({ outName: e.target.value, run: { kind: "idle" } })} />
      </label>
      <Row label={t("tool.optBookmarks")} checked={s.opts.bookmarks} onChange={(v) => setOpt({ bookmarks: v })} />
      <Row label={t("tool.optToc")} checked={s.opts.toc} onChange={(v) => setOpt({ toc: v })} />
      <Row label={t("tool.optA4")} checked={s.opts.a4} onChange={(v) => setOpt({ a4: v })} />
    </div>
  );
}

function Row({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="tool__row">
      <span>{label}</span>
      <Switch label={label} checked={checked} onChange={onChange} />
    </div>
  );
}

// ---------------------------------------------------------------- division

function SplitSetup() {
  const { t } = useTranslation();
  const s = useMergeTool();
  const src = s.source;
  const n = src?.pages ?? 0;
  const { parts } = s.splitParts();

  // Découpage par taille : calculé par le cœur Rust (objets réellement partagés).
  useEffect(() => {
    if (s.method !== "size" || !src) return;
    let alive = true;
    const id = window.setTimeout(() => {
      void toolsApi
        .bySize(src.path, s.maxMb * 1_000_000)
        .then((p) => alive && useMergeTool.getState().set({ sizeParts: p }))
        .catch(() => {});
    }, 180);
    return () => {
      alive = false;
      window.clearTimeout(id);
    };
  }, [s.method, s.maxMb, src]);

  if (!src) {
    return (
      <button type="button" className="mdrop rise" onClick={() => void pickInputs(t("tool.pickSplit"), true).then((p) => {
              if (p[0]) void s.setSource(p[0]);
            })}>
        <span className="mdrop__bub">
          <Icons.plus />
        </span>
        <span className="mdrop__t">{t("tool.pickSplit")}</span>
      </button>
    );
  }
  const owner = new Map<number, number>();
  parts.forEach(([a, b], k) => {
    for (let p = a; p <= b; p++) if (!owner.has(p)) owner.set(p, k);
  });
  const maxMb = Math.max(2, Math.ceil(src.size / 1_000_000));
  const top = src.marks.filter((m) => m.depth === 0);
  const methods: { id: typeof s.method; label: string; hint: string; disabled?: boolean }[] = [
    { id: "ranges", label: t("tool.byPages"), hint: t("tool.byPagesHint") },
    { id: "size", label: t("tool.bySize"), hint: t("tool.bySizeHint") },
    { id: "marks", label: t("tool.byMarks"), hint: top.length ? t("tool.byMarksHint") : t("tool.noMarks"), disabled: !top.length },
  ];

  return (
    <div className="rise tool__stack">
      <div className="tool__file">
        <span className="mfile__sheet" aria-hidden="true">
          <span style={{ height: 4, width: "70%", background: "var(--accent)", borderRadius: 2 }} />
          <span className="mfile__ln" />
          <span className="mfile__ln" style={{ width: "85%" }} />
        </span>
        <span className="mfile__text">
          <span className="mfile__name">{src.name}</span>
          <span className="muted tnum">
            {t("tool.pages", { count: n })} · {formatSize(src.size)}
            {top.length ? ` · ${t("tool.marksCount", { count: top.length })}` : ""}
          </span>
        </span>
        <Button variant="ghost" onClick={() => void pickInputs(t("tool.pickSplit"), true).then((p) => {
              if (p[0]) void s.setSource(p[0]);
            })}>
          {t("tool.change")}
        </Button>
      </div>
      <div className="tool__field">
        <span className="title" style={{ fontWeight: 500 }}>
          {t("tool.method")}
        </span>
        <div className="tool__methods">
          {methods.map((m) => (
            <button key={m.id} type="button" className="tool__method" aria-pressed={s.method === m.id} disabled={m.disabled} onClick={() => s.set({ method: m.id, run: { kind: "idle" } })}>
              <span className="tool__method-t">{m.label}</span>
              <span className="muted">{m.hint}</span>
            </button>
          ))}
        </div>
      </div>
      {s.method === "ranges" && (
        <label className="tool__field rise">
          <span className="muted">{t("tool.rangesLabel")}</span>
          <TextField className="tnum" value={s.ranges} aria-label={t("tool.rangesAria")} onChange={(e) => s.set({ ranges: e.target.value, run: { kind: "idle" } })} />
        </label>
      )}
      {s.method === "size" && (
        <div className="tool__field rise">
          <span className="tool__row">
            <span>{t("tool.maxSize")}</span>
          </span>
          <Slider label={t("tool.maxSize")} min={1} max={maxMb} value={Math.min(s.maxMb, maxMb)} format={(v) => `${v} Mo`} onChange={(v) => s.set({ maxMb: v, run: { kind: "idle" } })} />
        </div>
      )}
      {s.method === "marks" && (
        <div className="tool__marks rise">
          {top.map((m) => {
            const p = m.page + 1;
            const on = p === 1 || s.cuts.includes(p);
            return (
              <div key={`${m.title}-${p}`} className="tool__mark">
                <Checkbox
                  checked={on}
                  onChange={() => p !== 1 && s.set({ cuts: s.cuts.includes(p) ? s.cuts.filter((x) => x !== p) : [...s.cuts, p], run: { kind: "idle" } })}
                >
                  <span className="tool__mark-t">{m.title || t("tool.untitled")}</span>
                </Checkbox>
                <span className="muted tnum">{t("tool.pageN", { n: p })}</span>
              </div>
            );
          })}
        </div>
      )}
      <div className="tool__field">
        <div className="tool__row">
          <span className="muted">{t("tool.preview")}</span>
          <span className="muted tnum">{t("tool.files", { count: parts.length })}</span>
        </div>
        <div className={`tool__cells ${n > 40 ? "is-dense" : ""}`}>
          {Array.from({ length: n }, (_, i) => {
            const k = owner.get(i + 1);
            return (
              <span key={i} className="tool__cell tnum" style={{ background: k === undefined ? "var(--bg-nav-current)" : PART_COLORS[k % PART_COLORS.length], transitionDelay: `${Math.min(i, 60) * 18}ms` }}>
                {n <= 40 ? i + 1 : ""}
              </span>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function SplitResult() {
  const { t } = useTranslation();
  const s = useMergeTool();
  const { parts, bad } = s.splitParts();
  const src = s.source;
  if (!src) return null;
  const stem = stemOf(src.path);
  const done = s.run.kind === "done";
  return (
    <div className="rise tool__parts">
      {!bad &&
        parts.map(([a, b], k) => {
          const mark = s.method === "marks" ? src.marks.find((m) => m.page === a - 1 && m.depth === 0) : null;
          return (
            <div key={`${a}-${b}`} className="tool__part rise" style={{ animationDelay: `${Math.min(k, 10) * 50}ms` }}>
              <span className="tool__part-c" style={{ background: PART_COLORS[k % PART_COLORS.length] }} />
              <span className="mfile__text">
                <span className="tool__part-n">{mark?.title ? `${stem} — ${mark.title}.pdf` : `${stem} — partie ${k + 1}.pdf`}</span>
                <span className="muted tnum">{a === b ? t("tool.pageN", { n: a }) : t("tool.pagesAtoB", { a, b })}</span>
              </span>
              {done && (
                <span className="mfile__ok popin">
                  <Icons.check />
                </span>
              )}
            </div>
          );
        })}
      {bad && s.method === "ranges" && <p className="tool__warn">{t("tool.badRanges")}</p>}
    </div>
  );
}

// ---------------------------------------------------------------- pied : progression, résultat, lancement

function RunFooter() {
  const { t } = useTranslation();
  const s = useMergeTool();
  const openPath = useApp((x) => x.openPath);
  const merge = s.tab === "merge";
  const { parts, bad } = s.splitParts();
  const run = s.run;
  const pct = run.kind === "running" && run.total ? Math.round((run.done / run.total) * 100) : 0;
  const cant = run.kind === "running" || (merge ? s.files.length < 2 : bad || !parts.length);
  return (
    <div className="tool__foot">
      {run.kind === "running" && (
        <div className="rise tool__progress">
          <span className="muted tnum">
            {merge ? t("tool.merging") : t("tool.splitting")} {pct} %
          </span>
          <ProgressBar value={pct} label={merge ? t("tool.merging") : t("tool.splitting")} />
        </div>
      )}
      {run.kind === "done" && (
        <div className="rise tool__done">
          <span className="tool__done-ic popin">
            <Icons.check />
          </span>
          <span className="tool__done-t">
            {merge ? t("tool.mergeDone", { name: baseOf(run.outputs[0]?.path), count: run.outputs[0]?.pages ?? 0 }) : t("tool.splitDone", { count: run.outputs.length })}
          </span>
          {merge ? (
            <Button variant="ghost" size="sm" className="tool__open" onClick={() => void openPath(run.outputs[0].path)}>
              {t("tool.open")}
            </Button>
          ) : (
            <Button variant="ghost" size="sm" className="tool__open" onClick={() => void revealItemInDir(run.outputs[0].path)}>
              {t("tool.showFolder")}
            </Button>
          )}
        </div>
      )}
      {run.kind === "running" ? (
        <Button size="lg" onClick={() => s.cancel()}>
          {t("common.cancel")}
        </Button>
      ) : (
        <Button variant="primary" size="lg" disabled={cant} onClick={() => (merge ? void s.runMerge() : s.runSplit())}>
          {merge ? t("tool.mergeBtn", { count: s.files.length }) : t("tool.splitBtn", { count: parts.length })}
        </Button>
      )}
      {!merge && src(s) && <span className="muted tool__dest">{t("tool.dest", { dir: dirName(s.source!.path) })}</span>}
    </div>
  );
}

function src(s: { source: unknown }) {
  return !!s.source;
}

function baseOf(p?: string) {
  return p ? (p.split(/[\\/]/).pop() ?? p) : "";
}
