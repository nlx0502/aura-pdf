import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { FontPicker } from "../../components/FontPicker";
import { ColorDots, IconButton, Icons, Segmented, Slider, Stepper, ToastHost, type IconName } from "../../components";
import {
  blankAnnot,
  bounds,
  COLORS,
  rgba,
  STAMPS,
  stampSize,
  TEXT_COLORS,
  textBoxHeight,
  TOOLS,
  WIDTHS,
  type Annot,
  type AnnotKind,
  type Tool,
} from "../../lib/annot";
import type { TextStyle } from "../../stores/annot";
import { isMod, isTyping, shortcut } from "../../lib/platform";
import { perms } from "../../lib/api";
import { useAnnots, useDocAnnots } from "../../stores/annot";
import { useActiveTab, useApp, type DocTab } from "../../stores/app";
import { Viewer } from "../reader/Viewer";
import "./annot.css";

const TOOL_ICON: Record<Tool, IconName> = {
  select: "pointer",
  highlight: "highlighter",
  underline: "underline",
  strike: "strike",
  note: "note",
  text: "textBox",
  pen: "pen",
  arrow: "arrowShape",
  rect: "rect",
  ellipse: "ellipse",
  line: "line",
};

export const KIND_ICON: Record<AnnotKind, IconName> = {
  highlight: "highlighter",
  underline: "underline",
  strike: "strike",
  squiggly: "underline",
  note: "note",
  text: "textBox",
  ink: "pen",
  arrow: "arrowShape",
  rect: "rect",
  ellipse: "ellipse",
  line: "line",
  stamp: "stamp",
};

function authorName() {
  const s = useApp.getState();
  return s.prefs.userName.trim() || s.systemName || "Vous";
}

/** Module Annoter (maquette Annotate.dc.html). */
export function Annotate() {
  const tab = useActiveTab();
  const ensure = useAnnots((s) => s.ensure);
  useEffect(() => {
    if (tab) void ensure(tab.key, tab.info.id);
  }, [tab?.key, tab?.info.id, ensure]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!tab) return null;
  return <AnnotateInner tab={tab} />;
}

