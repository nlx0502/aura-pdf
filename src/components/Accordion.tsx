import { useId, useState, type ReactNode } from "react";
import { Icons } from "./icons";

/** Panneau accordéon (ouverture animée en hauteur). */
export function Accordion({ title, icon, defaultOpen = false, children }: { title: string; icon?: ReactNode; defaultOpen?: boolean; children: ReactNode }) {
  const [open, setOpen] = useState(defaultOpen);
  const id = useId();
  return (
    <section className="acc" data-open={open}>
      <button type="button" className="acc__head" aria-expanded={open} aria-controls={id} onClick={() => setOpen(!open)}>
        {icon}
        {title}
        <Icons.chevronDown className="i acc__chev" />
      </button>
      <div className="acc__body" id={id} role="region" aria-hidden={!open}>
        <div className="acc__inner">
          <div className="acc__content">{children}</div>
        </div>
      </div>
    </section>
  );
}
