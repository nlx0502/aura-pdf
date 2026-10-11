import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { api } from "../lib/api";

/** Couleurs proposées dans la palette étendue. */
export const EXTENDED = [
  "#1A1B1E", "#5E616A", "#9A9DA6", "#FFFFFF",
  "#D2453D", "#EF6B6B", "#F07A9A", "#C49BF5",
  "#F28C38", "#F2B84B", "#F5D547", "#B7D95A",
  "#4FCB91", "#1F9D6A", "#5BC0EB", "#2563C9",
  "#6E6AF6", "#8B87FA", "#7A4E2D", "#C9B8A6",
];

/** Couleurs récentes (mémorisées dans les préférences). */
const useRecent = create<{ colors: string[]; loaded: boolean }>(() => ({ colors: [], loaded: false }));

function pushRecent(hex: string) {
  const up = hex.toUpperCase();
  const colors = [up, ...useRecent.getState().colors.filter((c) => c !== up)].slice(0, 8);
  useRecent.setState({ colors });
  void api.prefsSet("recentColors", colors).catch(() => {});
}

const HEX_RE = /^#?([0-9a-f]{6})$/i;

export interface ColorDotsProps {
  value: string | null;
  onChange: (hex: string | null) => void;
  /** Couleurs de base affichées en bulles. */
  presets: { hex: string; name: string }[];
  /** Propose « Aucune » (fond, bordure). */
  allowNone?: boolean;
  label: string;
  size?: "md" | "sm";
}

/**
 * Bulles de couleur + bulle « autre couleur » (palette étendue, couleur libre,
 * récentes). La valeur hors préréglages est affichée dans la dernière bulle.
 */
export function ColorDots({ value, onChange, presets, allowNone, label, size = "md" }: ColorDotsProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);
  const isPreset = value !== null && presets.some((p) => p.hex.toUpperCase() === value.toUpperCase());
  const custom = value !== null && !isPreset ? value : null;

  useEffect(() => {
    if (useRecent.getState().loaded) return;
    useRecent.setState({ loaded: true });
    void api
      .prefsGet()
      .then((p) => Array.isArray(p.recentColors) && useRecent.setState({ colors: p.recentColors as string[] }))
      .catch(() => {});
  }, []);

  const pick = (hex: string | null) => {
    if (hex && !presets.some((p) => p.hex.toUpperCase() === hex.toUpperCase())) pushRecent(hex);
    onChange(hex);
  };

  return (
    <div className={`cdots ${size === "sm" ? "cdots--sm" : ""}`} role="group" aria-label={label}>
      {allowNone && (
        <button type="button" className="dot dot--none" aria-pressed={value === null} aria-label={t("color.none")} title={t("color.none")} onClick={() => pick(null)} />
      )}
      {presets.map((c) => (
        <button
          key={c.hex}
          type="button"
          className="dot"
          style={{ background: c.hex, color: c.hex }}
          aria-pressed={value !== null && value.toUpperCase() === c.hex.toUpperCase()}
          aria-label={`${label} ${c.name}`}
          title={c.name}
          onClick={() => pick(c.hex)}
        />
      ))}
      <button
        ref={btnRef}
        type="button"
        className={`dot dot--more ${custom ? "has-color" : ""}`}
        style={custom ? { background: custom, color: custom } : undefined}
        aria-pressed={!!custom}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={t("color.more")}
        title={t("color.more")}
        onClick={() => setOpen((o) => !o)}
      >
        {!custom && <span aria-hidden="true">+</span>}
      </button>
      {open && <ColorPopover anchor={btnRef} value={value} onPick={(h) => pick(h)} onClose={() => setOpen(false)} />}
    </div>
  );
}

function ColorPopover({ anchor, value, onPick, onClose }: { anchor: React.RefObject<HTMLButtonElement | null>; value: string | null; onPick: (hex: string) => void; onClose: () => void }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const recent = useRecent((s) => s.colors);
  const [hex, setHex] = useState((value ?? "#6E6AF6").toUpperCase());
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useEffect(() => {
    const r = anchor.current?.getBoundingClientRect();
    if (!r) return;
    const w = 248;
    const left = Math.max(12, Math.min(r.left + r.width / 2 - w / 2, window.innerWidth - w - 12));
    const below = r.bottom + 10 + 330 < window.innerHeight;
    setPos({ top: below ? r.bottom + 10 : Math.max(12, r.top - 340), left });
  }, [anchor]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      const el = e.target as Node;
      if (ref.current?.contains(el) || anchor.current?.contains(el)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [onClose, anchor]);

  const choose = (h: string) => {
    setHex(h.toUpperCase());
    onPick(h.toUpperCase());
  };
  const valid = HEX_RE.test(hex);

  if (!pos) return null;
  return createPortal(
    <div ref={ref} className="cpop" role="dialog" aria-label={t("color.more")} style={{ top: pos.top, left: pos.left }}>
      <span className="cpop__h">{t("color.palette")}</span>
      <div className="cpop__grid">
        {EXTENDED.map((c) => (
          <button
            key={c}
            type="button"
            className="cpop__swatch"
            style={{ background: c }}
            aria-pressed={value?.toUpperCase() === c}
            aria-label={c}
            title={c}
            onClick={() => {
              choose(c);
              onClose();
            }}
          />
        ))}
      </div>
      {recent.length > 0 && (
        <>
          <span className="cpop__h">{t("color.recent")}</span>
          <div className="cpop__grid">
            {recent.map((c) => (
              <button key={c} type="button" className="cpop__swatch" style={{ background: c }} aria-label={c} title={c} onClick={() => { choose(c); onClose(); }} />
            ))}
          </div>
        </>
      )}
      <span className="cpop__h">{t("color.custom")}</span>
      <div className="cpop__custom">
        <label className="cpop__native" style={{ background: valid ? `#${hex.replace("#", "")}` : "#888" }} title={t("color.system")}>
          <input
            type="color"
            aria-label={t("color.system")}
            value={valid ? `#${hex.replace("#", "").toLowerCase()}` : "#6e6af6"}
            onChange={(e) => setHex(e.target.value.toUpperCase())}
          />
        </label>
        <input
          className="field field--sm cpop__hex"
          value={hex}
          aria-label={t("color.hex")}
          spellCheck={false}
          onChange={(e) => setHex(e.target.value.toUpperCase())}
          onKeyDown={(e) => {
            if (e.key === "Enter" && valid) {
              choose(`#${hex.replace("#", "")}`);
              onClose();
            }
          }}
        />
        <button type="button" className="btn btn--primary btn--sm" disabled={!valid} onClick={() => { choose(`#${hex.replace("#", "")}`); onClose(); }}>
          {t("common.ok")}
        </button>
      </div>
    </div>,
    document.body,
  );
}