function AnnotateInner({ tab }: { tab: DocTab }) {
  const { t } = useTranslation();
  const d = useDocAnnots(tab.key);
  const store = useAnnots();

  // ---- Raccourcis du module
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e) || document.querySelector(".scrim")) return;
      const s = useAnnots.getState();
      const k = e.key.toLowerCase();
      if (isMod(e)) {
        if (k === "z" && !e.shiftKey) s.undo(tab.key);
        else if (k === "y" || (k === "z" && e.shiftKey)) s.redo(tab.key);
        else return;
        e.preventDefault();
        return;
      }
      if (e.altKey) return;
      const cur = s.docs[tab.key];
      if ((e.key === "Delete" || e.key === "Backspace") && cur?.selected) {
        e.preventDefault();
        s.commit(tab.key, (l) => l.filter((a) => a.id !== cur.selected), null);
        return;
      }
      if (e.key === "Escape") {
        if (cur?.selected) s.select(tab.key, null);
        else s.setTool("select");
        return;
      }
      const tool = TOOLS.find((x) => x.key.toLowerCase() === k);
      if (tool) {
        e.preventDefault();
        s.setTool(tool.id);
      }
    };
    // Clic dans le vide du document : désélection.
    const onDown = (e: PointerEvent) => {
      const el = e.target as HTMLElement;
      if (!el.closest(".viewer") || el.closest(".ann, .hud, .ann__editor")) return;
      const s = useAnnots.getState();
      if (s.tool === "select" && s.docs[tab.key]?.selected) s.select(tab.key, null);
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [tab.key]);

  if (!d?.loaded) return <div className="workspace" />;
  const readOnly = !perms(tab.info).annotate;

  return (
    <div className="workspace annotate">
      <AnnotList tab={tab} list={d.list} selected={d.selected} />
      <main className="panel annotate__main">
        <div className="atoolbar" role="toolbar" aria-label={t("annot.toolbar")}>
          {TOOLS.map((tool) => {
            const I = Icons[TOOL_ICON[tool.id]];
            return (
              <span key={tool.id} style={{ display: "contents" }}>
                {tool.sep && <span className="atoolbar__sep" />}
                <span className="tip-wrap">
                  <button
                    type="button"
                    className="tool"
                    aria-pressed={store.tool === tool.id}
                    aria-label={`${t(`annot.tools.${tool.id}`)} (${tool.key})`}
                    onClick={() => store.setTool(tool.id)}
                    disabled={readOnly && tool.id !== "select"}
                  >
                    <I />
                  </button>
                  <span className="tip" aria-hidden="true">
                    {t(`annot.tools.${tool.id}`)}
                    <span className="kbd">{tool.key}</span>
                  </span>
                </span>
              </span>
            );
          })}
          <span className="atoolbar__sep" style={{ margin: "0 8px 0 5px" }} />
          <div className="atoolbar__colors">
            {(() => {
              const selAnnot = d.list.find((a) => a.id === d.selected);
              const textMode = store.tool === "text" || (store.tool === "select" && selAnnot?.kind === "text");
              const value = store.tool === "select" && selAnnot ? selAnnot.color : textMode ? store.textStyle.color : store.style.color;
              return (
                <ColorDots
                  size="sm"
                  presets={textMode ? TEXT_COLORS : COLORS}
                  value={value}
                  label={t("annot.color")}
                  onChange={(hex) => {
                    if (!hex) return;
                    if (textMode) store.setTextStyle({ color: hex });
                    else store.setStyle({ color: hex });
                    // Avec l'outil Sélection, la couleur s'applique aussi à l'annotation choisie.
                    if (d.selected && store.tool === "select")
                      store.commit(tab.key, (l) => l.map((a) => (a.id === d.selected && !a.locked ? { ...a, color: hex, geomDirty: true } : a)));
                  }}
                />
              );
            })()}
          </div>
          <span className="atoolbar__sep" style={{ margin: "0 4px 0 2px" }} />
          <IconButton label={t("annot.undo")} shortcut={shortcut("Z")} icon={<Icons.undo />} onClick={() => store.undo(tab.key)} disabled={!d.past.length} />
          <IconButton label={t("annot.redo")} shortcut={shortcut("Y")} icon={<Icons.redo />} onClick={() => store.redo(tab.key)} disabled={!d.future.length} />
          <IconButton
            label={t("annot.save")}
            shortcut={shortcut("S")}
            tipAlign="end"
            variant={d.dirty ? "primary" : "ghost"}
            icon={<Icons.save />}
            onClick={() => void store.save(tab.key)}
            disabled={!d.dirty || d.saving || readOnly}
          />
        </div>
        {readOnly && <div className="annotate__notice">{t("annot.encrypted")}</div>}
        <Viewer key={tab.key} tab={tab} active annotate={!readOnly} />
        <ToastHost only="right" contained />
      </main>
      <AnnotSide tab={tab} list={d.list} selected={d.selected} readOnly={readOnly} />
    </div>
  );
}

// ---------------------------------------------------------------- liste des annotations

function subtitle(a: Annot, t: (k: string, o?: Record<string, unknown>) => string): string {
  if (a.kind === "stamp") return t(`annot.stampLabels.${a.text}`, { defaultValue: a.text });
  if (a.text) return a.text;
  if (a.contents) return a.contents;
  return t("annot.pageN", { n: a.page + 1 });
}

