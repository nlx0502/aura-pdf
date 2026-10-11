import type { ReactNode } from "react";
import { Icons } from "./icons";

export function Spinner({ size = 18 }: { size?: number }) {
  return <span className="spinner" style={{ width: size, height: size }} role="progressbar" aria-busy="true" />;
}

/** Barre de progression (valeur 0–100, ou indéterminée si `value` est omis). */
export function ProgressBar({ value, thin, tone, label }: { value?: number; thin?: boolean; tone?: "accent" | "success"; label?: string }) {
  const indeterminate = value === undefined;
  return (
    <div
      className={`progress ${thin ? "progress--thin" : ""} ${indeterminate ? "progress--indeterminate" : ""}`}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : Math.round(value)}
    >
      <span className={`progress__bar ${tone === "success" ? "progress__bar--success" : ""}`} style={{ width: `${indeterminate ? 35 : value}%` }} />
    </div>
  );
}

export type StepState = "pending" | "active" | "done";

export interface StepItem {
  label: ReactNode;
  state: StepState;
  meta?: ReactNode;
}

/** Liste à étapes : en attente → en cours (spinner) → terminée (coche). */
export function StepList({ steps }: { steps: StepItem[] }) {
  return (
    <ol className="steps">
      {steps.map((s, i) => (
        <li key={i} className={`step step--${s.state}`} aria-current={s.state === "active" ? "step" : undefined}>
          <span className="step__dot">
            {s.state === "active" && <span className="spinner" aria-hidden="true" />}
            {s.state === "done" && <Icons.check size={13} strokeWidth={2.4} />}
          </span>
          <span className="step__label">{s.label}</span>
          {s.meta && <span className="step__meta">{s.meta}</span>}
        </li>
      ))}
    </ol>
  );
}
