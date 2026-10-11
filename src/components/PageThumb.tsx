import type { ReactNode } from "react";

/** Miniature de page : feuille, numéro, état « page courante ». */
export function PageThumb({
  number,
  current,
  aspect,
  onClick,
  label,
  children,
  style,
}: {
  number: number | string;
  current?: boolean;
  /** Rapport hauteur / largeur de la page. */
  aspect: number;
  onClick?: () => void;
  label: string;
  children?: ReactNode;
  style?: React.CSSProperties;
}) {
  return (
    <button type="button" className="pthumb" aria-current={current} aria-label={label} onClick={onClick} style={style}>
      <span className="pthumb__sheet" style={{ aspectRatio: `1 / ${aspect}` }}>
        {children ?? <span className="skeleton" style={{ position: "absolute", inset: 0 }} />}
      </span>
      <span className="pthumb__num">{number}</span>
    </button>
  );
}