function AnnotList({ tab, list, selected }: { tab: DocTab; list: Annot[]; selected: string | null }) {
  const { t } = useTranslation();
  const sorted = useMemo(() => [...list].sort((a, b) => a.page - b.page || bounds(a).y - bounds(b).y), [list]);
  const store = useAnnots.getState();
  const jumpTo = useApp((s) => s.jumpTo);
  return (
    <aside className="panel apanel apanel--left" aria-label={t("annot.title")}>
      <div className="rpanel__head" style={{ padding: "0 4px" }}>
        <h2 className="title">{t("annot.title")}</h2>
        <span className="muted tnum">{list.length}</span>
      </div>
      <div className="alist">
        {sorted.length === 0 && <p className="muted rise-sm" style={{ padding: "4px 6px" }}>{t("annot.empty")}</p>}
        {sorted.map((a, i) => {
          const I = Icons[KIND_ICON[a.kind]];
          return (
            <div
              key={a.id}
              className={`aitem rise-sm ${selected === a.id ? "is-sel" : ""}`}
              style={{ animationDelay: `${Math.min(i, 12) * 30}ms` }}
              onClick={() => {
                store.select(tab.key, a.id);
                jumpTo(tab.key, a.page, bounds(a).y);
              }}
            >
              <span className="aitem__tile" style={{ background: rgba(a.color, 0.16), color: a.color }}>
                <I />
              </span>
              <span className="aitem__text">
                <span className="aitem__kind">
                  {t(`annot.kinds.${a.kind}`)}
                  <span className="muted tnum" style={{ fontSize: 11.5 }}>
                    {" "}· p. {a.page + 1}
                  </span>
                </span>
                <span className="muted ellipsis">{subtitle(a, t)}</span>
              </span>
              <button
                type="button"
                className="btn btn--ghost btn--icon aitem__x"
                aria-label={t("annot.removeNamed", { name: t(`annot.kinds.${a.kind}`) })}
                onClick={(e) => {
                  e.stopPropagation();
                  store.commit(tab.key, (l) => l.filter((x) => x.id !== a.id), null);
                }}
              >
                <Icons.close size={12} />
              </button>
            </div>
          );
        })}
      </div>
      <p className="muted apanel__hint">{t("annot.hint")}</p>
    </aside>
  );
}

// ---------------------------------------------------------------- panneau droit : style, tampons, commentaires

function AnnotSide({ tab, list, selected, readOnly }: { tab: DocTab; list: Annot[]; selected: string | null; readOnly: boolean }) {
  const { t } = useTranslation();
  const store = useAnnots();
  const sel = list.find((a) => a.id === selected) ?? null;
  const isHl = sel ? sel.kind === "highlight" : store.tool === "highlight";
  const opacity = sel ? sel.opacity : isHl ? store.highlightOpacity : store.style.opacity;
  const width = sel && !sel.kind.match(/highlight|underline|strike|note|stamp/) ? sel.width : store.style.width;
  const label = sel ? t(`annot.kinds.${sel.kind}`) : t(`annot.tools.${store.tool}`);

  const setOpacity = (v: number) => {
    const o = v / 100;
    if (isHl) store.setHighlightOpacity(o);
    else store.setStyle({ opacity: o });
    if (sel && !sel.locked) store.commit(tab.key, (l) => l.map((a) => (a.id === sel.id ? { ...a, opacity: o, geomDirty: true } : a)));
  };
  const setWidth = (w: number) => {
    store.setStyle({ width: w });
    if (sel && !sel.locked) store.commit(tab.key, (l) => l.map((a) => (a.id === sel.id ? { ...a, width: w, geomDirty: true } : a)));
  };
  const addStamp = (s: (typeof STAMPS)[number]) => {
    const page = tab.page;
    const size = tab.info.pages[page];
    const { w, h } = stampSize(s.label);
    // Placé au tiers supérieur de la page courante, décalé pour ne pas empiler les tampons.
    const n = list.filter((a) => a.kind === "stamp" && a.page === page).length;
    const a = blankAnnot({
      page,
      kind: "stamp",
      color: s.color,
      text: s.label,
      rect: { x: Math.max(10, size.w / 2 - w / 2 + (n % 3) * 24), y: Math.max(10, size.h * 0.18 + n * 46), w, h },
      author: authorName(),
    });
    store.commit(tab.key, (l) => [...l, a], a.id);
    store.setTool("select");
  };

  return (
    <aside className="panel apanel apanel--right" aria-label={t("annot.style")}>
      <section className="rsec" style={{ gap: 14 }}>
        <div className="rpanel__head">
          <h2 className="title">{t("annot.style")}</h2>
          <span className="muted">{label}</span>
        </div>
        <div className="aside__field">
          <span className="aside__row">
            <span style={{ color: "var(--text-2)" }}>{t("annot.opacity")}</span>
          </span>
          <Slider label={t("annot.opacity")} value={Math.round(opacity * 100)} min={20} max={100} step={5} onChange={setOpacity} format={(v) => `${v} %`} />
        </div>
        <div className="aside__field">
          <span style={{ color: "var(--text-2)", fontSize: 13 }}>{t("annot.thickness")}</span>
          <Segmented
            block
            label={t("annot.thickness")}
            value={String(width)}
            onChange={(v) => setWidth(+v)}
            options={WIDTHS.map((w) => ({ value: String(w), ariaLabel: t("annot.thicknessN", { n: w }), title: t("annot.thicknessN", { n: w }), icon: <span className="wbar" style={{ height: Math.max(1.5, w * 1.6) }} /> }))}
          />
        </div>
        {sel?.locked && <p className="muted">{t("annot.locked")}</p>}
      </section>

      {(store.tool === "text" || sel?.kind === "text") && <TextPanel tab={tab} sel={sel?.kind === "text" ? sel : null} />}

      <section className="rsec" style={{ gap: 12 }}>
        <h2 className="title">{t("annot.stamps")}</h2>
        <div className="astamps">
          {STAMPS.map((s) => (
            <button key={s.label} type="button" className="btn astamp" style={{ color: s.text, background: rgba(s.color, 0.16) }} onClick={() => addStamp(s)} disabled={readOnly}>
              {t(`annot.stampLabels.${s.label}`)}
            </button>
          ))}
        </div>
      </section>

      <Comments tab={tab} list={list} selected={sel} readOnly={readOnly} />
    </aside>
  );
}

