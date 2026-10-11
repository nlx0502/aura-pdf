import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { Icons } from "../components";

/** Boutons Réduire / Agrandir / Fermer (la fenêtre n'a pas de barre de titre système). */
export function WindowControls() {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const w = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    void w.isMaximized().then(setMaximized).catch(() => {});
    void w
      .onResized(() => {
        void w.isMaximized().then(setMaximized).catch(() => {});
      })
      .then((u) => (unlisten = u))
      .catch(() => {});
    return () => unlisten?.();
  }, []);

  const w = () => getCurrentWindow();
  return (
    <div className="winctl" role="group" aria-label="Fenêtre">
      <button type="button" className="winctl__btn" aria-label={t("window.minimize")} title={t("window.minimize")} onClick={() => void w().minimize()}>
        <Icons.minus size={15} />
      </button>
      <button
        type="button"
        className="winctl__btn"
        aria-label={maximized ? t("window.restore") : t("window.maximize")}
        title={maximized ? t("window.restore") : t("window.maximize")}
        onClick={() => void w().toggleMaximize()}
      >
        {maximized ? <Icons.winRestore size={13} /> : <Icons.winMaximize size={12} />}
      </button>
      <button type="button" className="winctl__btn winctl__btn--close" aria-label={t("window.close")} title={t("window.close")} onClick={() => void w().close()}>
        <Icons.close size={15} />
      </button>
    </div>
  );
}
