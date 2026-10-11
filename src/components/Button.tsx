import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "sm" | "md" | "lg";
  /** État « activé » pour les boutons-bascules (fond d'accent léger). */
  on?: boolean;
  block?: boolean;
  icon?: ReactNode;
  kbd?: string;
}

function classes(p: Pick<ButtonProps, "variant" | "size" | "on" | "block">, extra?: string, iconOnly?: boolean) {
  return [
    "btn",
    p.variant && p.variant !== "secondary" ? `btn--${p.variant}` : "",
    p.size && p.size !== "md" ? `btn--${p.size}` : "",
    p.on ? "btn--on" : "",
    p.block ? "btn--block" : "",
    iconOnly ? "btn--icon" : "",
    extra ?? "",
  ]
    .filter(Boolean)
    .join(" ");
}

/** Bouton : principal, secondaire, fantôme ou danger. */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", on, block, icon, kbd, className, children, type = "button", ...rest },
  ref,
) {
  return (
    <button ref={ref} type={type} className={classes({ variant, size, on, block }, className)} {...rest}>
      {icon}
      {children}
      {kbd && <span className="kbd">{kbd}</span>}
    </button>
  );
});

export interface IconButtonProps extends Omit<ButtonProps, "children" | "icon"> {
  label: string;
  icon: ReactNode;
  /** Texte de l'infobulle (par défaut le libellé), raccourci éventuel. */
  tip?: string | false;
  shortcut?: string;
  tipAlign?: "center" | "end";
  tipSide?: "bottom" | "top";
}

/** Bouton icône seule, avec libellé accessible et infobulle. */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, icon, tip, shortcut, tipAlign = "center", tipSide = "bottom", variant = "ghost", size = "md", on, className, type = "button", ...rest },
  ref,
) {
  const tipText = tip === false ? null : (tip ?? label);
  const btn = (
    <button
      ref={ref}
      type={type}
      aria-label={shortcut ? `${label} (${shortcut})` : label}
      className={classes({ variant, size, on }, className, true)}
      {...rest}
    >
      {icon}
    </button>
  );
  if (!tipText) return btn;
  return (
    <span className="tip-wrap">
      {btn}
      <span className={`tip ${tipAlign === "end" ? "tip--end" : ""} ${tipSide === "top" ? "tip--top" : ""}`} aria-hidden="true">
        {tipText}
        {shortcut && <span className="kbd">{shortcut}</span>}
      </span>
    </span>
  );
});

export function Kbd({ children }: { children: ReactNode }) {
  return <span className="kbd">{children}</span>;
}

/** Infobulle autour d'un élément quelconque. */
export function Tooltip({ text, shortcut, children, align = "center" }: { text: string; shortcut?: string; children: ReactNode; align?: "center" | "end" }) {
  return (
    <span className="tip-wrap">
      {children}
      <span className={`tip ${align === "end" ? "tip--end" : ""}`} role="tooltip">
        {text}
        {shortcut && <span className="kbd">{shortcut}</span>}
      </span>
    </span>
  );
}