// ---------------------------------------------------------------- style des zones de texte

function TextPanel({ tab, sel }: { tab: DocTab; sel: Annot | null }) {
  const { t } = useTranslation();
  const store = useAnnots();
  // Valeurs : celles de la zone sélectionnée, sinon le style des prochaines zones.
  const v: TextStyle = sel
    ? { font: sel.font, fontSize: sel.fontSize, bold: sel.bold, italic: sel.italic, align: sel.align, color: sel.color, fill: sel.fill, borderColor: sel.width > 0 ? sel.borderColor : null }
    : store.textStyle;

  const apply = (patch: Partial<TextStyle>) => {
    store.setTextStyle(patch);
    if (!sel || sel.locked) return;
    store.commit(tab.key, (l) =>
      l.map((a) => {
        if (a.id !== sel.id) return a;
        const next = { ...a, ...patch, geomDirty: true };
        if ("borderColor" in patch) next.width = patch.borderColor ? Math.max(1, a.width || store.style.width) : 0;
        // La hauteur suit la police et la taille.
        next.rect = a.rect ? { ...a.rect, h: textBoxHeight(next, a.rect.w) } : a.rect;
        return next;
      }),
    );
  };

  return (
    <section className="rsec rise-sm" style={{ gap: 14 }}>
      <h2 className="title">{t("annot.textStyle")}</h2>
      <div className="aside__field">
        <span className="aside__label">{t("annot.font")}</span>
        <FontPicker block value={v.font} onChange={(font) => apply({ font })} />
      </div>
      <div className="aside__row aside__row--center">
        <span className="aside__label">{t("annot.fontSize")}</span>
        <span style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <IconButton size="sm" label={t("annot.bold")} on={v.bold} aria-pressed={v.bold} icon={<Icons.bold />} onClick={() => apply({ bold: !v.bold })} />
          <IconButton size="sm" label={t("annot.italic")} on={v.italic} aria-pressed={v.italic} icon={<Icons.italic />} onClick={() => apply({ italic: !v.italic })} />
          <Stepper value={v.fontSize} min={6} max={96} onChange={(fontSize) => apply({ fontSize })} decLabel={t("annot.smaller")} incLabel={t("annot.bigger")} />
        </span>
      </div>
      <div className="aside__row aside__row--center">
        <span className="aside__label">{t("annot.align")}</span>
        <Segmented
          size="sm"
          label={t("annot.align")}
          value={String(v.align)}
          onChange={(a) => apply({ align: +a })}
          itemWidth={40}
          options={[
            { value: "0", icon: <Icons.alignLeft />, ariaLabel: t("annot.aligns.left"), title: t("annot.aligns.left") },
            { value: "1", icon: <Icons.alignCenter />, ariaLabel: t("annot.aligns.center"), title: t("annot.aligns.center") },
            { value: "2", icon: <Icons.alignRight />, ariaLabel: t("annot.aligns.right"), title: t("annot.aligns.right") },
          ]}
        />
      </div>
      <div className="aside__field">
        <span className="aside__label">{t("annot.textColor")}</span>
        <ColorDots presets={TEXT_COLORS} value={v.color} label={t("annot.textColor")} onChange={(c) => c && apply({ color: c })} />
      </div>
      <div className="aside__field">
        <span className="aside__label">{t("annot.fill")}</span>
        <ColorDots allowNone presets={[{ name: "blanc", hex: "#FFFFFF" }, { name: "jaune pâle", hex: "#FFF4C2" }, { name: "bleu pâle", hex: "#E3EEFF" }]} value={v.fill} label={t("annot.fill")} onChange={(fill) => apply({ fill })} />
      </div>
      <div className="aside__field">
        <span className="aside__label">{t("annot.border")}</span>
        <ColorDots allowNone presets={TEXT_COLORS.slice(0, 3)} value={v.borderColor} label={t("annot.border")} onChange={(borderColor) => apply({ borderColor })} />
      </div>
    </section>
  );
}

