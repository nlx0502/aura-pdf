import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { Button, Icons, PageThumb, Segmented, Switch, toast } from "../../components";
import { api, asAppError, type UserMark } from "../../lib/api";
import { formatSize } from "../../lib/format";
import { editApi, type Layer } from "../../lib/edit";
import { Prio, renderPage } from "../../lib/render";
import { markColors } from "../../theme/tokens";
import { useApp, type DocTab, type LeftPanel as PanelId } from "../../stores/app";

const PANELS: { id: PanelId; icon: keyof typeof Icons }[] = [
  { id: "pages", icon: "grid" },
  { id: "toc", icon: "toc" },
  { id: "marks", icon: "bookmark" },
  { id: "files", icon: "attachment" },
  { id: "layers", icon: "layers" },
];

/** Panneau de navigation : miniatures, table des matières, signets, pièces jointes, calques. */
export function LeftPanel({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const panel = useApp((s) => s.leftPanel);
  const set = useApp((s) => s.set);
  return (
    <aside className="panel rpanel rpanel--left slide-in" aria-label={t("reader.navigation")} style={{ animationName: "rise" }}>
      <div className="rpanel__head">
        <h2 className="title">{t("reader.navigation")}</h2>
        <span className="muted tnum">
          {t("common.pages", { count: tab.info.pageCount })} · {formatSize(tab.info.fileSize)}
        </span>
      </div>
      <Segmented
        tabs
        block
        label={t("reader.panels")}
        value={panel}
        onChange={(v) => set({ leftPanel: v })}
        options={PANELS.map((p) => {
          const I = Icons[p.icon];
          return { value: p.id, icon: <I />, ariaLabel: t(`reader.panelNames.${p.id}`), title: t(`reader.panelNames.${p.id}`) };
        })}
      />
      <span className="rpanel__sub">{t(`reader.panelNames.${panel}`)}</span>
      <div className="rpanel__body" role="tabpanel">
        {panel === "pages" && <Thumbnails tab={tab} />}
        {panel === "toc" && <Toc tab={tab} />}
        {panel === "marks" && <Marks tab={tab} />}
        {panel === "files" && <Files tab={tab} />}
        {panel === "layers" && <Layers tab={tab} />}
      </div>
    </aside>
  );
}

// ---------------------------------------------------------------- miniatures (virtualisées)

const COL_GAP = 16;
const ROW_GAP = 20;
const LABEL_H = 20;

function Thumbnails({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(0);
  const jumpTo = useApp((s) => s.jumpTo);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setWidth(el.clientWidth - 12);
      setHeight(el.clientHeight);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const cellW = Math.max(40, (width - COL_GAP) / 2);
  const odd = tab.rot % 2 === 1;
  const rows = useMemo(() => {
    const out: { y: number; h: number; pages: number[] }[] = [];
    let y = 0;
    const pages = tab.info.pages;
    for (let i = 0; i < pages.length; i += 2) {
      const idx = i + 1 < pages.length ? [i, i + 1] : [i];
      const h = Math.max(...idx.map((k) => cellW * (odd ? pages[k].w / pages[k].h : pages[k].h / pages[k].w))) + LABEL_H;
      out.push({ y, h, pages: idx });
      y += h + ROW_GAP;
    }
    return out;
  }, [tab.info.pages, cellW, odd]);
  const total = rows.length ? rows[rows.length - 1].y + rows[rows.length - 1].h + 8 : 0;

  // Garder la page courante visible dans la liste.
  useEffect(() => {
    const el = ref.current;
    const row = rows[Math.floor(tab.page / 2)];
    if (!el || !row) return;
    if (row.y < el.scrollTop || row.y + row.h > el.scrollTop + el.clientHeight) {
      el.scrollTo({ top: row.y - el.clientHeight / 2 + row.h / 2, behavior: "smooth" });
    }
  }, [tab.page, rows]);

  const visibleRows = rows.filter((r) => r.y + r.h >= top - 300 && r.y <= top + height + 300);

  return (
    <div ref={ref} className="thumbs" onScroll={(e) => setTop(e.currentTarget.scrollTop)}>
      <div style={{ height: total, position: "relative" }}>
        {visibleRows.map((row) =>
          row.pages.map((p, k) => {
            const ps = tab.info.pages[p];
            const aspect = odd ? ps.w / ps.h : ps.h / ps.w;
            return (
              <PageThumb
                key={p}
                number={p + 1}
                current={p === tab.page}
                aspect={aspect}
                label={t("reader.goToPage", { n: p + 1 })}
                onClick={() => jumpTo(tab.key, p)}
                style={{ position: "absolute", top: row.y, left: k * (cellW + COL_GAP), width: cellW }}
              >
                <ThumbCanvas doc={tab.info.id} page={p} width={cellW} rot={tab.rot} />
              </PageThumb>
            );
          }),
        )}
      </div>
    </div>
  );
}

const ThumbCanvas = memo(function ThumbCanvas({ doc, page, width, rot }: { doc: number; page: number; width: number; rot: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let alive = true;
    const w = Math.round(Math.min(320, width * (window.devicePixelRatio || 1)) / 16) * 16;
    const timer = window.setTimeout(async () => {
      const res = await renderPage({ doc, page, width: w, rot, annots: true, forms: true, prio: Prio.THUMB, gen: 0 });
      if (!alive || res.kind !== "ok" || !ref.current) return;
      ref.current.width = res.bitmap.width;
      ref.current.height = res.bitmap.height;
      ref.current.getContext("2d")?.drawImage(res.bitmap, 0, 0);
      setReady(true);
    }, 40);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [doc, page, width, rot]);
  return (
    <>
      {!ready && <span className="skeleton" style={{ position: "absolute", inset: 0 }} />}
      <canvas ref={ref} style={{ opacity: ready ? 1 : 0, transition: "opacity .25s" }} />
    </>
  );
});

// ---------------------------------------------------------------- table des matières

function Toc({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const jumpTo = useApp((s) => s.jumpTo);
  const items = tab.info.outline;
  // Section courante : dernière entrée dont la page précède la page affichée.
  let current = -1;
  items.forEach((it, i) => {
    if (it.page !== null && it.page <= tab.page) current = i;
  });
  if (!items.length) return <p className="muted rise-sm">{t("reader.noToc")}</p>;
  return (
    <div className="rlist">
      {items.map((it, i) => (
        <button
          key={i}
          type="button"
          className={`rrowbtn rise-sm ${i === current ? "is-current" : ""}`}
          style={{ paddingLeft: 10 + Math.min(it.depth, 5) * 14, animationDelay: `${Math.min(i, 20) * 35}ms`, fontWeight: it.depth === 0 ? 600 : 400 }}
          onClick={() => it.page !== null && jumpTo(tab.key, it.page)}
          disabled={it.page === null}
        >
          <span style={{ flex: 1 }}>{it.title || "—"}</span>
          {it.page !== null && <span className="tnum rrowbtn__meta">{it.page + 1}</span>}
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------- signets de l'utilisateur

function Marks({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const jumpTo = useApp((s) => s.jumpTo);
  const updateTab = useApp((s) => s.updateTab);
  const [editing, setEditing] = useState<number | null>(null);

  const saveMarks = (marks: UserMark[]) => {
    updateTab(tab.key, { marks });
    void api.recentsSetMarks(tab.info.path, marks).catch(() => {});
  };
  const add = () => {
    const marks = [...tab.marks, { title: t("reader.markDefault", { n: tab.page + 1 }), page: tab.page, color: markColors[tab.marks.length % markColors.length] }];
    marks.sort((a, b) => a.page - b.page);
    saveMarks(marks);
  };

  return (
    <div className="rlist">
      {tab.marks.length === 0 && <p className="muted rise-sm">{t("reader.noMarks")}</p>}
      {tab.marks.map((m, i) => (
        <div key={i} className="rrowbtn rrowbtn--mark rise-sm" style={{ animationDelay: `${i * 45}ms` }}>
          <span className="mark-dot" style={{ background: m.color }} />
          {editing === i ? (
            <input
              className="field field--sm"
              autoFocus
              defaultValue={m.title}
              aria-label={t("reader.addMark")}
              onBlur={(e) => {
                saveMarks(tab.marks.map((x, k) => (k === i ? { ...x, title: e.target.value.trim() || x.title } : x)));
                setEditing(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                if (e.key === "Escape") setEditing(null);
              }}
            />
          ) : (
            <button type="button" className="rrowbtn__main" onClick={() => jumpTo(tab.key, m.page)} onDoubleClick={() => setEditing(i)}>
              <span style={{ flex: 1 }}>{m.title}</span>
              <span className="tnum rrowbtn__meta">p. {m.page + 1}</span>
            </button>
          )}
          <button type="button" className="rrowbtn__x" aria-label={t("reader.removeMark", { name: m.title })} onClick={() => saveMarks(tab.marks.filter((_, k) => k !== i))}>
            <Icons.close size={12} />
          </button>
        </div>
      ))}
      <Button variant="ghost" icon={<Icons.plus />} style={{ justifyContent: "flex-start", marginTop: 6 }} onClick={add}>
        {t("reader.addMark")}
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------- pièces jointes

const EXT_STYLE: Record<string, [string, string]> = {
  XLS: ["#4FCB91", "rgba(79,203,145,.14)"],
  PDF: ["#FF8C8C", "rgba(239,107,107,.14)"],
  PNG: ["#A9A6FF", "rgba(110,106,246,.16)"],
  JPG: ["#A9A6FF", "rgba(110,106,246,.16)"],
  DOC: ["#7FCDEE", "rgba(91,192,235,.15)"],
};

function Files({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const files = tab.info.attachments;
  if (!files.length) return <p className="muted rise-sm">{t("reader.noFiles")}</p>;
  const saveOne = async (index: number, name: string) => {
    const dest = await save({ defaultPath: name });
    if (!dest) return;
    try {
      await api.saveAttachment(tab.info.id, index, dest);
      toast.success(t("reader.attachmentSaved"));
    } catch (e) {
      toast.error(asAppError(e).message);
    }
  };
  return (
    <div className="rlist" style={{ gap: 6 }}>
      {files.map((f, i) => {
        const ext = (f.name.split(".").pop() ?? "").toUpperCase().slice(0, 4);
        const [fg, bg] = EXT_STYLE[ext.slice(0, 3)] ?? ["#B9BBC2", "rgba(255,255,255,.08)"];
        return (
          <button key={f.index} type="button" className="rrowbtn rise-sm" style={{ gap: 12, animationDelay: `${i * 55}ms` }} onClick={() => void saveOne(f.index, f.name)} title={t("reader.saveAttachment", { name: f.name })}>
            <span className="filetile" style={{ color: fg, background: bg }}>
              {ext || "?"}
            </span>
            <span style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
              <span className="ellipsis" style={{ fontWeight: 500 }}>
                {f.name}
              </span>
              <span className="muted">{formatSize(f.size)}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------- calques

function Layers({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const updateTab = useApp((s) => s.updateTab);
  const items = [
    { k: "annotations" as const, label: t("reader.layerNames.annotations") },
    { k: "forms" as const, label: t("reader.layerNames.forms") },
  ];
  return (
    <div className="rlist" style={{ gap: 4 }}>
      {items.map((l, i) => (
        <div key={l.k} className="rrowbtn rise-sm" style={{ animationDelay: `${i * 45}ms`, cursor: "default" }}>
          <span style={{ flex: 1, color: tab.layers[l.k] ? "var(--text)" : "var(--text-faint)", transition: "color .2s" }}>{l.label}</span>
          <Switch label={t("reader.layerToggle", { name: l.label })} checked={tab.layers[l.k]} onChange={(v) => updateTab(tab.key, { layers: { ...tab.layers, [l.k]: v } })} />
        </div>
      ))}
      <DocLayers tab={tab} />
    </div>
  );
}

/** Calques du document (contenu optionnel /OCG). */
function DocLayers({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const updateTab = useApp((s) => s.updateTab);
  const [list, setList] = useState<Layer[] | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let alive = true;
    void editApi
      .layers(tab.info.id)
      .then((l) => alive && setList(l))
      .catch(() => alive && setList([]));
    return () => {
      alive = false;
    };
  }, [tab.info.id]);
  if (!list) return null;
  if (!list.length)
    return (
      <p className="muted" style={{ marginTop: 10 }}>
        {t("reader.noDocLayers")}
      </p>
    );
  const toggle = async (id: string, visible: boolean) => {
    const next = list.map((l) => (l.id === id ? { ...l, visible } : l));
    setList(next);
    setBusy(true);
    try {
      await editApi.setLayers(tab.info.id, next.filter((l) => !l.visible).map((l) => l.id));
      updateTab(tab.key, (x) => ({ viewRev: (x.viewRev ?? 0) + 1 }));
    } catch (e) {
      toast.error(asAppError(e).message);
      setList(list);
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <p className="muted" style={{ margin: "14px 6px 4px", fontSize: 12 }}>{t("reader.docLayers")}</p>
      {list.map((l, i) => (
        <div key={l.id} className="rrowbtn rise-sm" style={{ animationDelay: `${(i + 2) * 45}ms`, cursor: "default", paddingLeft: 10 + l.depth * 14 }}>
          <span style={{ flex: 1, color: l.visible ? "var(--text)" : "var(--text-faint)", transition: "color .2s" }}>{l.name}</span>
          <Switch label={t("reader.layerToggle", { name: l.name })} checked={l.visible} disabled={busy || l.locked} onChange={(v) => void toggle(l.id, v)} />
        </div>
      ))}
      <p className="muted" style={{ marginTop: 10 }}>
        {t("reader.layersHint")}
      </p>
    </>
  );
}
