import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";

export interface MenuProps {
  anchor: RefObject<HTMLElement | null>;
  open: boolean;
  onClose: () => void;
  align?: "start" | "end";
  width?: number;
  label: string;
  children: ReactNode;
}

/**
 * Menu déroulant : apparition par échelle et fondu depuis son déclencheur,
 * fermeture par Échap ou clic extérieur, navigation aux flèches.
 */
export function Menu({ anchor, open, onClose, align = "start", width = 380, label, children }: MenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    if (!open || !anchor.current) return;
    const r = anchor.current.getBoundingClientRect();
    const w = Math.min(width, window.innerWidth - 32);
    let left = align === "end" ? r.right - w : r.left;
    left = Math.max(16, Math.min(left, window.innerWidth - w - 16));
    setPos({ top: r.bottom + 10, left });
  }, [open, anchor, align, width]);

  useEffect(() => {
    if (!open) return;
    const items = () => Array.from(ref.current?.querySelectorAll<HTMLElement>("[role=menuitem]:not(:disabled)") ?? []);
    const first = window.setTimeout(() => items()[0]?.focus(), 30);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
        anchor.current?.focus();
      } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const list = items();
        const i = list.indexOf(document.activeElement as HTMLElement);
        const n = (i + (e.key === "ArrowDown" ? 1 : -1) + list.length) % list.length;
        list[n]?.focus();
      } else if (e.key === "Tab") {
        onClose();
      }
    };
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (ref.current?.contains(t) || anchor.current?.contains(t)) return;
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown, true);
    return () => {
      window.clearTimeout(first);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
    };
  }, [open, onClose, anchor]);

  if (!open || !pos) return null;
  return createPortal(
    <div
      ref={ref}
      className={`menu ${align === "end" ? "menu--end" : ""}`}
      role="menu"
      aria-label={label}
      style={{ top: pos.top, left: pos.left, width }}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest("[role=menuitem]")) onClose();
      }}
    >
      {children}
    </div>,
    document.body,
  );
}

export interface MenuItemProps {
  icon?: ReactNode;
  tileBg?: string;
  tileFg?: string;
  label: string;
  desc?: string;
  kbd?: string;
  current?: boolean;
  disabled?: boolean;
  delay?: number;
  compact?: boolean;
  onSelect: () => void;
}

export function MenuItem({ icon, tileBg, tileFg, label, desc, kbd, current, disabled, delay = 0, compact, onSelect }: MenuItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      className={`menu__item ${compact ? "menu__item--compact" : ""}`}
      aria-current={current ? "page" : undefined}
      disabled={disabled}
      style={{ animationDelay: `${delay}ms` }}
      onClick={onSelect}
    >
      {icon && (
        <span className="menu__tile" style={{ background: tileBg, color: tileFg }}>
          {icon}
        </span>
      )}
      <span className="menu__text">
        <span className="menu__label">{label}</span>
        {desc && <span className="menu__desc">{desc}</span>}
      </span>
      {kbd && <span className="kbd">{kbd}</span>}
    </button>
  );
}
