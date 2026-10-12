import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Segmented } from "./Segmented";
import { Icons } from "./icons";
import { fontCss } from "../lib/annot";

export interface SystemFont {
  family: string;
  bold: boolean;
  italic: boolean;
  mono: boolean;
}

let cache: Promise<SystemFont[]> | null = null;
/** Polices installées (recensées une fois par le cœur Rust). */
export function systemFonts(): Promise<SystemFont[]> {
  cache ??= invoke<SystemFont[]>("system_fonts").catch(() => {
    cache = null;
    return [];
  });
  return cache;
}

const BASE = ["sans", "serif", "mono"] as const;

/**
 * Choix de police : les trois familles standard du PDF, plus un menu
 * « Autres polices » listant les polices installées (chacune dans son style).
 */
export function FontPicker({ value, onChange, block, disabled }: { value: string; onChange: (font: string) => void; block?: boolean; disabled?: boolean }) {
  const { t } = useTranslation();
  const btn = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const isBase = (BASE as readonly string[]).includes(value);
  return (
    <div className="fpick">
      <Segmented
        block={block}
        size="sm"
        label={t("annot.font")}
        value={isBase ? value : "__sys"}
        onChange={(f) => onChange(f)}
        options={BASE.map((f) => ({
          value: f,
          title: t(`annot.fontsLong.${f}`),
          label: <span style={{ fontFamily: fontCss(f), fontSize: 13 }}>{t(`annot.fonts.${f}`)}</span>,
        }))}
      />
      <button
        ref={btn}
        type="button"
        className={`fpick__more ${!isBase ? "is-on" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        title={t("fonts.more")}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="fpick__name" style={{ fontFamily: isBase ? undefined : fontCss(value) }}>
          {isBase ? t("fonts.others") : value}
        </span>
        <Icons.chevronDown />
      </button>
      {open && (
        <FontList
          anchor={btn}
          value={value}
          onPick={(f) => {
            onChange(f);
            setOpen(false);
          }}
          onClose={() => setOpen(false)}
        />
      )}
    </div>
  );
}

function FontList({ anchor, value, onPick, onClose }: { anchor: React.RefObject<HTMLButtonElement | null>; value: string; onPick: (f: string) => void; onClose: () => void }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [fonts, setFonts] = useState<SystemFont[] | null>(null);
  const [q, setQ] = useState("");
  const [active, setActive] = useState(0);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useEffect(() => {
    void systemFonts().then(setFonts);
    const r = anchor.current?.getBoundingClientRect();
    if (r) {
      const w = 280;
      const left = Math.max(12, Math.min(r.right - w, window.innerWidth - w - 12));
      const below = r.bottom + 10 + 380 < window.innerHeight;
      setPos({ top: below ? r.bottom + 8 : Math.max(12, r.top - 388), left });
    }
    const id = window.setTimeout(() => input.current?.focus(), 30);
    const onDown = (e: PointerEvent) => {
      const el = e.target as Node;
      if (ref.current?.contains(el) || anchor.current?.contains(el)) return;
      onClose();
    };
    window.addEventListener("pointerdown", onDown, true);
    return () => {
      window.clearTimeout(id);
      window.removeEventListener("pointerdown", onDown, true);
    };
  }, [anchor, onClose]);

  const list = useMemo(() => {
    const n = q.trim().toLowerCase();
    return (fonts ?? []).filter((f) => !n || f.family.toLowerCase().includes(n));
  }, [fonts, q]);

  useEffect(() => {
    ref.current?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
      anchor.current?.focus();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, Math.min(list.length - 1, a + (e.key === "ArrowDown" ? 1 : -1))));
    } else if (e.key === "Enter" && list[active]) {
      e.preventDefault();
      onPick(list[active].family);
    }
  };

  if (!pos) return null;
  return createPortal(
    <div ref={ref} className="cpop fpop" role="dialog" aria-label={t("fonts.more")} style={{ top: pos.top, left: pos.left }} onKeyDown={onKey}>
      <input
        ref={input}
        className="field field--sm"
        placeholder={t("fonts.search")}
        value={q}
        spellCheck={false}
        onChange={(e) => {
          setQ(e.target.value);
          setActive(0);
        }}
      />
      <div className="fpop__list" role="listbox" aria-label={t("fonts.more")}>
        {!fonts && <span className="muted fpop__empty">{t("fonts.loading")}</span>}
        {fonts && !list.length && <span className="muted fpop__empty">{t("fonts.none")}</span>}
        {list.map((f, i) => (
          <button
            key={f.family}
            type="button"
            role="option"
            data-i={i}
            aria-selected={f.family === value}
            className={`fpop__item ${i === active ? "is-active" : ""}`}
            style={{ fontFamily: fontCss(f.family) }}
            onMouseEnter={() => setActive(i)}
            onClick={() => onPick(f.family)}
          >
            <span>{f.family}</span>
            {f.family === value && <Icons.check />}
          </button>
        ))}
      </div>
      <p className="muted fpop__hint">{t("fonts.hint")}</p>
    </div>,
    document.body,
  );
}