// ---------------------------------------------------------------- commentaires

const AVATAR = ["#4FCB91", "#8B87FA", "#F2B84B", "#F07A9A", "#5BC0EB"];
function avatarColor(name: string) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) | 0;
  return AVATAR[Math.abs(h) % AVATAR.length];
}
function initials(name: string) {
  const p = name.trim().split(/\s+/);
  return ((p[0]?.[0] ?? "?") + (p[1]?.[0] ?? p[0]?.[1] ?? "")).toUpperCase();
}
function when(raw: string | null, dirty: boolean, t: (k: string) => string): string {
  if (dirty || !raw) return t("annot.justNow");
  const m = raw.match(/^(?:D:)?(\d{4})(\d{2})(\d{2})(\d{2})?(\d{2})?/);
  if (!m) return "";
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], +(m[4] ?? 0), +(m[5] ?? 0)));
  const diff = (Date.now() - d.getTime()) / 60000;
  if (diff < 1) return t("annot.justNow");
  if (diff < 60) return `Il y a ${Math.round(diff)} min`;
  if (diff < 60 * 24) return `Il y a ${Math.round(diff / 60)} h`;
  if (diff < 60 * 48) return "Hier";
  return d.toLocaleDateString("fr-FR", { day: "numeric", month: "short" });
}

