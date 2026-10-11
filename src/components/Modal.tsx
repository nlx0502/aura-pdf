import { useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { IconButton } from "./Button";
import { Icons } from "./icons";

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  label: string;
  width?: number;
  /** Hauteur fixe (ex. fenêtre d'impression). */
  height?: string;
  /** Position haute (palette de commandes). */
  top?: boolean;
  /** Empêche la fermeture (opération en cours). */
  locked?: boolean;
  className?: string;
  children: ReactNode;
}

/**
 * Dialogue modal : fond flouté, apparition « pop », piège du focus,
 * fermeture par Échap ou clic sur le fond.
 */
export function Modal({ open, onClose, label, width = 560, height, top, locked, className, children }: ModalProps) {
  const ref = useRef<HTMLDivElement>(null);
  const lockedRef = useRef(locked);
  lockedRef.current = locked;

  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    const focusables = () =>
      Array.from(
        ref.current?.querySelectorAll<HTMLElement>(
          'button:not(:disabled), [href], input:not(:disabled), select, textarea, [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      );
    const t = window.setTimeout(() => {
      const auto = ref.current?.querySelector<HTMLElement>("[data-autofocus]");
      (auto ?? focusables()[0])?.focus();
    }, 20);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !lockedRef.current) {
        e.stopPropagation();
        e.preventDefault();
        onClose();
      } else if (e.key === "Tab") {
        const list = focusables();
        if (!list.length) return;
        const first = list[0];
        const last = list[list.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.clearTimeout(t);
      window.removeEventListener("keydown", onKey, true);
      previous?.focus?.();
    };
  }, [open, onClose]);

  if (!open) return null;
  return createPortal(
    <div
      className={`scrim ${top ? "scrim--top" : ""}`}
      onPointerDown={(e) => {
        if (e.target === e.currentTarget && !locked) onClose();
      }}
    >
      <div ref={ref} className={`dialog ${className ?? ""}`} role="dialog" aria-modal="true" aria-label={label} style={{ width, height }}>
        {children}
      </div>
    </div>,
    document.body,
  );
}

/** En-tête standard : pastille d'icône, titre, sous-titre, bouton Fermer. */
export function ModalHeader({ icon, title, subtitle, onClose }: { icon?: ReactNode; title: string; subtitle?: ReactNode; onClose?: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="dialog__head">
      {icon && <span className="dialog__badge">{icon}</span>}
      <span className="dialog__titles">
        <span className="dialog__title">{title}</span>
        {subtitle && <span className="muted">{subtitle}</span>}
      </span>
      {onClose && <IconButton label={t("common.close")} tip={false} icon={<Icons.close />} onClick={onClose} />}
    </div>
  );
}
