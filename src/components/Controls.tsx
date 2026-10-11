import { forwardRef, type InputHTMLAttributes, type ReactNode } from "react";
import { Icons } from "./icons";

// ---------------------------------------------------------------- Interrupteur

export function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      className="sw"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}

/** Ligne « libellé + interrupteur » des panneaux de réglages. */
export function SwitchRow({ label, checked, onChange, disabled }: { label: string; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <div className="row-setting">
      <span>{label}</span>
      <Switch label={label} checked={checked} onChange={onChange} disabled={disabled} />
    </div>
  );
}

// ---------------------------------------------------------------- Pastille

export function Chip({ pressed, onClick, children, dialog, className }: { pressed: boolean; onClick: () => void; children: ReactNode; dialog?: boolean; className?: string }) {
  return (
    <button type="button" className={`chip ${dialog ? "chip--dialog" : ""} ${className ?? ""}`} aria-pressed={pressed} onClick={onClick}>
      {children}
    </button>
  );
}

// ---------------------------------------------------------------- Champ texte

export interface TextFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  icon?: ReactNode;
  pill?: boolean;
  small?: boolean;
  invalid?: boolean;
  suffix?: ReactNode;
}

/** Champ texte avec halo de focus, icône et suffixe optionnels. */
export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(function TextField(
  { icon, pill, small, invalid, suffix, className, style, ...rest },
  ref,
) {
  return (
    <span className={`field-wrap ${icon ? "field-wrap--icon" : ""}`} style={style}>
      {icon}
      <input
        ref={ref}
        className={`field ${pill ? "field--pill" : ""} ${small ? "field--sm" : ""} ${invalid ? "field--invalid" : ""} ${className ?? ""}`}
        aria-invalid={invalid || undefined}
        spellCheck={false}
        {...rest}
      />
      {suffix && <span className="field__suffix">{suffix}</span>}
    </span>
  );
});

// ---------------------------------------------------------------- Curseur

export function Slider({
  value,
  min,
  max,
  step = 1,
  onChange,
  label,
  format = (v) => String(v),
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  label: string;
  format?: (v: number) => string;
}) {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <label className="slider">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-label={label}
        style={{ ["--pct" as string]: `${pct}%` }}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <span className="slider__value">{format(value)}</span>
    </label>
  );
}

// ---------------------------------------------------------------- Pas à pas

export function Stepper({
  value,
  min = 1,
  max = 99,
  onChange,
  decLabel,
  incLabel,
}: {
  value: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
  decLabel: string;
  incLabel: string;
}) {
  return (
    <span className="stepper">
      <button type="button" className="btn btn--ghost" aria-label={decLabel} disabled={value <= min} onClick={() => onChange(Math.max(min, value - 1))}>
        <Icons.minus />
      </button>
      <span className="stepper__value" aria-live="polite">
        {value}
      </span>
      <button type="button" className="btn btn--ghost" aria-label={incLabel} disabled={value >= max} onClick={() => onChange(Math.min(max, value + 1))}>
        <Icons.plus />
      </button>
    </span>
  );
}

// ---------------------------------------------------------------- Case à cocher

/** Case à cocher dont la coche se dessine (trait animé) avec un léger rebond. */
export function Checkbox({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children?: ReactNode }) {
  return (
    <button type="button" role="checkbox" aria-checked={checked} className="cb" onClick={() => onChange(!checked)}>
      <span className="cb__box">
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M3.5 8.5l3 3 6-7" />
        </svg>
      </span>
      {children}
    </button>
  );
}