function Comments({ tab, list, selected, readOnly }: { tab: DocTab; list: Annot[]; selected: Annot | null; readOnly: boolean }) {
  const { t } = useTranslation();
  const store = useAnnots.getState();
  const jumpTo = useApp((s) => s.jumpTo);
  const [filter, setFilter] = useState<"all" | "open" | "done">("all");
  const [draft, setDraft] = useState("");
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [replyDraft, setReplyDraft] = useState("");
  const [target, setTarget] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // « Commenter » (barre flottante, nouvelle note) : cible l'annotation et place le curseur.
  useEffect(() => {
    const on = (e: Event) => {
      const id = (e as CustomEvent<string>).detail;
      setTarget(id);
      store.select(tab.key, id);
      window.setTimeout(() => inputRef.current?.focus(), 30);
    };
    window.addEventListener("aura:comment", on);
    return () => window.removeEventListener("aura:comment", on);
  }, [tab.key, store]);

  const threads = list.filter((a) => a.contents || a.replies.length || a.kind === "note");
  const shown = threads.filter((a) => filter === "all" || (filter === "open" ? !a.resolved : a.resolved));
  const targetAnnot = list.find((a) => a.id === (target ?? selected?.id)) ?? null;

  const send = () => {
    const text = draft.trim();
    if (!text) return;
    const author = authorName();
    if (targetAnnot) {
      store.commit(tab.key, (l) =>
        l.map((a) =>
          a.id !== targetAnnot.id
            ? a
            : a.contents
              ? { ...a, replies: [...a.replies, { author, text, date: null, saved: false }] }
              : { ...a, contents: text, metaDirty: true },
        ),
      );
    } else {
      // Commentaire libre : une note dans la marge de la page courante.
      const size = tab.info.pages[tab.page];
      const n = list.filter((a) => a.kind === "note" && a.page === tab.page).length;
      const a = blankAnnot({ page: tab.page, kind: "note", color: "#F2B84B", contents: text, author, rect: { x: size.w - 34, y: 40 + n * 30, w: 22, h: 22 } });
      store.commit(tab.key, (l) => [...l, a], a.id);
    }
    setDraft("");
    setTarget(null);
    setFilter((f) => (f === "done" ? "all" : f));
  };

  const sendReply = (a: Annot) => {
    const text = replyDraft.trim();
    if (!text) return;
    store.commit(tab.key, (l) => l.map((x) => (x.id === a.id ? { ...x, replies: [...x.replies, { author: authorName(), text, date: null, saved: false }] } : x)));
    setReplyDraft("");
    setReplyTo(null);
  };

  return (
    <section className="rsec acomments" style={{ gap: 12 }}>
      <div className="acomments__head">
        <h2 className="title">{t("annot.comments")}</h2>
        <Segmented
          size="sm"
          label={t("annot.filterLabel")}
          value={filter}
          onChange={setFilter}
          itemWidth={68}
          options={(["all", "open", "done"] as const).map((f) => ({ value: f, label: t(`annot.filters.${f}`) }))}
        />
      </div>
      {shown.map((a, i) => {
        const quote = a.kind.match(/highlight|underline|strike|squiggly/) ? a.text : "";
        return (
          <article
            key={a.id}
            className={`acard rise-sm ${a.resolved ? "is-resolved" : ""} ${selected?.id === a.id ? "is-sel" : ""}`}
            style={{ animationDelay: `${Math.min(i, 10) * 40}ms` }}
            onClick={() => {
              store.select(tab.key, a.id);
              jumpTo(tab.key, a.page, bounds(a).y);
            }}
          >
            <div className="acard__head">
              <span className="acard__avatar" style={{ background: avatarColor(a.author || "?") }}>
                {initials(a.author || t("annot.you"))}
              </span>
              <span className="acard__who">{a.author || t("annot.you")}</span>
              <span className="muted">{when(a.modified, a.source === null || a.metaDirty, t)}</span>
            </div>
            {quote && <p className="acard__quote">« {quote.length > 90 ? `${quote.slice(0, 90)}…` : quote} »</p>}
            {a.contents && <p className="acard__text">{a.contents}</p>}
            {a.replies.map((r, k) => (
              <div key={k} className="acard__reply">
                <span className="acard__avatar acard__avatar--sm" style={{ background: avatarColor(r.author || "?") }}>
                  {initials(r.author || "?")}
                </span>
                <span>
                  <b>{r.author || t("annot.you")}</b> {r.text}
                </span>
              </div>
            ))}
            {replyTo === a.id && (
              <input
                className="field field--sm field--pill"
                autoFocus
                placeholder={t("annot.replyPlaceholder")}
                value={replyDraft}
                onClick={(e) => e.stopPropagation()}
                onChange={(e) => setReplyDraft(e.target.value)}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === "Enter") sendReply(a);
                  if (e.key === "Escape") setReplyTo(null);
                }}
              />
            )}
            {!readOnly && (
              <div className="acard__actions" onClick={(e) => e.stopPropagation()}>
                <button type="button" className="btn btn--ghost btn--sm" onClick={() => setReplyTo(replyTo === a.id ? null : a.id)}>
                  <Icons.reply />
                  {t("annot.reply")}
                </button>
                <button
                  type="button"
                  className={`btn btn--ghost btn--sm ${a.resolved ? "is-done" : ""}`}
                  onClick={() => store.commit(tab.key, (l) => l.map((x) => (x.id === a.id ? { ...x, resolved: !x.resolved } : x)))}
                >
                  {a.resolved ? <Icons.resolved className="i acard__check" /> : <Icons.ellipse />}
                  {a.resolved ? t("annot.resolvedLabel") : t("annot.resolve")}
                </button>
              </div>
            )}
          </article>
        );
      })}
      {shown.length === 0 && <p className="muted" style={{ margin: 0 }}>{t("annot.noComments")}</p>}
      {!readOnly && (
        <div className="acomments__input">
          <input
            ref={inputRef}
            className="field field--pill"
            placeholder={targetAnnot ? t("annot.newCommentFor", { name: t(`annot.kinds.${targetAnnot.kind}`) }) : t("annot.newComment")}
            value={draft}
            aria-label={t("annot.newComment")}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") send();
            }}
          />
          <button type="button" className="btn btn--primary btn--icon acomments__send" aria-label={t("annot.send")} onClick={send}>
            <Icons.send />
          </button>
        </div>
      )}
    </section>
  );
}
