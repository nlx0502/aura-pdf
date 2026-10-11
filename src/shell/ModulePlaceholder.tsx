import { useTranslation } from "react-i18next";
import { Button, Icons } from "../components";
import { useApp, type ModuleId } from "../stores/app";

const INFO: Record<Exclude<ModuleId, "lire">, { icon: keyof typeof Icons; milestone: number; bg: string; fg: string; label: string }> = {
  annoter: { icon: "annotate", milestone: 3, bg: "var(--warning-soft)", fg: "var(--warning-text)", label: "nav.annoter" },
  editer: { icon: "edit", milestone: 4, bg: "var(--accent-soft)", fg: "var(--accent-text)", label: "nav.editer" },
  signer: { icon: "sign", milestone: 6, bg: "var(--success-soft)", fg: "var(--success)", label: "nav.signer" },
  securite: { icon: "security", milestone: 7, bg: "var(--info-soft)", fg: "var(--info-text)", label: "nav.securite" },
  fusion: { icon: "merge", milestone: 5, bg: "var(--accent-soft)", fg: "var(--accent-text)", label: "tools.fusion.label" },
  forms: { icon: "forms", milestone: 7, bg: "var(--info-soft)", fg: "var(--info-text)", label: "tools.forms.label" },
  compress: { icon: "compress", milestone: 5, bg: "var(--success-soft)", fg: "var(--success)", label: "tools.compress.label" },
  convert: { icon: "convert", milestone: 8, bg: "var(--warning-soft)", fg: "var(--warning-text)", label: "tools.convert.label" },
};

/** Écran d'attente des modules prévus aux jalons suivants. */
export function ModulePlaceholder({ module }: { module: Exclude<ModuleId, "lire"> }) {
  const { t } = useTranslation();
  const setModule = useApp((s) => s.setModule);
  const hasDoc = useApp((s) => s.tabs.length > 0);
  const goHome = useApp((s) => s.goHome);
  const info = INFO[module];
  const I = Icons[info.icon];
  return (
    <div className="workspace">
      <div className="soon">
        <div className="soon__card rise" key={module}>
          <span className="soon__tile" style={{ background: info.bg, color: info.fg }}>
            <I />
          </span>
          <h1>{t("placeholder.title", { name: t(info.label) })}</h1>
          <p className="muted" style={{ fontSize: 14 }}>
            {t("placeholder.body", { n: info.milestone })}
          </p>
          <Button variant="primary" onClick={() => (hasDoc ? setModule("lire") : goHome())}>
            {hasDoc ? t("placeholder.back") : t("app.homeLink")}
          </Button>
        </div>
      </div>
    </div>
  );
}
