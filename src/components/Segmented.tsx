import { useLayoutEffect, useRef, useState, type ReactNode } from "react";

export interface SegOption<T extends string> {
  value: T;
  label?: ReactNode;
  icon?: ReactNode;
  /** Libellé accessible (obligatoire si l'option n'a qu'une icône). */
  ariaLabel?: string;
  title?: string;
}

export interface SegmentedProps<T extends string> {
  options: SegOption<T>[];
  value: T;
  onChange: (v: T) => void;
  label: string;
  block?: boolean;
  size?: "sm" | "md";
  /** Largeur fixe de chaque option (px). */
  itemWidth?: number;
  /** Rôle « onglets » (panneaux) plutôt que groupe de boutons. */
  tabs?: boolean;
  className?: string;
}

/**
 * Contrôle segmenté avec indicateur qui glisse sous l'option choisie.
 * Navigation au clavier : flèches gauche/droite.
 */
export function Segmented<T extends string>({ options, value, onChange, label, block, size = "md", itemWidth, tabs, className }: SegmentedProps<T>) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [ind, setInd] = useState<{ x: number; w: number } | null>(null);

  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const measure = () => {
      const idx = options.findIndex((o) => o.value === value);
      const btn = root.querySelectorAll<HTMLButtonElement>(".seg__btn")[idx];
      // Aucune option choisie (valeur hors liste) : pas d'indicateur.
      setInd(btn ? { x: btn.offsetLeft, w: btn.offsetWidth } : null);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(root);
    return () => ro.disconnect();
  }, [value, options]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const idx = options.findIndex((o) => o.value === value);
    const next = (idx + (e.key === "ArrowRight" ? 1 : -1) + options.length) % options.length;
    onChange(options[next].value);
    const btns = rootRef.current?.querySelectorAll<HTMLButtonElement>(".seg__btn");
    btns?.[next]?.focus();
  };

  return (
    <div
      ref={rootRef}
      className={`seg ${block ? "seg--block" : ""} ${size === "sm" ? "seg--sm" : ""} ${className ?? ""}`}
      role={tabs ? "tablist" : "group"}
      aria-label={label}
      onKeyDown={onKey}
    >
      {ind && <span className="seg__ind" aria-hidden="true" style={{ width: ind.w, transform: `translateX(${ind.x}px)` }} />}
      {options.map((o, i) => {
        const on = o.value === value;
        const focusable = on || (i === 0 && !options.some((x) => x.value === value));
        return (
          <button
            key={o.value}
            type="button"
            className="seg__btn"
            style={itemWidth ? { width: itemWidth, padding: 0 } : undefined}
            role={tabs ? "tab" : undefined}
            aria-selected={tabs ? on : undefined}
            aria-pressed={tabs ? undefined : on}
            aria-label={o.ariaLabel}
            title={o.title}
            tabIndex={focusable ? 0 : -1}
            onClick={() => onChange(o.value)}
          >
            {o.icon}
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
